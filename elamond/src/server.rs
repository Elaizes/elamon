//! The HTTPS API clients talk to: docs/protocol.md, API 2. Pairing, keys of their own and the
//! certificate's rotations here, with `GET /api/v1/ui` and snapshots; the stream is in `stream`,
//! acting in `control`.

use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime};

use axum::extract::{ConnectInfo, DefaultBodyLimit, Extension, Query, Request, State};
use axum::http::header;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::watch;

use crate::access::{Caller, RateLimiter, Role};
use crate::actions::Caps;
use crate::activity::Activity;
use crate::consts::{cadence, limit, protocol, timeout};
use crate::control::Acted;
use crate::errors::ErrorCode;
use crate::history::Ring;
use crate::store::{self, Attempt};
use crate::stream::{self, Open};
use crate::ui::{self, Sources, Ui};
use crate::values::Values;

/// What the collector publishes every tick.
pub struct Frame {
	/// Counts the readings.
	pub seq: u64,
	/// Every value of the reading.
	pub values: Arc<Values>,
	/// What in the reading a client's `ui` may depend on, hashed: the addresses that have values,
	/// the keys of each collection, and what the computer can do.
	pub shape: u64,
	/// What the computer can do, for the built-in actions.
	pub caps: Arc<Caps>,
	/// The costlier sources read afresh for this reading.
	pub read: Sources,
	/// `hello`'s `features` but `control`, which the settings decide when it is asked.
	pub features: Arc<Vec<&'static str>>,
}

impl Frame {
	/// Before the first reading.
	pub fn empty() -> Frame {
		Frame {
			seq: 0,
			values: Arc::new(Values::default()),
			shape: 0,
			caps: Arc::new(Caps::default()),
			read: Sources::default(),
			features: Arc::new(vec!["processes"]),
		}
	}
}

/// What in `values` and `caps` a client's `ui` may depend on, hashed: which addresses have values,
/// the items of each collection, and what the computer can do. A reading with the same shape as
/// the one before needs no new `ui`, unless what the strings hold changed (see `Hub::ui`).
pub fn shape_of(values: &Values, caps: &Caps) -> u64 {
	let mut h = std::collections::hash_map::DefaultHasher::new();
	for a in values.scalars.keys() {
		a.hash(&mut h);
	}
	for (name, items) in &values.collections {
		name.hash(&mut h);
		for i in items {
			i.key.hash(&mut h);
		}
	}
	values.power_profiles.hash(&mut h);
	format!("{caps:?}").hash(&mut h);
	h.finish()
}

/// What decides a client's `ui`: the same for two clients, the same document.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UiKey {
	pub role: Role,
	/// The profile it gets: its own, or `default_profile`.
	pub profile: String,
	/// `android`, later `web`.
	pub kind: String,
	/// The widget types it draws; `None` for all of them.
	pub draws: Draws,
}

/// A `ui` as built, and what it was built from.
struct CachedUi {
	stamp: (u64, u64, Option<&'static str>, u64),
	ui: Arc<Ui>,
	used: Instant,
}

/// What the client says it is: `Elamon-Client: <kind>/<version>`. Without it, `android` of an
/// unknown version.
#[derive(Clone, Debug, PartialEq)]
pub struct ClientApp {
	pub kind: String,
	pub version: Option<String>,
}

impl ClientApp {
	pub fn parse(header: Option<&str>) -> ClientApp {
		let unknown = ClientApp { kind: "android".into(), version: None };
		let Some(h) = header.map(str::trim).filter(|h| !h.is_empty()) else { return unknown };
		let (kind, version) = h.split_once('/').unwrap_or((h, ""));
		let kind = kind.trim().to_ascii_lowercase();
		let version = version.trim();
		let kind_ok = (1..=16).contains(&kind.len()) && kind.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-');
		if !kind_ok {
			return unknown;
		}
		let version_ok = (1..=32).contains(&version.len()) && version.bytes().all(|b| b.is_ascii_alphanumeric() || b".+-_".contains(&b));
		ClientApp { kind, version: version_ok.then(|| version.to_string()) }
	}

	/// "android/0.6.0", as clients.json and `elamond clients` have it; none of an unknown version.
	pub fn label(&self) -> Option<String> {
		self.version.as_ref().map(|v| format!("{}/{v}", self.kind))
	}
}

pub struct Hub {
	pub id: String,
	pub name: String,
	pub frame: watch::Receiver<Arc<Frame>>,
	/// Every value a graph or a spark shows, over the last `history_minutes`.
	pub history: Mutex<Ring>,
	pub interval: Duration,
	/// Swapped whole when the settings files change; see `config()`.
	config: RwLock<Arc<store::Config>>,
	/// Counts the settings taken up, and the times they stopped parsing: a `ui` built before is stale.
	config_gen: AtomicU64,
	pub bus: crate::bus::Bus,
	/// Set after an action, so the next tick re-reads what it may have changed.
	pub refresh: AtomicBool,
	clients: RwLock<ClientCache>,
	pair_lock: tokio::sync::Mutex<()>,
	/// The data directory: the paired phones, the activity log and `elamond control off`'s file.
	pub dir: PathBuf,
	/// `elamond control off` holds control off, as last looked.
	control_off: AtomicBool,
	/// Why the settings files don't parse, while they don't: control is off meanwhile.
	settings_broken: Mutex<Option<String>>,
	/// How often each phone may act.
	pub rate: Mutex<RateLimiter>,
	pub activity: Activity,
	/// Unix time the computer booted, as read when the service started: process start times
	/// count from it.
	pub boot_time: u64,
	/// Each phone's streams, for the journal.
	stream_notes: Mutex<HashMap<String, StreamNotes>>,
	/// The certificate rotations that lead to the certificate presented now, as served.
	rotations: RwLock<Arc<String>>,
	/// What the clients looking now need read, by who asked: streams and snapshots.
	wants: Mutex<HashMap<u64, Sources>>,
	next_want: AtomicU64,
	/// Each `ui` built lately, by what decides it.
	uis: Mutex<HashMap<UiKey, CachedUi>>,
	/// The open streams, by id.
	pub(crate) streams: Mutex<HashMap<String, Open>>,
	/// The app each client said it is last ("android/0.6.0"), by its id.
	apps: Mutex<HashMap<String, String>>,
	/// The widget types each client said it draws in its last `GET /api/v1/ui`, by its id and kind:
	/// what its streams describe with.
	draws: Mutex<HashMap<(String, String), Draws>>,
}

/// The widget types a client draws; `None` for all of them.
pub type Draws = Option<BTreeSet<String>>;

/// The hash of the token a request came with, so a stream can tell when it is revoked.
#[derive(Clone)]
pub struct TokenHash(pub String);

struct ClientCache {
	stamp: Option<(SystemTime, u64, u64)>,
	/// Token hashes and the phones they belong to.
	clients: Vec<(String, Caller)>,
}

impl Hub {
	/// The hub for a service whose state is in `dir`.
	pub fn new(
		id: String,
		name: String,
		frame: watch::Receiver<Arc<Frame>>,
		history: Ring,
		interval: Duration,
		config: Arc<store::Config>,
		dir: PathBuf,
	) -> Self {
		Hub {
			id,
			name,
			frame,
			history: Mutex::new(history),
			interval,
			config: RwLock::new(config),
			config_gen: AtomicU64::new(0),
			bus: crate::bus::Bus::new(),
			refresh: AtomicBool::new(false),
			clients: RwLock::new(ClientCache { stamp: None, clients: Vec::new() }),
			pair_lock: tokio::sync::Mutex::new(()),
			control_off: AtomicBool::new(store::control_held_off(&dir)),
			settings_broken: Mutex::new(None),
			rate: Mutex::new(RateLimiter::default()),
			activity: Activity::load(&dir),
			boot_time: sysinfo::System::boot_time().unwrap_or(0),
			stream_notes: Mutex::new(HashMap::new()),
			rotations: RwLock::new(Arc::new("[]".into())),
			wants: Mutex::new(HashMap::new()),
			next_want: AtomicU64::new(1),
			uis: Mutex::new(HashMap::new()),
			streams: Mutex::new(HashMap::new()),
			apps: Mutex::new(HashMap::new()),
			draws: Mutex::new(HashMap::new()),
			dir,
		}
	}

	/// The phone a token's hash belongs to, rereading clients.json when `pair`, `revoke` or
	/// `clients role` changed it. A file that stops parsing is reported and the phones read
	/// before keep working, rather than all being locked out.
	pub fn authorized(&self, hash: &str) -> Option<Caller> {
		let stamp = store::clients_stamp_in(&self.dir);
		let stale = self.clients.read().map(|c| c.stamp != stamp || stamp.is_none()).unwrap_or(true);
		if stale && let Ok(mut c) = self.clients.write() {
			match store::load_clients_in(&self.dir) {
				Ok(list) => {
					let fresh: Vec<(String, Caller)> = list
						.iter()
						.map(|c| {
							let family = store::family_of(&list, &c.id);
							// Cleaned again: the file may hold what doesn't show, whoever wrote it.
							let who = Caller {
								id: c.id.clone(),
								name: store::sanitize_name(&c.name),
								role: c.role,
								profile: c.profile.clone(),
								app: c.app.clone(),
								family,
							};
							(c.token_hash.clone(), who)
						})
						.collect();
					for (_, now) in &fresh {
						if let Some((_, before)) = c.clients.iter().find(|(_, b)| b.id == now.id) {
							if before.role != now.role {
								eprintln!("clients: \"{}\" ({}) is now {}", now.name, now.id, now.role.as_str());
							}
							if before.profile != now.profile {
								let p = now.profile.as_deref().unwrap_or("the default profile");
								eprintln!("clients: \"{}\" ({}) now has {p}", now.name, now.id);
							}
						}
					}
					c.clients = fresh;
				}
				Err(e) if c.stamp != stamp => {
					eprintln!("{e}\n  keeping the {} paired phone(s) read before; fix the file or pair again", c.clients.len());
				}
				Err(_) => {}
			}
			c.stamp = stamp;
		}
		self.clients
			.read()
			.ok()?
			.clients
			.iter()
			.find(|(h, _)| store::same(h.as_bytes(), hash.as_bytes()))
			.map(|(_, who)| who.clone())
	}

	/// Notes the app a client says it is; clients.json keeps the last of each, written only when it
	/// changes, for `elamond clients`.
	fn saw(&self, who: &Caller, app: &ClientApp) {
		let Some(label) = app.label() else { return };
		if who.app.as_deref() == Some(label.as_str()) {
			return;
		}
		{
			let mut apps = self.apps.lock().unwrap_or_else(|p| p.into_inner());
			if apps.get(&who.id) == Some(&label) {
				return;
			}
			apps.insert(who.id.clone(), label.clone());
		}
		let (dir, id, name) = (self.dir.clone(), who.id.clone(), who.name.clone());
		tokio::task::spawn_blocking(move || {
			if let Err(e) = store::set_client_app(&dir, &id, &label) {
				eprintln!("clients: can't note that \"{name}\" ({id}) is {label}: {e}");
			}
		});
	}

	/// The settings as last read.
	pub fn config(&self) -> Arc<store::Config> {
		self.config.read().map(|c| c.clone()).unwrap_or_else(|p| p.into_inner().clone())
	}

	/// Whether phones may act, as last looked: `control = true` in the settings, settings that
	/// parse, and no `elamond control off`.
	pub fn control_on(&self) -> bool {
		self.off_reason().is_none()
	}

