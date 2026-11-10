//! The HTTPS API the app talks to. See docs/protocol.md.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::net::SocketAddr;
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

use crate::collect::HistoryRing;
use crate::consts::{cadence, limit, protocol, timeout};
use crate::control;
use crate::errors::ErrorCode;
use crate::model::Process;
use crate::store;

/// What the collector publishes every tick.
pub struct Frame {
	/// The sample, already serialized: every stream sends the same text.
	pub sample: String,
	/// Media players and the volume, serialized, while some app asked for them; only the
	/// samples of those apps carry it.
	pub media: Option<String>,
	pub processes: Option<Arc<Vec<Process>>>,
	pub info: Arc<String>,
	/// Read while an app was connected, so services, displays, profiles and GPUs are in it.
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
	/// Services added from the app.
	pub app_services: tokio::sync::Mutex<Vec<crate::services::ServiceDef>>,
	/// Set after an action, so the next watched tick re-reads what it may have changed.
	pub refresh: AtomicBool,
	clients: RwLock<ClientCache>,
	pair_lock: tokio::sync::Mutex<()>,
}

/// The paired phone a request comes from, for the log.
#[derive(Clone)]
pub struct ClientName(pub String);

/// The hash of the token a request came with, so a stream can tell when it is revoked.
#[derive(Clone)]
struct TokenHash(String);

struct ClientCache {
	stamp: Option<(SystemTime, u64, u64)>,
	/// Token hashes and the phones' names.
	clients: Vec<(String, String)>,
}

