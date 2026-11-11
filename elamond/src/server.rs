//! The HTTPS API the app talks to. See docs/protocol.md.

use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime};

use axum::body::{Body, Bytes};
use axum::extract::{ConnectInfo, Extension, Query, Request, State};
use axum::http::{header, HeaderValue};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::watch;

use crate::access::{Can, Caller, RateLimiter, Role};
use crate::activity::Activity;
use crate::collect::HistoryRing;
use crate::consts::{cadence, limit, protocol, timeout};
use crate::control::{self, Acted};
use crate::errors::ErrorCode;
use crate::model::Process;
use crate::store::{self, Attempt};

/// What the collector publishes every tick.
pub struct Frame {
	/// The sample, already serialized: every stream sends the same text.
	pub sample: String,
	/// The same without what only some roles see (monitors, links), for phones that glance.
	pub basic: String,
	/// Media players and the volume, serialized, while some app asked for them; only the
	/// samples of those apps carry it.
	pub media: Option<String>,
	pub processes: Option<Arc<Vec<Process>>>,
	pub info: Arc<String>,
	/// Read while an app was connected, so links, displays, profiles and GPUs are in it.
	pub watched: bool,
}

pub struct Hub {
	pub id: String,
	pub name: String,
	pub frame: watch::Receiver<Arc<Frame>>,
	pub history: Mutex<HistoryRing>,
	/// Apps connected right now; heavier sources are read only while this is above zero.
	pub watchers: AtomicUsize,
	/// Apps that asked for the process list.
	pub proc_watchers: AtomicUsize,
	pub interval: Duration,
	/// Apps showing media controls, so players are only polled for them.
	pub media_watchers: AtomicUsize,
	/// Swapped whole when the settings files change; see `config()`.
	config: RwLock<Arc<store::Config>>,
	/// Points kept in the history ring.
	pub history_len: usize,
	pub bus: crate::bus::Bus,
	/// Links added from the app.
	pub app_links: tokio::sync::Mutex<Vec<crate::links::LinkDef>>,
	/// Set after an action, so the next watched tick re-reads what it may have changed.
	pub refresh: AtomicBool,
	clients: RwLock<ClientCache>,
	pair_lock: tokio::sync::Mutex<()>,
	/// The data directory: the paired phones, links added from the app, the activity log and
	/// `elamond control off`'s file.
	pub dir: PathBuf,
	/// `elamond control off` holds control off, as last looked.
	control_off: AtomicBool,
	/// How often each phone may act.
	pub rate: Mutex<RateLimiter>,
	pub activity: Activity,
	/// Unix time the computer booted, as read when the service started: process start times
	/// in samples count from it.
	pub boot_time: u64,
	/// Each phone's streams, for the journal.
	streams: Mutex<HashMap<String, StreamNotes>>,
	/// The certificate rotations that lead to the certificate presented now, as served.
	rotations: RwLock<Arc<String>>,
}