	/// Why control is off, as last looked, if it is: the settings stopped parsing
	/// (`settings_unreadable`), `elamond control off` (`held_off`), or `control = false` (`settings`).
	pub fn off_reason(&self) -> Option<&'static str> {
		if self.settings_broken.lock().map(|b| b.is_some()).unwrap_or(true) {
			Some("settings_unreadable")
		} else if self.control_off.load(Ordering::SeqCst) {
			Some("held_off")
		} else if !self.config().control {
			Some("settings")
		} else {
			None
		}
	}

	/// The settings files no longer parse (`why`): the settings read before stay, but control is
	/// off until they parse again, so a half-written file can't leave it on.
	pub fn settings_failed(&self, why: String) {
		let mut broken = self.settings_broken.lock().unwrap_or_else(|p| p.into_inner());
		if broken.as_deref() != Some(why.as_str()) {
			let was = if self.config().control { "; control is off until they parse again" } else { "" };
			eprintln!("settings: they don't parse, so the ones read before stay{was}:\n  {why}");
			*broken = Some(why);
			self.config_gen.fetch_add(1, Ordering::SeqCst);
			self.refresh_soon();
		}
	}

	/// Looks at `elamond control off`'s file, says in the log when that changed, and returns
	/// whether control is held off.
	pub fn sync_control_off(&self) -> bool {
		let off = store::control_held_off(&self.dir);
		if self.control_off.swap(off, Ordering::SeqCst) != off {
			if off {
				eprintln!("control: held off by `elamond control off`; no phone acts until `elamond control on`");
			} else {
				let by = if self.config().control { "on" } else { "off" };
				eprintln!("control: `elamond control on` let go of it; control is {by} by the settings");
			}
			self.refresh_soon();
		}
		off
	}

	/// Takes newly read settings. What only applies at start (address, port, name, mDNS,
	/// interval, history) is reported and left as it was.
	pub fn set_config(&self, new: store::Config) {
		let old = self.config();
		let restart: Vec<&str> = [
			("listen", old.listen != new.listen),
			("port", old.port != new.port),
			("name", old.name != new.name),
			("mdns", old.mdns != new.mdns),
			("interval_ms", old.interval_ms != new.interval_ms),
			("history_minutes", old.history_minutes != new.history_minutes),
		]
		.into_iter()
		.filter_map(|(k, changed)| changed.then_some(k))
		.collect();
		if !restart.is_empty() {
			eprintln!("settings: {} take effect after a restart", restart.join(", "));
		}
		if old.control != new.control {
			let held = if new.control && self.control_off.load(Ordering::SeqCst) { ", but `elamond control off` holds it off" } else { "" };
			eprintln!("settings: control is now {}{held}", if new.control { "on" } else { "off" });
		}
		if old.default_role != new.default_role {
			eprintln!("settings: phones pair as {} from now on", new.default_role.as_str());
		}
		let control = new.control;
		if let Ok(mut c) = self.config.write() {
			*c = Arc::new(new);
		}
		if self.settings_broken.lock().unwrap_or_else(|p| p.into_inner()).take().is_some() {
			eprintln!("settings: they parse again; control is {} by them", if control { "on" } else { "off" });
		}
		self.config_gen.fetch_add(1, Ordering::SeqCst);
		self.refresh_soon();
	}

	/// Counts the settings taken up, and the times they stopped parsing.
	pub fn config_gen(&self) -> u64 {
		self.config_gen.load(Ordering::SeqCst)
	}

	/// The rotations to serve from now on: those leading to the certificate presented.
	pub fn set_rotations(&self, chain: &[crate::identity::Rotation]) {
		let json = serde_json::to_string(chain).unwrap_or_else(|_| "[]".into());
		if let Ok(mut r) = self.rotations.write() {
			*r = Arc::new(json);
		}
	}

	/// Asks the collector to re-read links, profiles, the volume and what the computer can do on
	/// its next tick.
	pub fn refresh_soon(&self) {
		self.refresh.store(true, Ordering::SeqCst);
	}

	/// Asks for `sources` to be read for as long as the answer is held.
	pub fn want(self: &Arc<Self>, sources: Sources) -> Want {
		let id = self.next_want.fetch_add(1, Ordering::SeqCst);
		self.wants.lock().unwrap_or_else(|p| p.into_inner()).insert(id, sources);
		Want { hub: self.clone(), id }
	}

	/// What the clients looking now need read; none when nobody looks.
	pub fn wanted(&self) -> Option<Sources> {
		let wants = self.wants.lock().unwrap_or_else(|p| p.into_inner());
		(!wants.is_empty()).then(|| wants.values().fold(Sources::default(), |a, b| a.union(*b)))
	}

	/// Clients looking now: streams and snapshots.
	#[cfg(test)]
	pub fn watchers(&self) -> usize {
		self.wants.lock().map(|w| w.len()).unwrap_or(0)
	}

	/// The service as `ui` names it.
	fn service(&self) -> ui::Service {
		let cap = self.history.lock().map(|h| h.capacity()).unwrap_or(0);
		ui::Service {
			id: self.id.clone(),
			name: self.name.clone(),
			version: env!("CARGO_PKG_VERSION").into(),
			interval_ms: self.interval.as_millis() as u64,
			history_seconds: (self.interval * cap as u32).as_secs(),
		}
	}

	/// What decides the `ui` of `who`, a client of `kind` that draws `draws`.
	pub fn ui_key(&self, who: &Caller, kind: &str, draws: Draws) -> UiKey {
		let config = self.config();
		let asked = who.profile.as_deref().unwrap_or(&config.default_profile);
		UiKey { role: who.role, profile: config.profile(asked).id, kind: kind.to_string(), draws }
	}

	/// The widget types the client `id` of `kind` said it draws last; all, when it never said.
	pub fn draws_of(&self, id: &str, kind: &str) -> Draws {
		self.draws.lock().ok().and_then(|d| d.get(&(id.to_string(), kind.to_string())).cloned()).flatten()
	}

	fn remember_draws(&self, id: &str, kind: &str, draws: Draws) {
		if let Ok(mut d) = self.draws.lock() {
			d.insert((id.to_string(), kind.to_string()), draws);
		}
	}

	/// The `ui` of clients with `key`, from the reading `frame`: the one built before while nothing
	/// it depends on changed, and at least every `cadence::UI_REBUILD` while the strings it fills
	/// in may have.
	pub fn ui(&self, key: &UiKey, frame: &Frame) -> Arc<Ui> {
		let ticks = (cadence::UI_REBUILD.as_millis() / self.interval.as_millis().max(1)).max(1) as u64;
		let stamp = (frame.shape, self.config_gen(), self.off_reason(), frame.seq / ticks);
		let now = Instant::now();
		if let Some(c) = self.uis.lock().unwrap_or_else(|p| p.into_inner()).get_mut(key)
			&& c.stamp == stamp
		{
			c.used = now;
			return c.ui.clone();
		}
		let config = self.config();
		let off_reason = stamp.2;
		let built = Arc::new(ui::build(&ui::Input {
			config: &config,
			values: &frame.values,
			caps: &frame.caps,
			service: self.service(),
			control: ui::Control { enabled: off_reason.is_none(), off_reason },
			role: key.role,
			profile: &key.profile,
			kind: &key.kind,
			draws: key.draws.as_ref(),
		}));
		let mut uis = self.uis.lock().unwrap_or_else(|p| p.into_inner());
		uis.retain(|_, c| now.duration_since(c.used) < cadence::UI_FORGET);
		uis.insert(key.clone(), CachedUi { stamp, ui: built.clone(), used: now });
		built
	}

	/// Notes a stream opening; it is logged unless another of the phone's was a moment ago.
	/// Returns whether it was, so its closing is logged too.
	pub(crate) fn stream_opened(&self, who: &Caller, from: IpAddr, what: &str) -> bool {
		let now = Instant::now();
		let mut map = self.stream_notes.lock().unwrap_or_else(|p| p.into_inner());
		map.retain(|id, n| *id == who.id || !n.idle(now));
		let n = map.entry(who.id.clone()).or_default();
		let Some(more) = n.open(now) else { return false };
		eprintln!("stream: \"{}\" ({}) opened from {}: {what}{more}", who.name, who.role.as_str(), from.to_canonical());
		true
	}

	pub(crate) fn stream_closed(&self, who: &Caller, logged: bool, lasted: Duration, why: &str) {
		let mut map = self.stream_notes.lock().unwrap_or_else(|p| p.into_inner());
		let n = map.entry(who.id.clone()).or_default();
		if let Some(more) = n.close(logged) {
			eprintln!("stream: \"{}\" closed after {}: {why}{more}", who.name, duration_words(lasted));
		}
	}
}

/// Sources asked for, until dropped.
pub struct Want {
	hub: Arc<Hub>,
	id: u64,
}

impl Want {
	/// Asks for other sources from now on.
	pub fn set(&self, sources: Sources) {
		self.hub.wants.lock().unwrap_or_else(|p| p.into_inner()).insert(self.id, sources);
	}
}

impl Drop for Want {
	fn drop(&mut self) {
		self.hub.wants.lock().unwrap_or_else(|p| p.into_inner()).remove(&self.id);
	}
}

/// A phone's streams as the journal has heard of them: when one was last logged opening, and
/// how many opened and closed since without a line of their own.
#[derive(Default)]
struct StreamNotes {
	logged: Option<Instant>,
	opened: u32,
	closed: u32,
}

impl StreamNotes {
	/// A stream opens at `now`: what to add to its line if it is logged, or nothing if it isn't.
	fn open(&mut self, now: Instant) -> Option<String> {
		if self.logged.is_some_and(|t| now.duration_since(t) < cadence::STREAM_LOG_QUIET) {
			self.opened += 1;
			return None;
		}
		self.logged = Some(now);
		Some(self.more())
	}

	/// A stream closes: what to add to its line if it is logged (because its opening was).
	fn close(&mut self, logged: bool) -> Option<String> {
		if !logged {
			self.closed += 1;
			return None;
		}
		Some(self.more())
	}

	/// The streams that opened and closed without a line, counted since the last line.
	fn more(&mut self) -> String {
		let (o, c) = (std::mem::take(&mut self.opened), std::mem::take(&mut self.closed));
		if o + c == 0 { String::new() } else { format!(" (and {o} more opened, {c} closed, since the last line)") }
	}

	/// Nothing to tell and nothing to hold back: the phone can be forgotten.
	fn idle(&self, now: Instant) -> bool {
		self.opened + self.closed == 0 && self.logged.is_none_or(|t| now.duration_since(t) >= cadence::STREAM_LOG_QUIET)
	}
}

/// "45 s", "3 min 20 s", "2 h 5 min".
fn duration_words(d: Duration) -> String {
	let s = d.as_secs();
	match s {
		0..60 => format!("{s} s"),
		60..3600 => format!("{} min {} s", s / 60, s % 60),
		_ => format!("{} h {} min", s / 3600, s % 3600 / 60),
	}
}