impl Hub {
	pub fn new(
		id: String,
		name: String,
		frame: watch::Receiver<Arc<Frame>>,
		history: HistoryRing,
		interval: Duration,
		config: Arc<store::Config>,
	) -> Self {
		let app_services = crate::services::load().unwrap_or_else(|e| {
			eprintln!("services: {e}\n  the services added from the app are left out; the file is kept until one is saved");
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
			app_services: tokio::sync::Mutex::new(app_services),
			refresh: AtomicBool::new(false),
			clients: RwLock::new(ClientCache { stamp: None, clients: Vec::new() }),
			pair_lock: tokio::sync::Mutex::new(()),
		}
	}

	/// Checks a token's hash against clients.json, rereading it when `pair` or `revoke`
	/// changed it. Returns the phone's name. A file that stops parsing is reported and the
	/// phones read before keep working, rather than all being locked out.
	fn authorized(&self, hash: &str) -> Option<String> {
		let stamp = store::clients_stamp();
		let stale = self.clients.read().map(|c| c.stamp != stamp || stamp.is_none()).unwrap_or(true);
		if stale && let Ok(mut c) = self.clients.write() {
			match store::load_clients() {
				Ok(list) => c.clients = list.into_iter().map(|c| (c.token_hash, c.name)).collect(),
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
			.map(|(_, name)| name.clone())
	}

	/// The settings as last read.
	pub fn config(&self) -> Arc<store::Config> {
		self.config.read().map(|c| c.clone()).unwrap_or_else(|p| p.into_inner().clone())
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
			eprintln!("settings: control is now {}", if new.control { "on" } else { "off" });
		}
		if let Ok(mut c) = self.config.write() {
			*c = Arc::new(new);
		}
		self.refresh_soon();
	}

	/// Asks the collector to re-read services, profiles and media on its next watched tick.
	pub fn refresh_soon(&self) {
		self.refresh.store(true, Ordering::SeqCst);
	}
}

pub fn router(hub: Arc<Hub>) -> Router {
	let private = Router::new()
		.route("/api/v1/info", get(info))
		.route("/api/v1/snapshot", get(snapshot))
		.route("/api/v1/history", get(history))
		.route("/api/v1/stream", get(stream))
		.route("/api/v1/control", get(control::info))
		.route("/api/v1/control/power", post(control::power))
		.route("/api/v1/control/profile", post(control::profile))
		.route("/api/v1/control/signal", post(control::signal))
		.route("/api/v1/control/action", post(control::action))
		.route("/api/v1/media", post(control::media))
		.route("/api/v1/media/volume", post(control::volume))
		.route("/api/v1/media/art", get(control::art))
		.route("/api/v1/services", get(control::services_list).post(control::service_put))
		.route("/api/v1/services/detected", get(control::detected))
		.route("/api/v1/services/{id}", delete(control::service_delete))
		.route("/api/v1/services/{id}/unit", post(control::service_unit))
		.route("/api/v1/units", get(control::units))
		.route_layer(middleware::from_fn_with_state(hub.clone(), auth));
	Router::new()
		.route("/api/v1/hello", get(hello))
		.route("/api/v1/pair", post(pair))
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
		Some((name, hash)) => {
			req.extensions_mut().insert(ClientName(name));
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

pub(crate) fn paired_body(token: &str, client_id: &str, id: &str, name: &str) -> serde_json::Value {
	json!({ "token": token, "client_id": client_id, "id": id, "name": name })
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
	// One attempt at a time, each wrong one slowed down, a few per code.
	let _guard = hub.pair_lock.lock().await;
	let Some(mut pending) = store::load_pairing() else {
		tokio::time::sleep(timeout::NO_PAIRING_DELAY).await;
		return error(ErrorCode::NoPairing);
	};
	let code: String = req.code.chars().filter(char::is_ascii_digit).collect();
	if !store::same(code.as_bytes(), pending.code.as_bytes()) {
		pending.attempts += 1;
		if pending.attempts >= limit::PAIRING_ATTEMPTS {
			store::clear_pairing();
		} else {
			let _ = store::save_pairing(&pending);
		}
		eprintln!("pairing: wrong code from {}", peer.ip());
		tokio::time::sleep(timeout::WRONG_CODE_DELAY).await;
		return error(ErrorCode::WrongCode);
	}
	// The phone is added before the code is withdrawn: `elamond pair` takes a code gone with
	// no new phone for one used up by wrong attempts.
	match store::add_client(&req.device) {
		Ok((client, token)) => {
			store::clear_pairing();
			eprintln!("pairing: added \"{}\" ({}) from {}", client.name, client.id, peer.ip());
			Json(paired_body(&token, &client.id, &hub.id, &hub.name)).into_response()
		}
		Err(e) => {
			eprintln!("pairing: can't save the client: {e}");
			error(ErrorCode::Storage)
		}
	}
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

/// What one app asked to see in its samples.
pub(crate) struct Want {
	pub(crate) procs: usize,
	pub(crate) sort: Option<String>,
	pub(crate) media: bool,
}

impl Want {
	fn new(p: &StreamParams) -> Self {
		Want { procs: p.procs.unwrap_or(0).min(limit::PROCS_MAX), sort: p.sort.clone(), media: p.media.unwrap_or(0) != 0 }
	}

	/// Whether a frame was read with everything this app asked for.
	fn complete(&self, f: &Frame) -> bool {
		f.watched && (self.procs == 0 || f.processes.is_some()) && (!self.media || f.media.is_some())
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

/// One sample. The request counts as watching until a reading made for it arrives (services,
/// displays, profiles, GPUs, and processes or media when asked for), a few ticks at most.
async fn snapshot(State(hub): State<Arc<Hub>>, Query(p): Query<StreamParams>) -> Response {
	let want = Want::new(&p);
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

async fn stream(State(hub): State<Arc<Hub>>, Extension(token): Extension<TokenHash>, Query(p): Query<StreamParams>) -> Response {
	let want = Want::new(&p);
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
		want: Want,
		token: TokenHash,
		watch: Watch,
	}
	let st = St {
		rx,
		pending,
		info: frame.info.clone(),
		last: None,
		skipped: 0,
		interval,
		watch: Watch::new(&hub, &want),
		want,
		token,
	};

	let body = futures_util::stream::unfold(st, |mut st| async move {
		loop {
			if let Some(line) = st.pending.pop_front() {
				return Some((Ok::<_, Infallible>(Bytes::from(line)), st));
			}
			st.rx.changed().await.ok()?;
			// Revoking a phone ends its stream within a tick, not when it next reconnects.
			st.watch.hub.authorized(&st.token.0)?;
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

/// The shared sample with what this app asked for added: media, and its heaviest processes.
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
	let mut s = frame.sample.clone();
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
		Want { procs, sort: sort.map(str::to_string), media: false }
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
		let v: serde_json::Value = serde_json::from_str(&render_sample(&f, &Want { procs: 1, sort: None, media: true }, false)).unwrap();
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
}