/// The hash of the token a request came with, so a stream can tell when it is revoked.
#[derive(Clone)]
struct TokenHash(String);

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
		history: HistoryRing,
		interval: Duration,
		config: Arc<store::Config>,
		dir: PathBuf,
	) -> Self {
		let app_links = crate::links::load_in(&dir).unwrap_or_else(|e| {
			eprintln!("links: {e}\n  the links added from the app are left out; the file is kept until one is saved");
			Vec::new()
		});
		Hub {
			id,
			name,
			frame,
			history_len: history.capacity(),
			history: Mutex::new(history),
			watchers: AtomicUsize::new(0),
			proc_watchers: AtomicUsize::new(0),
			interval,
			media_watchers: AtomicUsize::new(0),
			config: RwLock::new(config),
			bus: crate::bus::Bus::new(),
			app_links: tokio::sync::Mutex::new(app_links),
			refresh: AtomicBool::new(false),
			clients: RwLock::new(ClientCache { stamp: None, clients: Vec::new() }),
			pair_lock: tokio::sync::Mutex::new(()),
			control_off: AtomicBool::new(store::control_held_off(&dir)),
			rate: Mutex::new(RateLimiter::default()),
			activity: Activity::load(&dir),
			boot_time: sysinfo::System::boot_time().unwrap_or(0),
			streams: Mutex::new(HashMap::new()),
			rotations: RwLock::new(Arc::new("[]".into())),
			dir,
		}
	}

	/// The phone a token's hash belongs to, rereading clients.json when `pair`, `revoke` or
	/// `clients role` changed it. A file that stops parsing is reported and the phones read
	/// before keep working, rather than all being locked out.
	fn authorized(&self, hash: &str) -> Option<Caller> {
		let stamp = store::clients_stamp_in(&self.dir);
		let stale = self.clients.read().map(|c| c.stamp != stamp || stamp.is_none()).unwrap_or(true);
		if stale && let Ok(mut c) = self.clients.write() {
			match store::load_clients_in(&self.dir) {
				Ok(list) => {
					let fresh: Vec<(String, Caller)> =
						list.into_iter().map(|c| (c.token_hash, Caller { id: c.id, name: c.name, role: c.role })).collect();
					for (_, now) in &fresh {
						if let Some((_, before)) = c.clients.iter().find(|(_, b)| b.id == now.id)
							&& before.role != now.role
						{
							eprintln!("clients: \"{}\" ({}) is now {}", now.name, now.id, now.role.as_str());
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

	/// The settings as last read.
	pub fn config(&self) -> Arc<store::Config> {
		self.config.read().map(|c| c.clone()).unwrap_or_else(|p| p.into_inner().clone())
	}

	/// Whether phones may act, as last looked: `control = true` in the settings and no
	/// `elamond control off`.
	pub fn control_on(&self) -> bool {
		self.config().control && !self.control_off.load(Ordering::SeqCst)
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
		if let Ok(mut c) = self.config.write() {
			*c = Arc::new(new);
		}
		self.refresh_soon();
	}

	/// The rotations to serve from now on: those leading to the certificate presented.
	pub fn set_rotations(&self, chain: &[crate::identity::Rotation]) {
		let json = serde_json::to_string(chain).unwrap_or_else(|_| "[]".into());
		if let Ok(mut r) = self.rotations.write() {
			*r = Arc::new(json);
		}
	}

	/// Asks the collector to re-read links, profiles and media on its next watched tick.
	pub fn refresh_soon(&self) {
		self.refresh.store(true, Ordering::SeqCst);
	}

	/// Notes a stream opening; it is logged unless another of the phone's was a moment ago.
	/// Returns whether it was, so its closing is logged too.
	fn stream_opened(&self, who: &Caller, from: IpAddr, what: &str) -> bool {
		let now = Instant::now();
		let mut map = self.streams.lock().unwrap_or_else(|p| p.into_inner());
		map.retain(|id, n| *id == who.id || !n.idle(now));
		let n = map.entry(who.id.clone()).or_default();
		let Some(more) = n.open(now) else { return false };
		eprintln!("stream: \"{}\" ({}) opened from {}: {what}{more}", who.name, who.role.as_str(), from.to_canonical());
		true
	}

	fn stream_closed(&self, who: &Caller, logged: bool, lasted: Duration, why: &str) {
		let mut map = self.streams.lock().unwrap_or_else(|p| p.into_inner());
		let n = map.entry(who.id.clone()).or_default();
		if let Some(more) = n.close(logged) {
			eprintln!("stream: \"{}\" closed after {}: {why}{more}", who.name, duration_words(lasted));
		}
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
	let private = Router::new()
		.route("/api/v1/info", get(info))
		.route("/api/v1/snapshot", get(snapshot))
		.route("/api/v1/history", get(history))
		.route("/api/v1/stream", get(stream))
		.route("/api/v1/control", get(control::info))
		.route("/api/v1/control/log", get(control::log))
		.route("/api/v1/control/power", post(control::power))
		.route("/api/v1/control/profile", post(control::profile))
		.route("/api/v1/control/signal", post(control::signal))
		.route("/api/v1/control/action", post(control::action))
		.route("/api/v1/media", post(control::media))
		.route("/api/v1/media/volume", post(control::volume))
		.route("/api/v1/media/art", get(control::art))
		.route("/api/v1/links", get(control::links_list).post(control::link_put))
		.route("/api/v1/links/detected", get(control::detected))
		.route("/api/v1/links/{id}", delete(control::link_delete))
		.route("/api/v1/links/{id}/unit", post(control::link_unit))
		// What the links were called before 0.5, for apps 0.4.
		.route("/api/v1/services", get(control::links_list).post(control::link_put))
		.route("/api/v1/services/detected", get(control::detected))
		.route("/api/v1/services/{id}", delete(control::link_delete))
		.route("/api/v1/services/{id}/unit", post(control::link_unit))
		.route("/api/v1/units", get(control::units))
		.route("/api/v1/rekey", post(rekey))
		.route_layer(middleware::from_fn_with_state(hub.clone(), auth));
	Router::new()
		.route("/api/v1/hello", get(hello))
		.route("/api/v1/pair", post(pair))
		.route("/api/v1/identity/rotations", get(rotations))
		.merge(private)
		.fallback(|| async { error(ErrorCode::NotFound) })
		.with_state(hub)
}

pub fn error(code: ErrorCode) -> Response {
	(code.status(), Json(json!({ "error": code.as_str() }))).into_response()
}

/// `failed`, with what went wrong.
pub fn failed(message: impl Into<String>) -> Response {
	let code = ErrorCode::Failed;
	(code.status(), Json(json!({ "error": code.as_str(), "message": message.into() }))).into_response()
}

/// The token from an `Authorization: Bearer <token>` header; the scheme's case doesn't matter.
fn bearer(header: &str) -> Option<&str> {
	let (scheme, token) = header.trim_start().split_once(' ')?;
	let token = token.trim();
	(scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then_some(token)
}

async fn auth(State(hub): State<Arc<Hub>>, mut req: Request, next: Next) -> Response {
	let hash = req
		.headers()
		.get(header::AUTHORIZATION)
		.and_then(|v| v.to_str().ok())
		.and_then(bearer)
		.map(|t| store::sha256_hex(t.as_bytes()));
	match hash.and_then(|h| Some((hub.authorized(&h)?, h))) {
		Some((who, hash)) => {
			req.extensions_mut().insert(who);
			req.extensions_mut().insert(TokenHash(hash));
			next.run(req).await
		}
		None => error(ErrorCode::Unauthorized),
	}
}

/// Lets the app check that an address is an Elamon service, and which one, before pairing.
async fn hello(State(hub): State<Arc<Hub>>) -> Response {
	Json(hello_body(&hub.id, &hub.name)).into_response()
}

pub(crate) fn hello_body(id: &str, name: &str) -> serde_json::Value {
	json!({
		"app": "elamon",
		"api": protocol::API_VERSION,
		"id": id,
		"name": name,
		"version": env!("CARGO_PKG_VERSION"),
		"min_app": protocol::MIN_APP,
	})
}

pub(crate) fn paired_body(token: &str, client_id: &str, id: &str, name: &str, role: Role) -> serde_json::Value {
	json!({ "token": token, "client_id": client_id, "id": id, "name": name, "role": role })
}

#[derive(Deserialize)]
struct PairRequest {
	code: String,
	#[serde(default)]
	device: String,
}

async fn pair(
	State(hub): State<Arc<Hub>>,
	ConnectInfo(peer): ConnectInfo<SocketAddr>,
	Json(req): Json<PairRequest>,
) -> Response {
	// One attempt at a time, each wrong one slowed down, a few per address and some more in all.
	let _guard = hub.pair_lock.lock().await;
	let Some(mut pending) = store::load_pairing() else {
		tokio::time::sleep(timeout::NO_PAIRING_DELAY).await;
		return error(ErrorCode::NoPairing);
	};
	let ip = peer.ip().to_canonical();
	let from = store::address_key(ip);
	let code: String = req.code.chars().filter(char::is_ascii_digit).collect();
	match pending.attempt(&from, &code) {
		// Answered at once: waiting here would hold up everyone else's attempts.
		Attempt::Blocked => return error(ErrorCode::TooManyRequests),
		Attempt::Wrong => {
			if pending.attempts >= limit::PAIRING_ATTEMPTS {
				store::clear_pairing();
				eprintln!("pairing: wrong code from {ip}; the code is used up");
			} else {
				let _ = store::save_pairing(&pending);
				let tried = pending.wrong.get(&from).copied().unwrap_or(0);
				if tried >= limit::PAIRING_ATTEMPTS_PER_ADDRESS {
					eprintln!("pairing: wrong code from {ip}; {from} may not try this code again");
				} else {
					eprintln!("pairing: wrong code from {ip}");
				}
			}
			tokio::time::sleep(timeout::WRONG_CODE_DELAY).await;
			return error(ErrorCode::WrongCode);
		}
		Attempt::Right => {}
	}
	// The phone is added before the code is withdrawn: `elamond pair` takes a code gone with
	// no new phone for one used up by wrong attempts.
	match store::add_client(&req.device, pending.role) {
		Ok((client, token)) => {
			store::clear_pairing();
			eprintln!("pairing: added \"{}\" ({}) as {} from {ip}", client.name, client.id, client.role.as_str());
			Json(paired_body(&token, &client.id, &hub.id, &hub.name, client.role)).into_response()
		}
		Err(e) => {
			eprintln!("pairing: can't save the client: {e}");
			error(ErrorCode::Storage)
		}
	}
}

#[derive(Deserialize)]
struct RekeyRequest {
	#[serde(default)]
	device: String,
}

/// A key of its own for a phone that holds another's token, as one does after Import pairings:
/// a new entry named `device`, with the role of the entry whose token asked and a new token, the
/// same answer as pairing's. That token stays good, so the phone it was copied from keeps working
/// until it is revoked on its own. Any role may ask, and none is raised by it; it counts as acting
/// (the rate and the activity log), but needs no control.
async fn rekey(
	State(hub): State<Arc<Hub>>,
	Extension(who): Extension<Caller>,
	ConnectInfo(peer): ConnectInfo<SocketAddr>,
	Json(req): Json<RekeyRequest>,
) -> Response {
	let name = store::sanitize_name(&req.device);
	let ip = peer.ip().to_canonical();
	hub.act_as(&who, None, format!("phone added: {name}"), async {
		// One write to the phones at a time, pairing's included.
		let _guard = hub.pair_lock.lock().await;
		match store::add_client_from(&hub.dir, &who.id, &name, who.role) {
			Ok((client, token)) => {
				eprintln!(
					"pairing: added \"{}\" ({}) as {} from {ip}, asked with the key of \"{}\" ({}), which stays paired",
					client.name,
					client.id,
					client.role.as_str(),
					who.name,
					who.id
				);
				let response = Json(paired_body(&token, &client.id, &hub.id, &hub.name, client.role)).into_response();
				Ok(Acted { response, note: Some(format!("id {}, {}", client.id, client.role.as_str())) })
			}
			Err(e) => {
				eprintln!("pairing: can't save the phone \"{}\" ({}) asked to add: {e}", who.name, who.id);
				Err(ErrorCode::Storage.into())
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

async fn info(State(hub): State<Arc<Hub>>) -> Response {
	let info = hub.frame.borrow().info.clone();
	json_response(info.as_str().to_owned())
}

async fn history(State(hub): State<Arc<Hub>>) -> Response {
	let h = hub.history.lock().map(|h| h.export()).unwrap_or_default();
	Json(h).into_response()
}

#[derive(Deserialize, Default)]
struct StreamParams {
	/// Milliseconds between samples; the service's own interval at the least.
	interval: Option<u64>,
	/// How many processes to include, heaviest first; none by default.
	procs: Option<usize>,
	/// "cpu" (default), "mem", "gpu" or "io".
	sort: Option<String>,
	/// Send the last minutes of the main series first.
	history: Option<u8>,
	/// Include media players and the volume (the volume needs control).
	media: Option<u8>,
}

/// What one app asked to see in its samples, or as much of it as its role allows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Want {
	pub(crate) procs: usize,
	pub(crate) sort: Option<String>,
	pub(crate) media: bool,
	/// Only the basics: no monitors and no links.
	pub(crate) basic: bool,
}

impl Want {
	fn new(p: &StreamParams) -> Self {
		Want {
			procs: p.procs.unwrap_or(0).min(limit::PROCS_MAX),
			sort: p.sort.clone(),
			media: p.media.unwrap_or(0) != 0,
			basic: false,
		}
	}

	/// What of it a phone with `role` gets.
	pub(crate) fn allowed(&self, role: Role) -> Want {
		Want {
			procs: if role.may(Can::Processes) { self.procs } else { 0 },
			sort: self.sort.clone(),
			media: self.media && role.may(Can::MediaRead),
			basic: self.basic || !(role.may(Can::Displays) && role.may(Can::Links)),
		}
	}

	/// Whether a frame was read with everything this app asked for.
	fn complete(&self, f: &Frame) -> bool {
		f.watched && (self.procs == 0 || f.processes.is_some()) && (!self.media || f.media.is_some())
	}

	/// The parameters that matter, for the journal.
	fn words(&self, interval: Duration) -> String {
		let mut w = vec![format!("every {} ms", interval.as_millis())];
		if self.procs > 0 {
			w.push(format!("{} processes", self.procs));
		}
		if self.media {
			w.push("media".into());
		}
		w.join(", ")
	}
}

/// Counts an app as watching for as long as it is held.
struct Watch {
	hub: Arc<Hub>,
	procs: bool,
	media: bool,
}

impl Watch {
	fn new(hub: &Arc<Hub>, want: &Want) -> Self {
		let (procs, media) = (want.procs > 0, want.media);
		hub.watchers.fetch_add(1, Ordering::SeqCst);
		if procs {
			hub.proc_watchers.fetch_add(1, Ordering::SeqCst);
		}
		if media {
			hub.media_watchers.fetch_add(1, Ordering::SeqCst);
		}
		Watch { hub: hub.clone(), procs, media }
	}
}

impl Drop for Watch {
	fn drop(&mut self) {
		self.hub.watchers.fetch_sub(1, Ordering::SeqCst);
		if self.procs {
			self.hub.proc_watchers.fetch_sub(1, Ordering::SeqCst);
		}
		if self.media {
			self.hub.media_watchers.fetch_sub(1, Ordering::SeqCst);
		}
	}
}

/// One sample. The request counts as watching until a reading made for it arrives (links,
/// displays, profiles, GPUs, and processes or media when asked for), a few ticks at most.
async fn snapshot(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Query(p): Query<StreamParams>) -> Response {
	let want = Want::new(&p).allowed(who.role);
	let mut rx = hub.frame.clone();
	let _watch = Watch::new(&hub, &want);
	let deadline = tokio::time::Instant::now() + hub.interval * cadence::FIRST_FRAME_TICKS;
	let mut frame = rx.borrow_and_update().clone();
	while !want.complete(&frame) {
		match tokio::time::timeout_at(deadline, rx.changed()).await {
			Ok(Ok(())) => frame = rx.borrow_and_update().clone(),
			_ => break,
		}
	}
	json_response(render_sample(&frame, &want, false))
}

/// Says in the journal when a stream ends, how long it lasted and why, if its start was.
struct StreamLog {
	hub: Arc<Hub>,
	who: Caller,
	opened: Instant,
	logged: bool,
	why: &'static str,
}

impl Drop for StreamLog {
	fn drop(&mut self) {
		self.hub.stream_closed(&self.who, self.logged, self.opened.elapsed(), self.why);
	}
}

async fn stream(
	State(hub): State<Arc<Hub>>,
	Extension(who): Extension<Caller>,
	Extension(token): Extension<TokenHash>,
	ConnectInfo(peer): ConnectInfo<SocketAddr>,
	Query(p): Query<StreamParams>,
) -> Response {
	let asked = Want::new(&p);
	let want = asked.allowed(who.role);
	let interval = Duration::from_millis(p.interval.unwrap_or(0)).clamp(hub.interval, cadence::STREAM_INTERVAL_MAX);
	let mut rx = hub.frame.clone();
	let frame = rx.borrow_and_update().clone();

	let mut pending = VecDeque::new();
	pending.push_back(tagged("info", &frame.info));
	if p.history.unwrap_or(0) != 0 {
		let h = hub.history.lock().map(|h| h.export()).unwrap_or_default();
		pending.push_back(tagged("history", &serde_json::to_string(&h).unwrap_or_else(|_| "{}".into())));
	}

	struct St {
		rx: watch::Receiver<Arc<Frame>>,
		pending: VecDeque<String>,
		info: Arc<String>,
		last: Option<Instant>,
		/// Frames passed over while waiting for the first one made for this app.
		skipped: u32,
		interval: Duration,
		/// What the app asked for, and what of it its role allows.
		asked: Want,
		want: Want,
		role: Role,
		token: TokenHash,
		watch: Watch,
		log: StreamLog,
	}
	let logged = hub.stream_opened(&who, peer.ip(), &asked.words(interval));
	let st = St {
		rx,
		pending,
		info: frame.info.clone(),
		last: None,
		skipped: 0,
		interval,
		watch: Watch::new(&hub, &want),
		asked,
		want,
		role: who.role,
		token,
		log: StreamLog { hub: hub.clone(), who, opened: Instant::now(), logged, why: "connection closed" },
	};

	let body = futures_util::stream::unfold(st, |mut st| async move {
		loop {
			if let Some(line) = st.pending.pop_front() {
				return Some((Ok::<_, Infallible>(Bytes::from(line)), st));
			}
			if st.rx.changed().await.is_err() {
				st.log.why = "the service is stopping";
				drop(st);
				return None;
			}
			// Revoking a phone ends its stream within a tick, not when it next reconnects.
			let hub = st.watch.hub.clone();
			let Some(who) = hub.authorized(&st.token.0) else {
				st.log.why = "the phone was unpaired";
				drop(st);
				return None;
			};
			// A new role applies from the next sample on, more or less.
			if who.role != st.role {
				st.role = who.role;
				st.want = st.asked.allowed(who.role);
				st.watch = Watch::new(&hub, &st.want);
			}
			let frame = st.rx.borrow_and_update().clone();
			if !Arc::ptr_eq(&frame.info, &st.info) && frame.info != st.info {
				st.info = frame.info.clone();
				st.pending.push_back(tagged("info", &frame.info));
			}
			// The first sample waits for a reading made for this app, as a snapshot does.
			if st.last.is_none() && !st.want.complete(&frame) && st.skipped < cadence::FIRST_FRAME_TICKS {
				st.skipped += 1;
				continue;
			}
			// Ticks jitter a little; a tenth of the interval of slack keeps 2 s from becoming 3 s.
			let due = st.last.is_none_or(|l| l.elapsed() + st.interval / 10 >= st.interval);
			if due {
				st.last = Some(Instant::now());
				st.pending.push_back(render_sample(&frame, &st.want, true));
			}
		}
	});

	let mut resp = Body::from_stream(body).into_response();
	let h = resp.headers_mut();
	h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/x-ndjson"));
	h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
	resp
}

fn json_response(body: String) -> Response {
	([(header::CONTENT_TYPE, "application/json")], body).into_response()
}

/// `{"type":"<kind>", ...fields of obj}` followed by a newline.
fn tagged(kind: &str, obj: &str) -> String {
	let inner = obj.trim().strip_prefix('{').unwrap_or("}");
	let sep = if inner.trim_start().starts_with('}') { "" } else { "," };
	format!("{{\"type\":\"{kind}\"{sep}{inner}\n")
}

/// The shared sample, or its basics, with what this app asked for added: media, and its
/// heaviest processes.
pub(crate) fn render_sample(frame: &Frame, want: &Want, line: bool) -> String {
	let mut extra = String::new();
	if want.media
		&& let Some(media) = &frame.media
	{
		extra.push_str(",\"media\":");
		extra.push_str(media);
	}
	if want.procs > 0
		&& let Some(all) = &frame.processes
	{
		let mut list: Vec<&Process> = all.iter().collect();
		let key = |p: &Process| -> f64 {
			match want.sort.as_deref() {
				Some("mem") => p.mem as f64,
				Some("gpu") => p.gpu.unwrap_or(0.0) as f64 * 1e12 + p.gpu_mem.unwrap_or(0) as f64,
				Some("io") => p.read + p.write,
				// CPU first, memory to order the many idle ones.
				_ => p.cpu as f64 * 1e15 + p.mem as f64,
			}
		};
		list.sort_by(|a, b| key(b).total_cmp(&key(a)));
		list.truncate(want.procs);
		let procs_json = serde_json::to_string(&list).unwrap_or_else(|_| "[]".into());
		extra.push_str(&format!(",\"process_count\":{},\"processes\":{procs_json}", all.len()));
	}
	let mut s = if want.basic { frame.basic.clone() } else { frame.sample.clone() };
	if !extra.is_empty() && s.ends_with('}') {
		s.pop();
		s.push_str(&extra);
		s.push('}');
	}
	if line { tagged("sample", &s) } else { s }
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

	fn process(pid: u32, cpu: f32, mem: u64, io: f64, gpu: Option<f32>) -> Process {
		Process {
			pid,
			ppid: None,
			name: format!("p{pid}"),
			cmd: String::new(),
			user: None,
			cpu,
			mem,
			status: "Running".into(),
			start: 0,
			read: io,
			write: 0.0,
			gpu,
			gpu_mem: None,
		}
	}

	fn frame() -> Frame {
		Frame {
			sample: "{\"t\":1}".into(),
			basic: "{\"t\":1}".into(),
			media: Some("{\"players\":[],\"volume\":null}".into()),
			processes: Some(Arc::new(vec![
				process(1, 0.0, 900, 0.0, None),
				process(2, 50.0, 10, 5.0, None),
				process(3, 0.0, 100, 99.0, Some(30.0)),
				process(4, 50.0, 20, 0.0, Some(1.0)),
			])),
			info: Arc::new("{}".into()),
			watched: true,
		}
	}

	fn order(want: Want) -> Vec<u32> {
		let v: serde_json::Value = serde_json::from_str(&render_sample(&frame(), &want, false)).unwrap();
		v["processes"].as_array().unwrap().iter().map(|p| p["pid"].as_u64().unwrap() as u32).collect()
	}

	fn want(procs: usize, sort: Option<&str>) -> Want {
		Want { procs, sort: sort.map(str::to_string), media: false, basic: false }
	}

	#[test]
	fn processes_sort_heaviest_first() {
		// CPU, then memory among equals.
		assert_eq!(order(want(10, None)), [4, 2, 1, 3]);
		assert_eq!(order(want(10, Some("mem"))), [1, 3, 4, 2]);
		assert_eq!(order(want(10, Some("io"))), [3, 2, 1, 4]);
		assert_eq!(order(want(2, Some("gpu"))), [3, 4]);
	}

	#[test]
	fn media_and_processes_only_for_those_who_asked() {
		let f = frame();
		assert_eq!(render_sample(&f, &want(0, None), false), "{\"t\":1}");
		let v: serde_json::Value = serde_json::from_str(&render_sample(&f, &Want { procs: 1, sort: None, media: true, basic: false }, false)).unwrap();
		assert_eq!(v["process_count"], 4);
		assert_eq!(v["processes"].as_array().unwrap().len(), 1);
		assert!(v["media"]["players"].is_array());
		let line = render_sample(&f, &want(0, None), true);
		assert_eq!(line, "{\"type\":\"sample\",\"t\":1}\n");
	}

	#[test]
	fn tagged_lines() {
		assert_eq!(tagged("info", "{\"a\":1}"), "{\"type\":\"info\",\"a\":1}\n");
		assert_eq!(tagged("history", "{}"), "{\"type\":\"history\"}\n");
		assert_eq!(tagged("x", " {\"a\":[]} "), "{\"type\":\"x\",\"a\":[]}\n");
	}

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

	// ---- roles, through the router as a phone meets it -----------------------------------

	use crate::store::Client;
	use axum::http::{Method, StatusCode};
	use futures_util::StreamExt;
	use tower::ServiceExt;

	/// The phones every test hub has: one for each role, its token "t-<role>", and one paired
	/// before roles, whose entry in clients.json has none ("t-old").
	const PHONES: [&str; 5] = ["glance", "monitor", "operate", "admin", "old"];

	fn scratch() -> PathBuf {
		let dir = std::env::temp_dir().join(format!("elamond-hub-{}", data_encoding::HEXLOWER.encode(&store::random_bytes::<6>())));
		std::fs::create_dir_all(&dir).unwrap();
		dir
	}

	fn client(name: &str, role: Role) -> Client {
		Client { id: format!("id-{name}"), name: name.into(), token_hash: store::sha256_hex(format!("t-{name}").as_bytes()), created: 1, role, from: None }
	}

	fn full_frame() -> Frame {
		let s = crate::fixtures::sample();
		Frame {
			sample: serde_json::to_string(&s).unwrap(),
			basic: serde_json::to_string(&s.into_basic()).unwrap(),
			media: Some(serde_json::to_string(&crate::fixtures::media()).unwrap()),
			processes: Some(Arc::new(crate::fixtures::processes())),
			info: Arc::new("{\"id\":\"x\"}".into()),
			watched: true,
		}
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
		let (tx, rx) = watch::channel(Arc::new(full_frame()));
		let config = store::Config { control, mdns: false, ..Default::default() };
		let hub = Hub::new("id".into(), "test".into(), rx, HistoryRing::new(10), Duration::from_millis(250), Arc::new(config), dir);
		*hub.rate.lock().unwrap() = RateLimiter::new(10_000, Duration::from_secs(1));
		(Arc::new(hub), tx)
	}

	fn request(method: &str, path: &str, phone: &str, body: &str) -> Request {
		request_as(method, path, &format!("t-{phone}"), body)
	}

	fn request_as(method: &str, path: &str, token: &str, body: &str) -> Request {
		axum::http::Request::builder()
			.method(Method::from_bytes(method.as_bytes()).unwrap())
			.uri(path)
			.header(header::AUTHORIZATION, format!("Bearer {token}"))
			.header(header::CONTENT_TYPE, "application/json")
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
		if path.starts_with("/api/v1/stream") {
			return (status, serde_json::Value::Null);
		}
		let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
		(status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
	}

	/// Every route behind a token and what it needs, with a body that reads but asks for nothing
	/// that could change the computer: an unknown power action, signal, player or link, an
	/// empty name. Every route in `router` is here.
	const ROUTES: &[(&str, &str, &str, Option<Can>)] = &[
		("GET", "/api/v1/info", "", None),
		("GET", "/api/v1/snapshot", "", None),
		("GET", "/api/v1/history", "", None),
		("GET", "/api/v1/stream", "", None),
		("GET", "/api/v1/control", "", None),
		("GET", "/api/v1/control/log", "", Some(Can::Log)),
		("POST", "/api/v1/control/power", r#"{"action":"nothing"}"#, Some(Can::Power)),
		("POST", "/api/v1/control/profile", r#"{"profile":"nothing"}"#, Some(Can::Profile)),
		("POST", "/api/v1/control/signal", r#"{"pid":0,"signal":"nothing"}"#, Some(Can::Signal)),
		("POST", "/api/v1/control/action", r#"{"id":"nothing"}"#, Some(Can::Actions)),
		("POST", "/api/v1/media", r#"{"player":"nothing","op":"play_pause"}"#, Some(Can::Media)),
		("POST", "/api/v1/media/volume", "{}", Some(Can::Volume)),
		("GET", "/api/v1/media/art?player=nothing", "", Some(Can::MediaRead)),
		("GET", "/api/v1/links", "", Some(Can::Links)),
		("POST", "/api/v1/links", r#"{"name":"","url":""}"#, Some(Can::LinksEdit)),
		("GET", "/api/v1/links/detected", "", Some(Can::Links)),
		("DELETE", "/api/v1/links/nothing", "", Some(Can::LinksEdit)),
		("POST", "/api/v1/links/nothing/unit", r#"{"op":"restart"}"#, Some(Can::Units)),
		("GET", "/api/v1/services", "", Some(Can::Links)),
		("POST", "/api/v1/services", r#"{"name":"","url":""}"#, Some(Can::LinksEdit)),
		("GET", "/api/v1/services/detected", "", Some(Can::Links)),
		("DELETE", "/api/v1/services/nothing", "", Some(Can::LinksEdit)),
		("POST", "/api/v1/services/nothing/unit", r#"{"op":"restart"}"#, Some(Can::Units)),
		("GET", "/api/v1/units", "", Some(Can::Links)),
		("POST", "/api/v1/rekey", r#"{"device":"Route phone"}"#, None),
	];

	fn role_of(phone: &str) -> Role {
		Role::parse(phone).unwrap_or(Role::Admin)
	}

	#[tokio::test]
	async fn every_route_for_every_role() {
		for control in [true, false] {
			let (hub, _tx) = hub(control);
			for phone in PHONES {
				for (method, path, body, can) in ROUTES {
					let (status, v) = call(&hub, method, path, phone, body).await;
					let error = v.get("error").and_then(|e| e.as_str()).unwrap_or("");
					assert!(
						!matches!(status.as_u16(), 401 | 405 | 422 | 429) && error != "not_found",
						"{method} {path} as {phone}: {status} {v}"
					);
					let got = match error {
						"not_allowed" | "control_disabled" => error,
						_ => "let through",
					};
					let expected = match can {
						Some(c) if !role_of(phone).may(*c) => "not_allowed",
						Some(c) if c.needs_control() && !control => "control_disabled",
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
			let acting = ROUTES.iter().filter(|(m, ..)| *m != "GET").count();
			assert_eq!(log.len(), acting * PHONES.len());
			let power = |phone: &str| log.iter().find(|e| e.phone == phone && e.what == "power nothing").unwrap().clone();
			assert_eq!((power("glance").result.as_str(), power("glance").client_id.as_str()), ("not_allowed", "id-glance"));
			if control {
				assert_eq!((power("old").result.as_str(), power("old").message.as_deref()), ("failed", Some("unknown_action")));
			} else {
				assert_eq!(power("admin").result, "control_disabled");
			}
			assert!(log.windows(2).all(|w| w[0].t >= w[1].t));
			std::fs::remove_dir_all(&hub.dir).unwrap();
		}
	}

	#[tokio::test]
	async fn control_tells_each_phone_its_role_and_what_it_may_do() {
		let (hub, _tx) = hub(true);
		for phone in PHONES {
			let (_, v) = call(&hub, "GET", "/api/v1/control", phone, "").await;
			let role = role_of(phone);
			assert_eq!(v["role"], role.as_str(), "{phone}");
			assert_eq!(v["allowed"], serde_json::json!(crate::access::allowed(role, true)), "{phone}");
			assert_eq!(v["enabled"], true);
			if role < Role::Admin {
				assert_eq!(v["power"]["reboot"], "na", "{phone}");
			}
			if role < Role::Operate {
				assert!(v["profiles"].is_null() && v["actions"] == serde_json::json!([]) && v["volume"] == false, "{phone}: {v}");
			}
		}
		// Held off: still on in the settings, off for everyone.
		store::set_control_off(&hub.dir, true).unwrap();
		let (_, v) = call(&hub, "GET", "/api/v1/control", "admin", "").await;
		assert_eq!((v["enabled"].clone(), v["allowed"].clone()), (serde_json::json!(false), serde_json::json!(crate::access::allowed(Role::Admin, false))));
		let (status, v) = call(&hub, "POST", "/api/v1/control/power", "admin", r#"{"action":"nothing"}"#).await;
		assert_eq!((status, v["error"].as_str()), (StatusCode::FORBIDDEN, Some("control_disabled")));
		assert!(!hub.control_on());
		store::set_control_off(&hub.dir, false).unwrap();
		let (status, _) = call(&hub, "POST", "/api/v1/control/power", "admin", r#"{"action":"nothing"}"#).await;
		assert_eq!(status, StatusCode::BAD_GATEWAY);
		std::fs::remove_dir_all(&hub.dir).unwrap();
	}

	#[tokio::test]
	async fn a_phone_that_acts_too_often_is_slowed_down() {
		let (hub, _tx) = hub(true);
		*hub.rate.lock().unwrap() = RateLimiter::default();
		let mut statuses = Vec::new();
		for _ in 0..limit::ACTS_BURST + 3 {
			statuses.push(call(&hub, "POST", "/api/v1/control/power", "admin", r#"{"action":"nothing"}"#).await.0);
		}
		assert!(statuses[..limit::ACTS_BURST as usize].iter().all(|s| *s == StatusCode::BAD_GATEWAY), "{statuses:?}");
		assert!(statuses[limit::ACTS_BURST as usize..].iter().all(|s| *s == StatusCode::TOO_MANY_REQUESTS), "{statuses:?}");
		// Another phone isn't held up, and reading isn't counted.
		assert_eq!(call(&hub, "POST", "/api/v1/control/power", "old", r#"{"action":"nothing"}"#).await.0, StatusCode::BAD_GATEWAY);
		assert_eq!(call(&hub, "GET", "/api/v1/control", "admin", "").await.0, StatusCode::OK);
		// Only the first refusal of the run is logged.
		let log = hub.activity.newest(usize::MAX);
		assert_eq!(log.iter().filter(|e| e.result == "too_many_requests").count(), 1);
		std::fs::remove_dir_all(&hub.dir).unwrap();
	}

	#[tokio::test]
	async fn a_phone_gets_a_key_of_its_own_beside_the_one_it_came_with() {
		// Control off: a key of its own needs none, whatever the role.
		let (hub, _tx) = hub(false);
		for phone in PHONES {
			let role = role_of(phone);
			let before = store::load_clients_in(&hub.dir).unwrap();
			// A role asked for in the body is no more than a word in it.
			let (status, v) = call(&hub, "POST", "/api/v1/rekey", phone, r#"{"device":" New\u0007 phone ","role":"admin"}"#).await;
			assert_eq!(status, StatusCode::OK, "{phone}: {v}");
			assert_eq!((v["role"].as_str(), v["id"].as_str(), v["name"].as_str()), (Some(role.as_str()), Some("id"), Some("test")), "{phone}");
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

			// Both tokens work, each as its own phone with the same role.
			for t in [format!("t-{phone}"), token.clone()] {
				let (status, v) = call_as(&hub, "GET", "/api/v1/control", &t, "").await;
				assert_eq!((status, v["role"].as_str()), (StatusCode::OK, Some(role.as_str())), "{phone}");
			}
			let (_, v) = call_as(&hub, "GET", "/api/v1/snapshot?procs=5", &token, "").await;
			assert_eq!(v.get("processes").is_some(), role >= Role::Monitor, "{phone}");

			// The old phone revoked, the new one carries on.
			let mut list = store::load_clients_in(&hub.dir).unwrap();
			list.retain(|c| c.id != from);
			store::save_clients_in(&hub.dir, &list).unwrap();
			assert_eq!(call(&hub, "GET", "/api/v1/info", phone, "").await.0, StatusCode::UNAUTHORIZED);
			assert_eq!(call_as(&hub, "GET", "/api/v1/info", &token, "").await.0, StatusCode::OK);
			let (_, v) = call_as(&hub, "GET", "/api/v1/control", &token, "").await;
			assert_eq!(v["role"].as_str(), Some(role.as_str()), "{phone}");
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
	async fn keys_of_their_own_are_rate_limited_and_never_written_over_a_damaged_file() {
		let (hub, _tx) = hub(true);
		*hub.rate.lock().unwrap() = RateLimiter::default();
		let mut statuses = Vec::new();
		for _ in 0..limit::ACTS_BURST + 2 {
			statuses.push(call(&hub, "POST", "/api/v1/rekey", "monitor", r#"{"device":"Again"}"#).await.0);
		}
		assert!(statuses[..limit::ACTS_BURST as usize].iter().all(|s| *s == StatusCode::OK), "{statuses:?}");
		assert!(statuses[limit::ACTS_BURST as usize..].iter().all(|s| *s == StatusCode::TOO_MANY_REQUESTS), "{statuses:?}");
		let log = hub.activity.newest(usize::MAX);
		assert_eq!(log.iter().filter(|e| e.result == "too_many_requests").count(), 1);
		assert_eq!(store::load_clients_in(&hub.dir).unwrap().len(), PHONES.len() + limit::ACTS_BURST as usize);

		// A file of phones that no longer parses is left alone: the phones read before keep working,
		// and no new one is added over them.
		std::fs::write(hub.dir.join(store::CLIENTS_FILE), "[{broken").unwrap();
		let (status, v) = call(&hub, "POST", "/api/v1/rekey", "admin", r#"{"device":"Over"}"#).await;
		assert_eq!((status, v["error"].as_str()), (StatusCode::INTERNAL_SERVER_ERROR, Some("storage")));
		assert_eq!(std::fs::read_to_string(hub.dir.join(store::CLIENTS_FILE)).unwrap(), "[{broken");
		assert_eq!(call(&hub, "GET", "/api/v1/info", "glance", "").await.0, StatusCode::OK);
		std::fs::remove_dir_all(&hub.dir).unwrap();
	}

	#[tokio::test]
	async fn snapshots_carry_what_the_role_allows() {
		let (hub, _tx) = hub(true);
		for phone in PHONES {
			let (_, v) = call(&hub, "GET", "/api/v1/snapshot?procs=5&media=1", phone, "").await;
			let all = role_of(phone) >= Role::Monitor;
			assert_eq!(v.get("processes").is_some(), all, "{phone}");
			assert_eq!(v.get("process_count").is_some(), all, "{phone}");
			assert_eq!(v.get("media").is_some(), all, "{phone}");
			assert_eq!(v["displays"].as_array().unwrap().is_empty(), !all, "{phone}");
			assert_eq!(v["links"].as_array().unwrap().is_empty(), !all, "{phone}");
			assert_eq!(v["services"].as_array().unwrap().is_empty(), !all, "{phone}");
			assert!(v["cpu"]["usage"].is_number() && v["net"].is_array() && v["battery"].is_object(), "{phone}");
		}
		std::fs::remove_dir_all(&hub.dir).unwrap();
	}

	#[tokio::test]
	async fn an_open_stream_follows_its_phones_role() {
		let (hub, tx) = hub(true);
		let resp = router(hub.clone()).oneshot(request("GET", "/api/v1/stream?procs=5&media=1", "monitor", "")).await.unwrap();
		let mut body = resp.into_body().into_data_stream();
		let mut next = async || -> Option<serde_json::Value> {
			let chunk = tokio::time::timeout(Duration::from_secs(5), body.next()).await.expect("a line in time")?.unwrap();
			Some(serde_json::from_slice(&chunk).unwrap())
		};
		assert_eq!(next().await.unwrap()["type"], "info");
		let mut sample = async |tx: &watch::Sender<Arc<Frame>>| {
			// Samples come an interval apart at the most often.
			tokio::time::sleep(Duration::from_millis(300)).await;
			tx.send_replace(Arc::new(full_frame()));
			next().await
		};
		let full = sample(&tx).await.unwrap();
		assert!(full["processes"].is_array() && full["media"].is_object() && !full["displays"].as_array().unwrap().is_empty());
		assert_eq!((hub.watchers.load(Ordering::SeqCst), hub.proc_watchers.load(Ordering::SeqCst)), (1, 1));

		let set_role = |role: Option<Role>| {
			let mut list = store::load_clients_in(&hub.dir).unwrap();
			match role {
				Some(r) => list.iter_mut().find(|c| c.name == "monitor").unwrap().role = r,
				None => list.retain(|c| c.name != "monitor"),
			}
			store::save_clients_in(&hub.dir, &list).unwrap();
		};
		set_role(Some(Role::Glance));
		let basic = sample(&tx).await.unwrap();
		assert!(basic.get("processes").is_none() && basic.get("media").is_none(), "{basic}");
		assert_eq!((basic["displays"].clone(), basic["links"].clone()), (serde_json::json!([]), serde_json::json!([])));
		assert_eq!((hub.watchers.load(Ordering::SeqCst), hub.proc_watchers.load(Ordering::SeqCst)), (1, 0));
		assert_eq!(hub.media_watchers.load(Ordering::SeqCst), 0);

		set_role(Some(Role::Monitor));
		let again = sample(&tx).await.unwrap();
		assert!(again["processes"].is_array() && again["media"].is_object(), "{again}");

		// Unpaired: the stream ends, and stops counting as watching.
		set_role(None);
		assert!(sample(&tx).await.is_none());
		assert_eq!(hub.watchers.load(Ordering::SeqCst), 0);
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