pub fn router(hub: Arc<Hub>) -> Router {
	let body = || DefaultBodyLimit::max(limit::BODY_MAX);
	let private = Router::new()
		.route("/api/v1/ui", get(ui_doc))
		.route("/api/v1/stream", get(stream::open))
		.route("/api/v1/stream/{id}", post(stream::change).layer(body()))
		.route("/api/v1/snapshot", get(snapshot))
		.route("/api/v1/actions/{id}", post(crate::control::action).layer(body()))
		.route("/api/v1/media/art", get(crate::control::art))
		.route("/api/v1/rekey", post(rekey).layer(body()))
		.route_layer(middleware::from_fn_with_state(hub.clone(), auth));
	Router::new()
		.route("/api/v1/hello", get(hello))
		.route("/api/v1/pair", post(pair).layer(body()))
		.route("/api/v1/identity/rotations", get(rotations))
		.merge(private)
		.fallback(|| async { error(ErrorCode::NotFound) })
		.with_state(hub)
}

pub fn error(code: ErrorCode) -> Response {
	(code.status(), Json(json!({ "error": code.as_str() }))).into_response()
}

/// An error with a word on it: what failed (`failed`), or which parameter is wrong (`bad_param`).
pub fn error_with(code: ErrorCode, message: impl Into<String>) -> Response {
	(code.status(), Json(json!({ "error": code.as_str(), "message": message.into() }))).into_response()
}

/// A body that isn't the JSON asked for: plain text, as axum answers what it can't read.
pub fn unreadable(e: impl std::fmt::Display) -> Response {
	(axum::http::StatusCode::BAD_REQUEST, format!("Failed to parse the request body as JSON: {e}")).into_response()
}

/// A JSON body of `T`; an empty one is `T`'s default.
pub fn body_of<T: serde::de::DeserializeOwned + Default>(bytes: &[u8]) -> Result<T, Box<Response>> {
	if bytes.iter().all(u8::is_ascii_whitespace) {
		return Ok(T::default());
	}
	serde_json::from_slice(bytes).map_err(|e| Box::new(unreadable(e)))
}

/// The token from an `Authorization: Bearer <token>` header; the scheme's case doesn't matter.
fn bearer(header: &str) -> Option<&str> {
	let (scheme, token) = header.trim_start().split_once(' ')?;
	let token = token.trim();
	(scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then_some(token)
}

/// Every request behind a token: the phone it is, and the app it says it is.
async fn auth(State(hub): State<Arc<Hub>>, mut req: Request, next: Next) -> Response {
	let hash = req
		.headers()
		.get(header::AUTHORIZATION)
		.and_then(|v| v.to_str().ok())
		.and_then(bearer)
		.map(|t| store::sha256_hex(t.as_bytes()));
	match hash.and_then(|h| Some((hub.authorized(&h)?, h))) {
		Some((who, hash)) => {
			// The connection is a phone's: it no longer counts among its address's strangers.
			if let Some(a) = req.extensions().get::<crate::listen::Admission>() {
				a.known();
			}
			let app = ClientApp::parse(req.headers().get(protocol::CLIENT_HEADER).and_then(|v| v.to_str().ok()));
			hub.saw(&who, &app);
			req.extensions_mut().insert(who);
			req.extensions_mut().insert(app);
			req.extensions_mut().insert(TokenHash(hash));
			next.run(req).await
		}
		None => error(ErrorCode::Unauthorized),
	}
}

/// Lets a client check that an address is an Elamon service, which one, and what it can do,
/// before pairing.
async fn hello(State(hub): State<Arc<Hub>>) -> Response {
	let features = hub.frame.borrow().features.clone();
	Json(hello_body(&hub.id, &hub.name, &features, hub.config().control)).into_response()
}

/// `features`: what the computer offers, `control` when the settings turn it on, sorted.
pub(crate) fn hello_body(id: &str, name: &str, features: &[&'static str], control: bool) -> serde_json::Value {
	let mut f: Vec<&str> = features.to_vec();
	if control {
		f.push("control");
	}
	f.sort_unstable();
	f.dedup();
	json!({
		"app": "elamon",
		"api": protocol::API_VERSION,
		"widgets": protocol::WIDGETS_VERSION,
		"id": id,
		"name": name,
		"version": env!("CARGO_PKG_VERSION"),
		"min_app": protocol::MIN_APP,
		"features": f,
	})
}

pub(crate) fn paired_body(token: &str, client_id: &str, id: &str, name: &str, role: Role, profile: &str) -> serde_json::Value {
	json!({ "token": token, "client_id": client_id, "id": id, "name": name, "role": role, "profile": profile })
}

/// The profile a client with `profile` in clients.json gets: its own, or `default_profile`.
fn profile_of(hub: &Hub, profile: Option<&str>) -> String {
	let config = hub.config();
	config.profile(profile.unwrap_or(&config.default_profile)).id
}


#[derive(Deserialize)]
struct PairRequest {
	code: String,
	#[serde(default)]
	device: String,
}

/// How an attempt at the pending code was judged.
enum Judged {
	/// No code is pending, or a phone that gave it waits for an answer.
	Nothing,
	Blocked,
	Wrong,
	/// The right code: the phone gets `role` and `profile`, at once or, with `ask`, once let in.
	Right { code: String, role: Role, profile: Option<String>, ask: Option<String> },
	/// The pending code couldn't be saved with the phone's request in it.
	Unsaved,
}

/// Judges an attempt with `code` from `ip` by a phone named `name` (cleaned), counting a wrong one
/// and, for a code from `elamond pair` without `--yes`, putting the phone's request in it for the
/// person there to answer. Called with the pairing lock held: one attempt is judged at a time.
fn judge(dir: &std::path::Path, code: &str, name: &str, ip: IpAddr) -> Judged {
	let Some(mut pending) = store::load_pairing(dir) else { return Judged::Nothing };
	// Spoken for: the phone that gave it is waiting to be let in.
	if pending.ask.is_some() {
		return Judged::Nothing;
	}
	let from = store::address_key(ip);
	match pending.attempt(&from, code) {
		Attempt::Blocked => Judged::Blocked,
		Attempt::Wrong => {
			if pending.attempts >= limit::PAIRING_ATTEMPTS {
				store::clear_pairing(dir);
				eprintln!("pairing: wrong code from {ip}; the code is used up");
			} else {
				let _ = store::save_pairing(dir, &pending);
				let tried = pending.wrong.get(&from).copied().unwrap_or(0);
				if tried >= limit::PAIRING_ATTEMPTS_PER_ADDRESS {
					eprintln!("pairing: wrong code from {ip}; {from} may not try this code again");
				} else {
					eprintln!("pairing: wrong code from {ip}");
				}
			}
			Judged::Wrong
		}
		Attempt::Right if !pending.confirm => Judged::Right { code: pending.code, role: pending.role, profile: pending.profile, ask: None },
		Attempt::Right => {
			let id = data_encoding::HEXLOWER.encode(&store::random_bytes::<8>());
			pending.ask = Some(store::Ask { id: id.clone(), name: name.to_string(), address: ip.to_string(), at: store::now(), allow: None });
			if let Err(e) = store::save_pairing(dir, &pending) {
				eprintln!("pairing: can't save the request of \"{name}\" from {ip}: {e}");
				return Judged::Unsaved;
			}
			eprintln!("pairing: \"{name}\" from {ip} gave the right code; asking at elamond pair whether to let it in");
			Judged::Right { code: pending.code, role: pending.role, profile: pending.profile, ask: Some(id) }
		}
	}
}

/// Withdraws the code a phone waits with when its request goes away unanswered (it hung up), so
/// it can't be let in afterwards with no one to tell.
struct Waiting {
	dir: PathBuf,
	code: String,
	ask: Option<String>,
}

impl Drop for Waiting {
	fn drop(&mut self) {
		if let Some(id) = self.ask.take()
			&& store::read_pairing(&self.dir).is_some_and(|p| p.code == self.code && p.ask.is_some_and(|a| a.id == id))
		{
			store::clear_pairing_code(&self.dir, &self.code);
			eprintln!("pairing: the phone that gave the right code hung up before it was let in; the code no longer works");
		}
	}
}

/// Waits for the person at `elamond pair` to answer the phone `ask` waiting with `code`: whether
/// to let it in, or why not.
async fn answer(dir: &std::path::Path, code: &str, ask: &str) -> Result<(), &'static str> {
	let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout::PAIR_APPROVAL_SECS);
	loop {
		tokio::time::sleep(cadence::PAIR_POLL).await;
		let Some(p) = store::read_pairing(dir).filter(|p| p.code == code) else {
			return Err("elamond pair withdrew the code");
		};
		match p.ask.filter(|a| a.id == ask).map(|a| a.allow) {
			None => return Err("elamond pair withdrew the code"),
			Some(Some(true)) => return Ok(()),
			Some(Some(false)) => return Err("turned away at elamond pair"),
			Some(None) if !store::pairing_answered(dir) => return Err("elamond pair isn't running to answer"),
			Some(None) if tokio::time::Instant::now() >= deadline => return Err("no answer at elamond pair in time"),
			Some(None) => {}
		}
	}
}

async fn pair(
	State(hub): State<Arc<Hub>>,
	ConnectInfo(peer): ConnectInfo<SocketAddr>,
	Json(req): Json<PairRequest>,
) -> Response {
	let ip = peer.ip().to_canonical();
	let code: String = req.code.chars().filter(char::is_ascii_digit).collect();
	let name = store::sanitize_name(&req.device);
	// Without a pending code there's nothing to judge; the lock isn't taken for it, nor held
	// while anyone waits, so one attempt can't hold up the others.
	let judged = if store::load_pairing(&hub.dir).is_none() {
		Judged::Nothing
	} else {
		let _guard = hub.pair_lock.lock().await;
		judge(&hub.dir, &code, &name, ip)
	};
	let (code, role, profile, ask) = match judged {
		Judged::Nothing => {
			tokio::time::sleep(timeout::NO_PAIRING_DELAY).await;
			return error(ErrorCode::NoPairing);
		}
		// Answered at once: it may not try again whatever it sends.
		Judged::Blocked => return error(ErrorCode::TooManyRequests),
		Judged::Wrong => {
			tokio::time::sleep(timeout::WRONG_CODE_DELAY).await;
			return error(ErrorCode::WrongCode);
		}
		Judged::Unsaved => return error(ErrorCode::Storage),
		Judged::Right { code, role, profile, ask } => (code, role, profile, ask),
	};
	let mut waiting = Waiting { dir: hub.dir.clone(), code: code.clone(), ask: ask.clone() };
	if let Some(ask) = &ask
		&& let Err(why) = answer(&hub.dir, &code, ask).await
	{
		waiting.ask = None;
		store::clear_pairing_code(&hub.dir, &code);
		eprintln!("pairing: \"{name}\" from {ip} not added: {why}; the code no longer works");
		return error(ErrorCode::PairDeclined);
	}
	waiting.ask = None;
	// The phone is added before the code is withdrawn: `elamond pair` takes a code gone with
	// no new phone for one used up by wrong attempts.
	let (dir, device) = (hub.dir.clone(), name.clone());
	let added = tokio::task::spawn_blocking(move || store::add_client_in(&dir, &device, role, profile.as_deref()))
		.await
		.unwrap_or_else(|e| Err(store::NotAdded::Io(std::io::Error::other(e))));
	match added {
		Ok((client, token)) => {
			store::clear_pairing_code(&hub.dir, &code);
			let profile = profile_of(&hub, client.profile.as_deref());
			eprintln!("pairing: added \"{}\" ({}) as {} with the profile {profile} from {ip}", client.name, client.id, client.role.as_str());
			Json(paired_body(&token, &client.id, &hub.id, &hub.name, client.role, &profile)).into_response()
		}
		Err(e) => {
			store::clear_pairing_code(&hub.dir, &code);
			eprintln!("pairing: \"{name}\" from {ip} not added: {e}");
			error(if matches!(e, store::NotAdded::Full) { ErrorCode::TooManyPhones } else { ErrorCode::Storage })
		}
	}
}

#[derive(Deserialize)]
struct RekeyRequest {
	#[serde(default)]
	device: String,
}

/// A key of its own for a phone that holds another's token, as one does after Import pairings:
/// a new entry named `device`, with the role and profile of the entry whose token asked and a new token, the
/// same answer as pairing's. That token stays good, so the phone it was copied from keeps working
/// until it is revoked on its own. Any role may ask, and none is raised by it; it counts as acting
/// (the rate and the activity log, both the family's), but needs no control. Only an entry paired
/// with a code may ask, and once (403 `rekey_refused`), and not past `limit::PHONES_MAX` phones
/// (403 `too_many_phones`).
async fn rekey(
	State(hub): State<Arc<Hub>>,
	Extension(who): Extension<Caller>,
	ConnectInfo(peer): ConnectInfo<SocketAddr>,
	Json(req): Json<RekeyRequest>,
) -> Response {
	let name = store::sanitize_name(&req.device);
	let ip = peer.ip().to_canonical();
	hub.act_as(&who, None, false, format!("phone added: {name}"), async {
		// clients.json's lock keeps it to one change at a time, pairing's and the commands' included.
		let (dir, from, device) = (hub.dir.clone(), who.id.clone(), name.clone());
		let added = tokio::task::spawn_blocking(move || store::add_client_from(&dir, &from, &device))
			.await
			.unwrap_or_else(|e| Err(store::NotAdded::Io(std::io::Error::other(e))));
		match added {
			Ok((client, token)) => {
				eprintln!(
					"pairing: added \"{}\" ({}) as {} from {ip}, asked with the key of \"{}\" ({}), which stays paired",
					client.name,
					client.id,
					client.role.as_str(),
					who.name,
					who.id
				);
				let profile = profile_of(&hub, client.profile.as_deref());
				let response = Json(paired_body(&token, &client.id, &hub.id, &hub.name, client.role, &profile)).into_response();
				Ok(Acted { response, note: Some(format!("id {}, {}", client.id, client.role.as_str())) })
			}
			Err(e) => {
				let code = match e {
					store::NotAdded::Full => ErrorCode::TooManyPhones,
					store::NotAdded::Rekeyed => ErrorCode::RekeyRefused,
					store::NotAdded::Gone => ErrorCode::Unauthorized,
					store::NotAdded::Io(_) => ErrorCode::Storage,
				};
				eprintln!("pairing: \"{}\" ({}) asked for a key of its own, not added: {e}", who.name, who.id);
				Err(code.into())
			}
		}
	})
	.await
}

/// How the certificate presented came to replace those before it; see `identity`. Public: it only
/// proves that one certificate follows another, which a phone checks for itself.
async fn rotations(State(hub): State<Arc<Hub>>) -> Response {
	let chain = hub.rotations.read().map(|r| r.clone()).unwrap_or_else(|p| p.into_inner().clone());
	json_response(chain.as_str().to_owned())
}


/// The widget types a client draws, from `widgets=value,gauge,…`.
pub fn draws_from(widgets: &str) -> BTreeSet<String> {
	widgets.split(',').map(str::trim).filter(|w| !w.is_empty()).map(str::to_string).collect()
}

#[derive(Deserialize, Default)]
struct UiQuery {
	widgets: Option<String>,
}

/// What the client shows: its profile, its tabs, the dashboards and the actions, as far as its role
/// goes, with the widget types it draws. Its streams describe with the same types from then on.
async fn ui_doc(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Extension(app): Extension<ClientApp>, Query(q): Query<UiQuery>) -> Response {
	let draws = q.widgets.as_deref().map(draws_from);
	hub.remember_draws(&who.id, &app.kind, draws.clone());
	let frame = hub.frame.borrow().clone();
	let ui = hub.ui(&hub.ui_key(&who, &app.kind, draws), &frame);
	json_response(ui.json())
}

/// The dashboard a client asked for in `ui`, or the first tab (the header, without tabs).
pub(crate) fn dashboard_in<'a>(ui: &'a Ui, asked: Option<&str>) -> Option<&'a ui::Dashboard> {
	match asked {
		Some(id) => ui.dashboard(id),
		None => ui.tabs.first().or(ui.header.as_ref()).and_then(|id| ui.dashboard(id)),
	}
}

/// What a dashboard on screen needs read: its own sources and the header's.
pub(crate) fn needs_of(ui: &Ui, dashboard: &ui::Dashboard) -> Sources {
	let header = ui.header.as_deref().and_then(|h| ui.dashboard(h)).map(ui::Dashboard::needs).unwrap_or_default();
	dashboard.needs().union(header)
}

/// A `data` line's object for `dashboard` of `ui`, as `who` sees the reading `frame`, with its
/// history when `history`.
pub(crate) fn data_of(hub: &Hub, ui: &Ui, dashboard: &ui::Dashboard, who: &Caller, frame: &Frame, sorts: &HashMap<String, String>, history: bool) -> ui::Obj {
	let header = ui.header.as_deref().and_then(|h| ui.dashboard(h));
	let history = history.then(|| {
		let ring = hub.history.lock().unwrap_or_else(|p| p.into_inner());
		let mut h = crate::history::json(dashboard, &ring, ui.units, frame.values.t);
		if let Some(header) = header.filter(|h| h.id != dashboard.id) {
			h.extend(crate::history::json(header, &ring, ui.units, frame.values.t));
		}
		h
	});
	let states = crate::states::States { view: crate::values::View::new(&frame.values, who.role, ui.units), control: hub.control_on(), sorts };
	crate::states::data(frame.values.t, dashboard, header, &states, history)
}

#[derive(Deserialize, Default)]
struct SnapshotQuery {
	dashboard: Option<String>,
	widgets: Option<String>,
	/// `0` leaves `history` out.
	history: Option<u8>,
}

/// One `data` object for a dashboard, with its history, once a reading has what it needs: a few
/// ticks at most.
async fn snapshot(
	State(hub): State<Arc<Hub>>,
	Extension(who): Extension<Caller>,
	Extension(app): Extension<ClientApp>,
	Query(q): Query<SnapshotQuery>,
) -> Response {
	let draws = match &q.widgets {
		Some(w) => Some(draws_from(w)),
		None => hub.draws_of(&who.id, &app.kind),
	};
	let key = hub.ui_key(&who, &app.kind, draws);
	let mut rx = hub.frame.clone();
	let mut frame = rx.borrow_and_update().clone();
	let ui = hub.ui(&key, &frame);
	let Some(need) = dashboard_in(&ui, q.dashboard.as_deref()).map(|d| needs_of(&ui, d)) else {
		return error(ErrorCode::NoSuchDashboard);
	};
	let _want = hub.want(need);
	let deadline = tokio::time::Instant::now() + hub.interval * (cadence::FIRST_FRAME_TICKS + 1);
	while !frame.read.covers(need) {
		match tokio::time::timeout_at(deadline, rx.changed()).await {
			Ok(Ok(())) => frame = rx.borrow_and_update().clone(),
			_ => break,
		}
	}
	let ui = hub.ui(&key, &frame);
	let Some(dashboard) = dashboard_in(&ui, q.dashboard.as_deref()) else {
		return error(ErrorCode::NoSuchDashboard);
	};
	let mut data = data_of(&hub, &ui, dashboard, &who, &frame, &HashMap::new(), q.history != Some(0));
	data.remove("type");
	json_response(serde_json::to_string(&data).unwrap_or_default())
}

pub(crate) fn json_response(body: String) -> Response {
	([(header::CONTENT_TYPE, "application/json")], body).into_response()
}

/// Keepalive on the listening socket, which Linux hands down to every connection it accepts:
/// a phone that dropped off the network without closing stops counting as watching within
/// about a minute, instead of the kernel's two hours or more.
pub fn keep_alive(listener: &std::net::TcpListener) -> std::io::Result<()> {
	use std::os::fd::AsRawFd;
	let fd = listener.as_raw_fd();
	let set = |level: libc::c_int, name: libc::c_int, value: libc::c_int| {
		// SAFETY: setsockopt(2) on a socket we own, reading one int from the stack.
		let rc = unsafe {
			libc::setsockopt(fd, level, name, (&value as *const libc::c_int).cast(), size_of::<libc::c_int>() as libc::socklen_t)
		};
		if rc == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
	};
	let secs = |d: Duration| d.as_secs() as libc::c_int;
	set(libc::SOL_SOCKET, libc::SO_KEEPALIVE, 1)?;
	set(libc::IPPROTO_TCP, libc::TCP_KEEPIDLE, secs(timeout::KEEPALIVE_IDLE))?;
	set(libc::IPPROTO_TCP, libc::TCP_KEEPINTVL, secs(timeout::KEEPALIVE_INTERVAL))?;
	set(libc::IPPROTO_TCP, libc::TCP_KEEPCNT, timeout::KEEPALIVE_PROBES as libc::c_int)?;
	set(libc::IPPROTO_TCP, libc::TCP_USER_TIMEOUT, timeout::UNACKED.as_millis() as libc::c_int)
}


#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn bearer_tokens() {
		assert_eq!(bearer("Bearer abc"), Some("abc"));
		assert_eq!(bearer("bearer abc "), Some("abc"));
		assert_eq!(bearer("BEARER  abc"), Some("abc"));
		assert_eq!(bearer("Basic abc"), None);
		assert_eq!(bearer("Bearer "), None);
		assert_eq!(bearer("Bearerabc"), None);
	}

	#[test]
	fn clients_say_what_they_are() {
		let app = |h: Option<&str>| ClientApp::parse(h);
		assert_eq!(app(Some("android/0.6.0")), ClientApp { kind: "android".into(), version: Some("0.6.0".into()) });
		assert_eq!(app(Some(" Web/0.8.0-rc.1 ")).label().as_deref(), Some("web/0.8.0-rc.1"));
		// Without it, or with what isn't one, an Android app of no known version.
		for h in [None, Some(""), Some("andr oid/1"), Some("\u{1b}[31m/1"), Some(&"x".repeat(40))] {
			assert_eq!(app(h), ClientApp { kind: "android".into(), version: None }, "{h:?}");
		}
		assert_eq!(app(Some("android/1.0 \u{7}")).version, None);
		assert_eq!(app(Some("android")).label(), None);
	}

	#[test]
	fn accepted_connections_inherit_keepalive() {
		let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
		keep_alive(&listener).unwrap();
		let _client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
		let (conn, _) = listener.accept().unwrap();
		let get = |level: libc::c_int, name: libc::c_int| {
			use std::os::fd::AsRawFd;
			let mut value: libc::c_int = 0;
			let mut len = size_of::<libc::c_int>() as libc::socklen_t;
			// SAFETY: getsockopt(2) into an int on the stack, with its size.
			let rc = unsafe { libc::getsockopt(conn.as_raw_fd(), level, name, (&mut value as *mut libc::c_int).cast(), &mut len) };
			assert_eq!(rc, 0);
			value
		};
		assert_eq!(get(libc::SOL_SOCKET, libc::SO_KEEPALIVE), 1);
		assert_eq!(get(libc::IPPROTO_TCP, libc::TCP_KEEPIDLE), 30);
		assert_eq!(get(libc::IPPROTO_TCP, libc::TCP_KEEPINTVL), 10);
		assert_eq!(get(libc::IPPROTO_TCP, libc::TCP_KEEPCNT), 3);
		assert_eq!(get(libc::IPPROTO_TCP, libc::TCP_USER_TIMEOUT), 60_000);
	}

	// ---- through the router as a phone meets it ----------------------------------------------

	use crate::store::Client;
	use axum::body::Body;
	use axum::http::{Method, StatusCode};
	use futures_util::StreamExt;
	use tower::ServiceExt;

	/// The phones every test hub has: one for each role, its token "t-<role>", and one paired
	/// before roles, whose entry in clients.json has none ("t-old").
	const PHONES: [&str; 5] = ["glance", "monitor", "operate", "admin", "old"];

	/// What the test hubs' settings add to the built-in ones: a link with a unit, an action with a
	/// parameter, a profile.
	const SETTINGS: &str = r#"
mdns = false
[links.web]
name = "Web"
url = "http://{host}:8080"
unit = "elamon-test-web.service"
[actions.echo]
name = "Echo"
command = ["echo", "{word}"]
params.word = { choices = ["hi", "there"], default = "hi" }
confirm = false
[profiles.owner]
name = "Owner"
theme = "dark"
dashboards = ["overview", "control"]
"#;

	fn scratch() -> PathBuf {
		let dir = std::env::temp_dir().join(format!("elamond-hub-{}", data_encoding::HEXLOWER.encode(&store::random_bytes::<6>())));
		std::fs::create_dir_all(&dir).unwrap();
		dir
	}

	fn client(name: &str, role: Role) -> Client {
		Client { id: format!("id-{name}"), name: name.into(), token_hash: store::sha256_hex(format!("t-{name}").as_bytes()), created: 1, role, ..Default::default() }
	}

	fn settings(control: bool) -> store::Config {
		let text = format!("control = {control}\n{SETTINGS}");
		crate::settings::read_sources(&[crate::settings::Source::new("test.toml", text)]).unwrap()
	}

	/// A reading of the fixtures for `config`, with everything read for it.
	fn frame(config: &store::Config, seq: u64) -> Frame {
		let values = crate::fixtures::values(config);
		let caps = Caps::all(&values.power_profiles);
		let all = Sources { processes: true, media: true, volume: true, gpus: true, displays: true, links: true };
		Frame { seq, shape: shape_of(&values, &caps), values: Arc::new(values), caps: Arc::new(caps), read: all, features: Arc::new(vec!["processes"]) }
	}

	/// A hub with its state in a directory of its own, the phones above paired, and a rate
	/// limit out of the way.
	fn hub(control: bool) -> (Arc<Hub>, watch::Sender<Arc<Frame>>) {
		let dir = scratch();
		let mut clients: Vec<serde_json::Value> =
			Role::ALL.iter().map(|r| serde_json::to_value(client(r.as_str(), *r)).unwrap()).collect();
		let mut old = serde_json::to_value(client("old", Role::Admin)).unwrap();
		old.as_object_mut().unwrap().remove("role");
		clients.push(old);
		std::fs::write(dir.join("clients.json"), serde_json::to_vec(&clients).unwrap()).unwrap();
		let config = settings(control);
		let (tx, rx) = watch::channel(Arc::new(frame(&config, 1)));
		let hub = Hub::new("id".into(), "test".into(), rx, Ring::new(10), Duration::from_millis(250), Arc::new(config), dir);
		*hub.rate.lock().unwrap() = RateLimiter::new(10_000, Duration::from_secs(1));
		(Arc::new(hub), tx)
	}

	fn request_as(method: &str, path: &str, token: &str, body: &str) -> Request {
		axum::http::Request::builder()
			.method(Method::from_bytes(method.as_bytes()).unwrap())
			.uri(path)
			.header(header::AUTHORIZATION, format!("Bearer {token}"))
			.header(header::CONTENT_TYPE, "application/json")
			.header(protocol::CLIENT_HEADER, "android/0.6.0")
			.extension(ConnectInfo(SocketAddr::from(([192, 0, 2, 1], 50000))))
			.body(Body::from(body.to_string()))
			.unwrap()
	}

	async fn call(hub: &Arc<Hub>, method: &str, path: &str, phone: &str, body: &str) -> (StatusCode, serde_json::Value) {
		call_as(hub, method, path, &format!("t-{phone}"), body).await
	}

	/// `call` with a token of its own rather than one of the test phones'.
	async fn call_as(hub: &Arc<Hub>, method: &str, path: &str, token: &str, body: &str) -> (StatusCode, serde_json::Value) {
		let resp = router(hub.clone()).oneshot(request_as(method, path, token, body)).await.unwrap();
		let status = resp.status();
		if path.starts_with("/api/v1/stream") && method == "GET" {
			return (status, serde_json::Value::Null);
		}
		let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
		(status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
	}

	/// Every route behind a token, the least role it needs and whether it needs control, with a
	/// body that reads but changes nothing on the computer: a parameter the action hasn't, a value
	/// out of its range, an unknown player, PID 0. Every route in `router` is here.
	const ROUTES: &[(&str, &str, &str, Option<Role>, bool)] = &[
		("GET", "/api/v1/ui", "", None, false),
		("GET", "/api/v1/ui?widgets=value,text", "", None, false),
		("GET", "/api/v1/snapshot", "", None, false),
		("GET", "/api/v1/stream", "", None, false),
		("POST", "/api/v1/stream/nothing", "{}", None, false),
		("GET", "/api/v1/media/art?player=nothing", "", Some(Role::Monitor), false),
		("POST", "/api/v1/actions/power.reboot", r#"{"params":{"nothing":1}}"#, Some(Role::Admin), true),
		("POST", "/api/v1/actions/power.poweroff", r#"{"params":{"nothing":1}}"#, Some(Role::Admin), true),
		("POST", "/api/v1/actions/process.signal", r#"{"params":{"pid":0,"signal":"term"}}"#, Some(Role::Admin), true),
		("POST", "/api/v1/actions/profile.set", r#"{"params":{"profile":"nothing"}}"#, Some(Role::Operate), true),
		("POST", "/api/v1/actions/unit.restart", r#"{"params":{"link":"nothing"}}"#, Some(Role::Operate), true),
		("POST", "/api/v1/actions/media.play_pause", r#"{"params":{"player":"nothing"}}"#, Some(Role::Operate), true),
		("POST", "/api/v1/actions/volume.set", r#"{"params":{"level":500}}"#, Some(Role::Operate), true),
		("POST", "/api/v1/actions/echo", r#"{"params":{"word":"nothing"}}"#, Some(Role::Operate), true),
		("POST", "/api/v1/actions/nothing", "", None, false),
		("POST", "/api/v1/rekey", r#"{"device":"Route phone"}"#, None, false),
	];

	fn role_of(phone: &str) -> Role {
		Role::parse(phone).unwrap_or(Role::Admin)
	}

	#[tokio::test]
	async fn every_route_for_every_role() {
		for control in [true, false] {
			let (hub, _tx) = hub(control);
			for phone in PHONES {
				for (method, path, body, need, needs_control) in ROUTES {
					let (status, v) = call(&hub, method, path, phone, body).await;
					let error = v.get("error").and_then(|e| e.as_str()).unwrap_or("");
					assert!(!matches!(status.as_u16(), 401 | 405 | 415 | 422 | 429) && error != "not_found", "{method} {path} as {phone}: {status} {v}");
					let got = match error {
						"not_allowed" | "control_disabled" => error,
						_ => "let through",
					};
					let expected = match need {
						Some(r) if role_of(phone) < *r => "not_allowed",
						Some(_) if *needs_control && !control => "control_disabled",
						_ => "let through",
					};
					assert_eq!(got, expected, "{method} {path} as {phone} with control {control}: {status} {v}");
					if got != "let through" {
						assert_eq!(status, StatusCode::FORBIDDEN);
					}
				}
			}
			// Every acting request is in the log, refused or not, newest first.
			let log = hub.activity.newest(usize::MAX);
			let acting = ROUTES.iter().filter(|(m, p, ..)| *m == "POST" && !p.starts_with("/api/v1/stream")).count();
			assert_eq!(log.len(), acting * PHONES.len());
			let reboot = |phone: &str| log.iter().find(|e| e.phone == phone && e.what == "power reboot").unwrap().clone();
			assert_eq!((reboot("glance").result.as_str(), reboot("glance").client_id.as_str()), ("not_allowed", "id-glance"));
			if control {
				assert_eq!((reboot("old").result.as_str(), reboot("old").message.as_deref()), ("bad_param", Some("nothing")));
				let echo = log.iter().find(|e| e.phone == "admin" && e.what.starts_with("action Echo")).unwrap();
				assert_eq!((echo.what.as_str(), echo.result.as_str()), ("action Echo nothing", "bad_param"));
				let signal = log.iter().find(|e| e.phone == "admin" && e.what.starts_with("signal")).unwrap();
				assert_eq!((signal.what.as_str(), signal.result.as_str()), ("signal term 0", "not_permitted"));
				let unknown = log.iter().find(|e| e.phone == "admin" && e.what == "action nothing").unwrap();
				assert_eq!(unknown.result, "no_such_action");
				let unit = log.iter().find(|e| e.phone == "admin" && e.what.starts_with("unit")).unwrap();
				assert_eq!(unit.what, "unit restart nothing");
			} else {
				assert_eq!(reboot("admin").result, "control_disabled");
			}
			assert!(log.windows(2).all(|w| w[0].t >= w[1].t));
			std::fs::remove_dir_all(&hub.dir).unwrap();
		}
	}

	#[tokio::test]
	async fn the_ui_follows_the_role_the_profile_and_what_the_client_draws() {
		let (hub, _tx) = hub(true);
		let (status, v) = call(&hub, "GET", "/api/v1/ui", "admin", "").await;
		assert_eq!(status, StatusCode::OK);
		assert_eq!((v["role"].as_str(), v["profile"]["id"].as_str(), v["control"]["enabled"].as_bool()), (Some("admin"), Some("default"), Some(true)));
		assert_eq!(v["service"]["interval_ms"], 250);
		assert!(v["version"].as_str().is_some_and(|s| s.len() == 16));
		assert!(v["actions"].as_array().unwrap().iter().any(|a| a["id"] == "echo"));
		// The same, asked again with nothing changed.
		assert_eq!(call(&hub, "GET", "/api/v1/ui", "admin", "").await.1["version"], v["version"]);
		// A glance phone sees no processes, no actions.
		let (_, g) = call(&hub, "GET", "/api/v1/ui", "glance", "").await;
		assert_eq!(g["role"], "glance");
		assert!(g["actions"].as_array().unwrap().is_empty() && !g["tabs"].as_array().unwrap().iter().any(|t| t == "processes"), "{g}");
		assert_ne!(g["version"], v["version"]);
		// Its profile in clients.json; one the settings don't have is the default.
		let set_profile = |p: Option<&str>| {
			// Locked, as the commands change it: the service notes which app each phone is meanwhile.
			let _lock = store::lock_clients(&hub.dir).unwrap();
			let mut list = store::load_clients_in(&hub.dir).unwrap();
			list.iter_mut().find(|c| c.name == "admin").unwrap().profile = p.map(str::to_string);
			store::save_clients_in(&hub.dir, &list).unwrap();
		};
		set_profile(Some("owner"));
		let (_, o) = call(&hub, "GET", "/api/v1/ui", "admin", "").await;
		assert_eq!((o["profile"]["id"].as_str(), o["profile"]["theme"].as_str()), (Some("owner"), Some("dark")));
		assert_eq!(o["tabs"], serde_json::json!(["overview", "control"]));
		set_profile(Some("gone"));
		assert_eq!(call(&hub, "GET", "/api/v1/ui", "admin", "").await.1["profile"]["id"], "default");
		// A client that draws few types gets what it draws.
		let (_, few) = call(&hub, "GET", "/api/v1/ui?widgets=value,text", "admin", "").await;
		let types: BTreeSet<String> = few["dashboards"]
			.as_array()
			.unwrap()
			.iter()
			.flat_map(|d| d["widgets"].as_array().unwrap().iter().map(|w| w["type"].as_str().unwrap().to_string()))
			.collect();
		assert!(types.iter().all(|t| t == "value" || t == "text"), "{types:?}");
		// It said which app it is: clients.json keeps it, written once.
		let app = async || store::load_clients_in(&hub.dir).unwrap().into_iter().find(|c| c.name == "admin").unwrap().app;
		for _ in 0..100 {
			if app().await.is_some() {
				break;
			}
			tokio::time::sleep(Duration::from_millis(20)).await;
		}
		assert_eq!(app().await.as_deref(), Some("android/0.6.0"));
		let stamp = store::clients_stamp_in(&hub.dir);
		call(&hub, "GET", "/api/v1/ui", "admin", "").await;
		tokio::time::sleep(Duration::from_millis(100)).await;
		assert_eq!(store::clients_stamp_in(&hub.dir), stamp);
		std::fs::remove_dir_all(&hub.dir).unwrap();
	}

	#[tokio::test]
	async fn control_off_disables_the_actions_and_says_why() {
		let (hub, _tx) = hub(true);
		let ui = async || call(&hub, "GET", "/api/v1/ui", "admin", "").await.1;
		assert_eq!(ui().await["control"], serde_json::json!({ "enabled": true, "off_reason": null }));
		let reboot = async || call(&hub, "POST", "/api/v1/actions/power.reboot", "admin", r#"{"params":{"nothing":1}}"#).await;
		assert_eq!(reboot().await.0, StatusCode::BAD_REQUEST);
		// Held off: still on in the settings, off for everyone.
		store::set_control_off(&hub.dir, true).unwrap();
		let (status, v) = reboot().await;
		assert_eq!((status, v["error"].as_str()), (StatusCode::FORBIDDEN, Some("control_disabled")));
		assert_eq!(ui().await["control"]["off_reason"], "held_off");
		store::set_control_off(&hub.dir, false).unwrap();
		assert_eq!(reboot().await.0, StatusCode::BAD_REQUEST);
		// Settings that stop parsing hold it off until they parse again; those read before stay.
		hub.settings_failed("elamond.d/half.toml: expected `=`".into());
		assert_eq!(ui().await["control"], serde_json::json!({ "enabled": false, "off_reason": "settings_unreadable" }));
		assert_eq!(reboot().await.1["error"], "control_disabled");
		assert!(hub.config().control);
		hub.set_config(settings(true));
		assert_eq!(ui().await["control"]["enabled"], true);
		hub.set_config(settings(false));
		assert_eq!(ui().await["control"]["off_reason"], "settings");
		std::fs::remove_dir_all(&hub.dir).unwrap();
	}

	#[tokio::test]
	async fn actions_run_with_their_parameters_checked() {
		let (hub, _tx) = hub(true);
		let (status, v) = call(&hub, "POST", "/api/v1/actions/echo", "operate", r#"{"params":{"word":"there"}}"#).await;
		assert_eq!(status, StatusCode::OK, "{v}");
		assert_eq!(v, serde_json::json!({ "ok": true, "exit_code": 0, "timed_out": false, "output": "there\n" }));
		// Its default, with no body at all.
		assert_eq!(call(&hub, "POST", "/api/v1/actions/echo", "operate", "").await.1["output"], "hi\n");
		// A value it doesn't take, a parameter it hasn't: 400 naming it.
		for (body, name) in [(r#"{"params":{"word":"rm -rf"}}"#, "word"), (r#"{"params":{"word":"hi","then":"x"}}"#, "then"), (r#"{"params":{"word":1}}"#, "word")] {
			let (status, v) = call(&hub, "POST", "/api/v1/actions/echo", "operate", body).await;
			assert_eq!((status, v["error"].as_str(), v["message"].as_str()), (StatusCode::BAD_REQUEST, Some("bad_param"), Some(name)), "{body}");
		}
		// What isn't JSON, or a body over 4 KiB, never reaches the action.
		assert_eq!(call(&hub, "POST", "/api/v1/actions/echo", "operate", "{").await.0, StatusCode::BAD_REQUEST);
		let big = format!(r#"{{"params":{{"word":"{}"}}}}"#, "x".repeat(limit::BODY_MAX));
		assert_eq!(call(&hub, "POST", "/api/v1/actions/echo", "operate", &big).await.0, StatusCode::PAYLOAD_TOO_LARGE);
		// A role that may see it but not run it, and one that doesn't run actions at all.
		assert_eq!(call(&hub, "POST", "/api/v1/actions/echo", "monitor", "").await.1["error"], "not_allowed");
		let log = hub.activity.newest(usize::MAX);
		let ok: Vec<(&str, Option<&str>)> = log.iter().filter(|e| e.result == "ok").map(|e| (e.what.as_str(), e.message.as_deref())).collect();
		assert_eq!(ok, [("action Echo hi", Some("exit 0")), ("action Echo there", Some("exit 0"))]);

		// A signal with a start time reaches only the process that started then.
		let mut child = std::process::Command::new("sleep").arg("30").spawn().unwrap();
		let pid = child.id();
		let start = process_start_of(pid, hub.boot_time);
		let body = |start: u64| format!(r#"{{"params":{{"pid":{pid},"signal":"kill","start":{start}}}}}"#);
		let (status, v) = call(&hub, "POST", "/api/v1/actions/process.signal", "admin", &body(start + 1)).await;
		assert_eq!((status, v["error"].as_str()), (StatusCode::CONFLICT, Some("process_changed")));
		assert!(child.try_wait().unwrap().is_none());
		let (status, v) = call(&hub, "POST", "/api/v1/actions/process.signal", "admin", &body(start)).await;
		assert_eq!((status, v), (StatusCode::OK, serde_json::json!({ "ok": true })));
		child.wait().unwrap();
		let e = &hub.activity.newest(1)[0];
		assert_eq!(e.what, format!("signal kill {pid} sleep"));
		// No signal for an operator.
		assert_eq!(call(&hub, "POST", "/api/v1/actions/process.signal", "operate", &body(start)).await.1["error"], "not_allowed");
		std::fs::remove_dir_all(&hub.dir).unwrap();
	}

	/// When a process started, as the process list counts it.
	fn process_start_of(pid: u32, boot: u64) -> u64 {
		let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
		let ticks: u64 = stat.rsplit_once(')').unwrap().1.split_whitespace().nth(19).unwrap().parse().unwrap();
		// SAFETY: sysconf(3) only reads a value.
		let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as u64;
		ticks / hz + boot
	}

	#[tokio::test]
	async fn a_phone_that_acts_too_often_is_slowed_down() {
		let (hub, _tx) = hub(true);
		*hub.rate.lock().unwrap() = RateLimiter::default();
		let reboot = r#"{"params":{"nothing":1}}"#;
		let mut statuses = Vec::new();
		for _ in 0..limit::ACTS_BURST + 3 {
			statuses.push(call(&hub, "POST", "/api/v1/actions/power.reboot", "admin", reboot).await.0);
		}
		assert!(statuses[..limit::ACTS_BURST as usize].iter().all(|s| *s == StatusCode::BAD_REQUEST), "{statuses:?}");
		assert!(statuses[limit::ACTS_BURST as usize..].iter().all(|s| *s == StatusCode::TOO_MANY_REQUESTS), "{statuses:?}");
		// Another phone isn't held up, and reading isn't counted.
		assert_eq!(call(&hub, "POST", "/api/v1/actions/power.reboot", "old", reboot).await.0, StatusCode::BAD_REQUEST);
		assert_eq!(call(&hub, "GET", "/api/v1/ui", "admin", "").await.0, StatusCode::OK);
		// Only the first refusal of the run is logged.
		let log = hub.activity.newest(usize::MAX);
		assert_eq!(log.iter().filter(|e| e.result == "too_many_requests").count(), 1);
		std::fs::remove_dir_all(&hub.dir).unwrap();
	}

	#[tokio::test]
	async fn a_phone_gets_a_key_of_its_own_beside_the_one_it_came_with() {
		// Control off: a key of its own needs none, whatever the role.
		let (hub, _tx) = hub(false);
		let mut list = store::load_clients_in(&hub.dir).unwrap();
		list.iter_mut().find(|c| c.name == "operate").unwrap().profile = Some("owner".into());
		store::save_clients_in(&hub.dir, &list).unwrap();
		for phone in PHONES {
			let role = role_of(phone);
			let profile = if phone == "operate" { "owner" } else { "default" };
			let before = store::load_clients_in(&hub.dir).unwrap();
			// A role asked for in the body is no more than a word in it.
			let (status, v) = call(&hub, "POST", "/api/v1/rekey", phone, r#"{"device":" New\u0007 phone ","role":"admin"}"#).await;
			assert_eq!(status, StatusCode::OK, "{phone}: {v}");
			assert_eq!((v["role"].as_str(), v["id"].as_str(), v["name"].as_str()), (Some(role.as_str()), Some("id"), Some("test")), "{phone}");
			assert_eq!(v["profile"].as_str(), Some(profile), "{phone}");
			let (token, new_id) = (v["token"].as_str().unwrap().to_string(), v["client_id"].as_str().unwrap().to_string());
			assert_ne!(token, format!("t-{phone}"));

			// A new entry beside the one that asked, which stays as it was.
			let list = store::load_clients_in(&hub.dir).unwrap();
			assert_eq!(list.len(), before.len() + 1, "{phone}");
			let added = list.iter().find(|c| c.id == new_id).unwrap();
			let from = format!("id-{phone}");
			assert_eq!((added.name.as_str(), added.role, added.from.as_deref()), ("New phone", role, Some(from.as_str())), "{phone}");
			assert_eq!(added.token_hash, store::sha256_hex(token.as_bytes()));
			assert!(list.iter().any(|c| c.id == from && c.token_hash == store::sha256_hex(format!("t-{phone}").as_bytes())));

			// Both tokens work, each as its own phone with the same role and profile.
			for t in [format!("t-{phone}"), token.clone()] {
				let (status, v) = call_as(&hub, "GET", "/api/v1/ui", &t, "").await;
				assert_eq!((status, v["role"].as_str(), v["profile"]["id"].as_str()), (StatusCode::OK, Some(role.as_str()), Some(profile)), "{phone}");
			}

			// The old phone revoked, the new one carries on. Locked, as the commands change it: the
			// service notes which app each phone is meanwhile.
			{
				let _lock = store::lock_clients(&hub.dir).unwrap();
				let mut list = store::load_clients_in(&hub.dir).unwrap();
				list.retain(|c| c.id != from);
				store::save_clients_in(&hub.dir, &list).unwrap();
			}
			assert_eq!(call(&hub, "GET", "/api/v1/ui", phone, "").await.0, StatusCode::UNAUTHORIZED);
			let (status, v) = call_as(&hub, "GET", "/api/v1/ui", &token, "").await;
			assert_eq!((status, v["role"].as_str()), (StatusCode::OK, Some(role.as_str())), "{phone}");
		}

		// In the activity log under the phone that asked, with the new entry's id.
		let log = hub.activity.newest(usize::MAX);
		assert_eq!(log.len(), PHONES.len());
		let e = log.iter().find(|e| e.client_id == "id-glance").unwrap();
		assert_eq!((e.phone.as_str(), e.what.as_str(), e.result.as_str()), ("glance", "phone added: New phone", "ok"));
		assert!(e.message.as_deref().is_some_and(|m| m.starts_with("id ") && m.ends_with(", glance")), "{e:?}");
		std::fs::remove_dir_all(&hub.dir).unwrap();
	}

	#[tokio::test]
	async fn a_token_gives_one_key_of_its_own_and_its_family_one_rate() {
		let (hub, _tx) = hub(true);
		*hub.rate.lock().unwrap() = RateLimiter::default();
		let (status, v) = call(&hub, "POST", "/api/v1/rekey", "monitor", r#"{"device":"New"}"#).await;
		assert_eq!(status, StatusCode::OK, "{v}");
		let new = v["token"].as_str().unwrap().to_string();
		// Once: not again from the old entry, and never from the new one.
		let refused = (StatusCode::FORBIDDEN, Some("rekey_refused"));
		let (status, v) = call(&hub, "POST", "/api/v1/rekey", "monitor", r#"{"device":"Again"}"#).await;
		assert_eq!((status, v["error"].as_str()), refused);
		let (status, v) = call_as(&hub, "POST", "/api/v1/rekey", &new, r#"{"device":"Again"}"#).await;
		assert_eq!((status, v["error"].as_str()), refused);
		// The two share one rate: what is left of the burst, then 429 for either.
		let mut statuses = Vec::new();
		for i in 0..limit::ACTS_BURST - 3 + 2 {
			let token = if i % 2 == 0 { "t-monitor".to_string() } else { new.clone() };
			statuses.push(call_as(&hub, "POST", "/api/v1/actions/profile.set", &token, r#"{"params":{"profile":"nothing"}}"#).await.0);
		}
		let used = (limit::ACTS_BURST - 3) as usize;
		assert!(statuses[..used].iter().all(|s| *s == StatusCode::FORBIDDEN), "{statuses:?}");
		assert!(statuses[used..].iter().all(|s| *s == StatusCode::TOO_MANY_REQUESTS), "{statuses:?}");
		// Another phone isn't held up.
		assert_eq!(call(&hub, "POST", "/api/v1/rekey", "operate", r#"{"device":"Mine"}"#).await.0, StatusCode::OK);
		let log = hub.activity.newest(usize::MAX);
		assert_eq!(log.iter().filter(|e| e.result == "too_many_requests").count(), 1);
		assert_eq!(log.iter().filter(|e| e.result == "rekey_refused").count(), 2);
		// The new phone's entries carry the family they share.
		assert!(log.iter().filter(|e| e.client_id != "id-monitor" && e.phone == "New").all(|e| e.family.as_deref() == Some("id-monitor")));
		assert_eq!(store::load_clients_in(&hub.dir).unwrap().len(), PHONES.len() + 2);

		// A file of phones that no longer parses is left alone: the phones read before keep working,
		// and no new one is added over them.
		// Under the file's lock, as anything that writes it: the app each phone is, noted in the
		// background after its requests, can't write the list it read before over the broken file.
		{
			let _lock = store::lock_clients(&hub.dir).unwrap();
			std::fs::write(hub.dir.join(store::CLIENTS_FILE), "[{broken").unwrap();
		}
		let (status, v) = call(&hub, "POST", "/api/v1/rekey", "admin", r#"{"device":"Over"}"#).await;
		assert_eq!((status, v["error"].as_str()), (StatusCode::INTERNAL_SERVER_ERROR, Some("storage")));
		assert_eq!(std::fs::read_to_string(hub.dir.join(store::CLIENTS_FILE)).unwrap(), "[{broken");
		assert_eq!(call(&hub, "GET", "/api/v1/ui", "glance", "").await.0, StatusCode::OK);
		std::fs::remove_dir_all(&hub.dir).unwrap();
	}

	#[tokio::test]
	async fn only_a_link_the_settings_give_has_a_unit_to_use() {
		let (hub, _tx) = hub(true);
		// The link the settings give, and no other: the parameter's choices.
		let (_, ui) = call(&hub, "GET", "/api/v1/ui", "admin", "").await;
		let restart = ui["actions"].as_array().unwrap().iter().find(|a| a["id"] == "unit.restart").unwrap().clone();
		assert_eq!(restart["params"][0]["choices"], serde_json::json!([{ "value": "web", "label": "Web" }]));
		let (status, v) = call(&hub, "POST", "/api/v1/actions/unit.restart", "admin", r#"{"params":{"link":"gone"}}"#).await;
		assert_eq!((status, v["error"].as_str(), v["message"].as_str()), (StatusCode::BAD_REQUEST, Some("bad_param"), Some("link")));
		// Without a link with a unit, no unit actions at all.
		let config = crate::settings::read_sources(&[crate::settings::Source::new(
			"a.toml",
			"control = true\nmdns = false\n[links.plain]\nname = \"Plain\"\nurl = \"http://{host}:81\"\n",
		)])
		.unwrap();
		hub.set_config(config);
		let (status, v) = call(&hub, "POST", "/api/v1/actions/unit.restart", "admin", r#"{"params":{"link":"plain"}}"#).await;
		assert_eq!((status, v["error"].as_str()), (StatusCode::NOT_FOUND, Some("no_such_action")));
		let statuses = hub.link_statuses().await;
		assert_eq!(statuses.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["plain"]);
		std::fs::remove_dir_all(&hub.dir).unwrap();
	}

	#[tokio::test]
	async fn snapshots_carry_a_dashboard_as_the_role_sees_it() {
		let (hub, _tx) = hub(true);
		for phone in PHONES {
			let (status, v) = call(&hub, "GET", "/api/v1/snapshot", phone, "").await;
			assert_eq!(status, StatusCode::OK, "{phone}");
			assert_eq!(v["dashboard"], "overview", "{phone}");
			assert!(v.get("type").is_none() && v["history"].is_object() && v["t"].is_u64(), "{phone}: {v}");
			let (_, bare) = call(&hub, "GET", "/api/v1/snapshot?history=0", phone, "").await;
			assert!(bare.get("history").is_none() && bare["widgets"].is_object(), "{phone}: {bare}");
			let (status, v) = call(&hub, "GET", "/api/v1/snapshot?dashboard=processes", phone, "").await;
			if role_of(phone) >= Role::Monitor {
				let rows = &v["widgets"]["processes/table"]["rows"];
				assert!(rows.as_array().is_some_and(|r| !r.is_empty()), "{phone}: {v}");
			} else {
				assert_eq!((status, v["error"].as_str()), (StatusCode::NOT_FOUND, Some("no_such_dashboard")), "{phone}");
			}
		}
		std::fs::remove_dir_all(&hub.dir).unwrap();
	}

	/// The lines of a stream as they come, each in time.
	struct Lines(axum::body::BodyDataStream);

	impl Lines {
		async fn open(hub: &Arc<Hub>, path: &str, token: &str) -> Lines {
			let resp = router(hub.clone()).oneshot(request_as("GET", path, token, "")).await.unwrap();
			assert_eq!(resp.status(), StatusCode::OK);
			assert_eq!(resp.headers()[header::CONTENT_TYPE], "application/x-ndjson");
			Lines(resp.into_body().into_data_stream())
		}

		async fn next(&mut self) -> Option<serde_json::Value> {
			let chunk = tokio::time::timeout(Duration::from_secs(5), self.0.next()).await.expect("a line in time")?.unwrap();
			assert!(chunk.ends_with(b"\n"));
			Some(serde_json::from_slice(&chunk).unwrap())
		}

		/// The next line of `kind`, after others.
		async fn next_of(&mut self, kind: &str) -> Option<serde_json::Value> {
			loop {
				let l = self.next().await?;
				if l["type"] == kind {
					return Some(l);
				}
			}
		}
	}

	/// A new reading, as the collector sends every tick.
	fn tick(hub: &Hub, tx: &watch::Sender<Arc<Frame>>) {
		let seq = tx.borrow().seq + 1;
		tx.send_replace(Arc::new(frame(&hub.config(), seq)));
	}

	/// Readings every 100 ms while held, as the collector sends them.
	fn ticking(hub: &Arc<Hub>, tx: watch::Sender<Arc<Frame>>) -> tokio::task::JoinHandle<()> {
		let hub = hub.clone();
		tokio::spawn(async move {
			loop {
				tokio::time::sleep(Duration::from_millis(100)).await;
				tick(&hub, &tx);
			}
		})
	}

	#[tokio::test]
	async fn a_stream_opens_sends_its_dashboard_and_changes_on_request() {
		let (hub, tx) = hub(true);
		let ui = call(&hub, "GET", "/api/v1/ui", "monitor", "").await.1;
		let mut lines = Lines::open(&hub, "/api/v1/stream", "t-monitor").await;
		let open = lines.next().await.unwrap();
		assert_eq!((open["type"].as_str(), open["ui"].as_str()), (Some("open"), ui["version"].as_str()));
		let id = open["stream"].as_str().unwrap().to_string();
		let _ticks = ticking(&hub, tx);
		// The first tab, with its history first.
		let first = lines.next_of("data").await.unwrap();
		assert_eq!(first["dashboard"], "overview");
		assert!(first["history"].is_object() && first["widgets"]["overview/cpu"].is_object() && first["widgets"]["header/os"].is_object(), "{first}");
		let next = lines.next_of("data").await.unwrap();
		assert!(next.get("history").is_none());
		assert!(hub.wanted().is_some());

		// Another dashboard, its table in another order, slower.
		let change = |body: &str| {
			let (hub, path, body) = (hub.clone(), format!("/api/v1/stream/{id}"), body.to_string());
			async move { call(&hub, "POST", &path, "monitor", &body).await }
		};
		assert_eq!(change(r#"{"dashboard":"nothing"}"#).await.1["error"], "no_such_dashboard");
		assert_eq!(change(r#"{"sort":{"processes/table":"nothing"}}"#).await.1["error"], "bad_sort");
		assert_eq!(change(r#"{"sort":{"overview/cpu":"cpu"}}"#).await.1["error"], "bad_sort");
		assert_eq!(change(r#"{"sort":{"overview/cpu":null}}"#).await.1["error"], "bad_sort");
		// A change refused in part is refused whole: the dashboard stays.
		let (status, v) = change(r#"{"dashboard":"processes","sort":{"processes/table":"nothing"}}"#).await;
		assert_eq!((status, v["message"].as_str()), (StatusCode::BAD_REQUEST, Some("processes/table")));
		assert_eq!(lines.next_of("data").await.unwrap()["dashboard"], "overview");
		let (status, v) = change(r#"{"dashboard":"processes","interval":300,"sort":{"processes/table":"mem"}}"#).await;
		assert_eq!((status, v), (StatusCode::OK, serde_json::json!({ "ok": true })));
		let procs = loop {
			let l = lines.next_of("data").await.unwrap();
			if l["dashboard"] == "processes" {
				break l;
			}
		};
		assert!(procs["history"].is_object() && procs["widgets"]["processes/table"]["rows"].is_array(), "{procs}");
		assert!(hub.wanted().is_some_and(|w| w.processes));
		// Back to the table's own order.
		assert_eq!(change(r#"{"sort":{"processes/table":null}}"#).await.0, StatusCode::OK);
		assert_eq!(lines.next_of("data").await.unwrap()["dashboard"], "processes");
		// Another phone's stream, or none, isn't there for this one.
		let (status, v) = call(&hub, "POST", &format!("/api/v1/stream/{id}"), "admin", "{}").await;
		assert_eq!((status, v["error"].as_str()), (StatusCode::NOT_FOUND, Some("no_such_stream")));

		// A new role: a `ui` line at once, and the stream goes on with what it sees.
		let set_role = |role: Option<Role>| {
			// Locked, as the commands change it: the service notes which app each phone is meanwhile.
			let _lock = store::lock_clients(&hub.dir).unwrap();
			let mut list = store::load_clients_in(&hub.dir).unwrap();
			match role {
				Some(r) => list.iter_mut().find(|c| c.name == "monitor").unwrap().role = r,
				None => list.retain(|c| c.name != "monitor"),
			}
			store::save_clients_in(&hub.dir, &list).unwrap();
		};
		set_role(Some(Role::Glance));
		let changed = lines.next_of("ui").await.unwrap();
		let glance = call(&hub, "GET", "/api/v1/ui", "monitor", "").await.1;
		assert_eq!(changed["ui"], glance["version"]);
		// Processes are gone for it: the first tab, from scratch.
		let after = lines.next_of("data").await.unwrap();
		assert_eq!(after["dashboard"], "overview");
		assert!(after["history"].is_object());
		assert!(hub.wanted().is_some_and(|w| !w.processes));

		// Unpaired: the stream ends, and stops asking for readings.
		set_role(None);
		while lines.next().await.is_some() {}
		tokio::time::sleep(Duration::from_millis(50)).await;
		assert_eq!(hub.watchers(), 0);
		assert!(hub.streams.lock().unwrap().is_empty());
		std::fs::remove_dir_all(&hub.dir).unwrap();
	}

	#[tokio::test]
	async fn a_client_holds_four_streams_and_a_fifth_ends_the_oldest() {
		let (hub, tx) = hub(true);
		let _ticks = ticking(&hub, tx);
		let mut streams = Vec::new();
		for _ in 0..limit::STREAMS_PER_CLIENT {
			let mut s = Lines::open(&hub, "/api/v1/stream?dashboard=control&interval=250", "t-admin").await;
			s.next().await.unwrap();
			streams.push(s);
		}
		// Another client's don't count.
		let mut other = Lines::open(&hub, "/api/v1/stream", "t-operate").await;
		other.next().await.unwrap();
		let mut fifth = Lines::open(&hub, "/api/v1/stream", "t-admin").await;
		fifth.next().await.unwrap();
		while streams[0].next().await.is_some() {}
		assert_eq!(streams[1].next_of("data").await.unwrap()["dashboard"], "control");
		assert!(other.next_of("data").await.is_some() && fifth.next_of("data").await.is_some());
		assert_eq!(hub.streams.lock().unwrap().len(), limit::STREAMS_PER_CLIENT + 1);
		std::fs::remove_dir_all(&hub.dir).unwrap();
	}

	#[tokio::test]
	async fn a_dashboard_gone_from_the_settings_gives_way_to_the_first_tab() {
		let (hub, tx) = hub(true);
		let config = |extra: &str| {
			let text = format!("control = true\n{SETTINGS}\n{extra}");
			crate::settings::read_sources(&[crate::settings::Source::new("test.toml", text)]).unwrap()
		};
		let mine = "[dashboards.mine]\ntitle = \"Mine\"\n[[dashboards.mine.widgets]]\ntype = \"value\"\nvalue = \"cpu.usage\"\n";
		hub.set_config(config(&format!("{mine}[profiles.default]\ndashboards = [\"mine\", \"overview\"]\n")));
		// One it doesn't get is the first tab.
		let mut lines = Lines::open(&hub, "/api/v1/stream?dashboard=nothing", "t-admin").await;
		lines.next().await.unwrap();
		let _ticks = ticking(&hub, tx);
		assert_eq!(lines.next_of("data").await.unwrap()["dashboard"], "mine");
		// New settings without the dashboard on screen: `ui`, then the first tab, from scratch.
		hub.set_config(config(""));
		lines.next_of("ui").await.unwrap();
		let after = lines.next_of("data").await.unwrap();
		assert_eq!(after["dashboard"], "overview");
		assert!(after["history"].is_object());
		std::fs::remove_dir_all(&hub.dir).unwrap();
	}

	/// A phone's attempt at pairing from 192.0.2.7, answered on a task of its own: it may wait.
	fn pair_from(hub: &Arc<Hub>, code: &str, device: &str) -> tokio::task::JoinHandle<(StatusCode, serde_json::Value)> {
		let (hub, body) = (hub.clone(), serde_json::json!({ "code": code, "device": device }).to_string());
		tokio::spawn(async move {
			let req = axum::http::Request::builder()
				.method(Method::POST)
				.uri("/api/v1/pair")
				.header(header::CONTENT_TYPE, "application/json")
				.extension(ConnectInfo(SocketAddr::from(([192, 0, 2, 7], 50000))))
				.body(Body::from(body))
				.unwrap();
			let resp = router(hub).oneshot(req).await.unwrap();
			let status = resp.status();
			let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
			(status, serde_json::from_slice(&bytes).unwrap_or_default())
		})
	}

	/// The phone waiting with the pending code, once there is one.
	async fn asked(dir: &std::path::Path) -> store::Ask {
		let wait = async {
			loop {
				if let Some(a) = store::read_pairing(dir).and_then(|p| p.ask) {
					return a;
				}
				tokio::time::sleep(Duration::from_millis(50)).await;
			}
		};
		tokio::time::timeout(Duration::from_secs(5), wait).await.expect("the phone's request in time")
	}

	#[tokio::test]
	async fn a_phone_with_the_right_code_waits_to_be_let_in() {
		let (hub, _tx) = hub(false);
		let declined = (StatusCode::FORBIDDEN, Some("pair_declined"));
		let paired = |hub: &Arc<Hub>, name: &str| store::load_clients_in(&hub.dir).unwrap().into_iter().find(|c| c.name == name);
		// `elamond pair` is there to answer while this is held.
		let held = store::hold_pairing(&hub.dir, Duration::ZERO).unwrap().unwrap();

		// Turned away: nothing added, the code gone.
		let p = store::new_pairing(&hub.dir, Role::Monitor, None, true).unwrap();
		let waiting = pair_from(&hub, &p.code, "Stranger\u{202e}");
		let ask = asked(&hub.dir).await;
		assert_eq!((ask.name.as_str(), ask.address.as_str(), ask.allow), ("Stranger", "192.0.2.7", None));
		// Meanwhile the code is spoken for, even the right one.
		let (status, v) = pair_from(&hub, &p.code, "Another").await.unwrap();
		assert_eq!((status, v["error"].as_str()), (StatusCode::FORBIDDEN, Some("no_pairing")));
		store::answer_pairing(&hub.dir, &p.code, &ask.id, false).unwrap();
		let (status, v) = waiting.await.unwrap();
		assert_eq!((status, v["error"].as_str()), declined);
		assert!(store::read_pairing(&hub.dir).is_none() && paired(&hub, "Stranger").is_none());

		// Let in, with the code's role.
		let p = store::new_pairing(&hub.dir, Role::Monitor, None, true).unwrap();
		let waiting = pair_from(&hub, &p.code, "Mine");
		let ask = asked(&hub.dir).await;
		store::answer_pairing(&hub.dir, &p.code, &ask.id, true).unwrap();
		let (status, v) = waiting.await.unwrap();
		assert_eq!((status, v["role"].as_str(), v["profile"].as_str()), (StatusCode::OK, Some("monitor"), Some("default")), "{v}");
		assert_eq!(paired(&hub, "Mine").unwrap().role, Role::Monitor);
		assert!(store::read_pairing(&hub.dir).is_none());

		// With no one at `elamond pair` to answer, turned away.
		drop(held);
		let p = store::new_pairing(&hub.dir, Role::Admin, None, true).unwrap();
		let (status, v) = pair_from(&hub, &p.code, "Nobody asked").await.unwrap();
		assert_eq!((status, v["error"].as_str()), declined);
		assert!(paired(&hub, "Nobody asked").is_none() && store::read_pairing(&hub.dir).is_none());

		// `elamond pair --yes`: at once.
		let p = store::new_pairing(&hub.dir, Role::Glance, Some("owner"), false).unwrap();
		let (status, v) = pair_from(&hub, &p.code, "Scripted").await.unwrap();
		assert_eq!((status, v["role"].as_str(), v["profile"].as_str()), (StatusCode::OK, Some("glance"), Some("owner")), "{v}");
		assert_eq!(paired(&hub, "Scripted").unwrap().profile.as_deref(), Some("owner"));

		// No code, or a body far bigger than a code and a name.
		let (status, v) = pair_from(&hub, "123456", "Late").await.unwrap();
		assert_eq!((status, v["error"].as_str()), (StatusCode::FORBIDDEN, Some("no_pairing")));
		store::new_pairing(&hub.dir, Role::Glance, Some("owner"), false).unwrap();
		let (status, _) = pair_from(&hub, "123456", &"x".repeat(limit::BODY_MAX)).await.unwrap();
		assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
		std::fs::remove_dir_all(&hub.dir).unwrap();
	}

	#[tokio::test]
	async fn rotations_are_served_without_a_token() {
		let (hub, _tx) = hub(true);
		let get = async || {
			let req = axum::http::Request::builder().uri("/api/v1/identity/rotations").body(Body::empty()).unwrap();
			let resp = router(hub.clone()).oneshot(req).await.unwrap();
			assert_eq!(resp.status(), StatusCode::OK);
			let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
			serde_json::from_slice::<Vec<crate::identity::Rotation>>(&bytes).unwrap()
		};
		assert!(get().await.is_empty());
		let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/rotations.json")).unwrap();
		let chain: Vec<crate::identity::Rotation> = serde_json::from_str(&text).unwrap();
		hub.set_rotations(&chain);
		assert_eq!(get().await, chain);
		std::fs::remove_dir_all(&hub.dir).unwrap();
	}

	#[test]
	fn streams_are_logged_a_few_at_a_time() {
		let mut n = StreamNotes::default();
		let t0 = Instant::now();
		assert_eq!(n.open(t0).as_deref(), Some(""));
		// A reconnect loop: the first stream's closing is logged, the next ones only counted.
		assert_eq!(n.close(true).as_deref(), Some(""));
		for i in 1..5 {
			assert_eq!(n.open(t0 + Duration::from_secs(i)), None);
			assert_eq!(n.close(false), None);
		}
		assert!(!n.idle(t0 + Duration::from_secs(60)));
		let line = n.open(t0 + cadence::STREAM_LOG_QUIET).unwrap();
		assert!(line.contains("4 more opened, 4 closed"), "{line}");
		assert_eq!(n.close(true).as_deref(), Some(""));
		assert!(n.idle(t0 + cadence::STREAM_LOG_QUIET * 2));
		assert_eq!(duration_words(Duration::from_secs(45)), "45 s");
		assert_eq!(duration_words(Duration::from_secs(200)), "3 min 20 s");
		assert_eq!(duration_words(Duration::from_secs(7500)), "2 h 5 min");
	}
}
