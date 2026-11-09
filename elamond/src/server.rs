//! The HTTPS API the app talks to. See docs/protocol.md.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime};

use axum::body::{Body, Bytes};
use axum::extract::{ConnectInfo, Query, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::watch;

use crate::collect::HistoryRing;
use crate::model::Process;
use crate::store;

/// What the collector publishes every tick.
pub struct Frame {
	/// The sample, already serialized: every stream sends the same text.
	pub sample: String,
	pub processes: Option<Arc<Vec<Process>>>,
	pub info: Arc<String>,
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
	clients: RwLock<ClientCache>,
	pair_lock: tokio::sync::Mutex<()>,
}

struct ClientCache {
	mtime: Option<SystemTime>,
	hashes: Vec<String>,
}

impl Hub {
	pub fn new(id: String, name: String, frame: watch::Receiver<Arc<Frame>>, history: HistoryRing, interval: Duration) -> Self {
		Hub {
			id,
			name,
			frame,
			history: Mutex::new(history),
			watchers: AtomicUsize::new(0),
			proc_watchers: AtomicUsize::new(0),
			interval,
			clients: RwLock::new(ClientCache { mtime: None, hashes: Vec::new() }),
			pair_lock: tokio::sync::Mutex::new(()),
		}
	}

	/// Checks a token against clients.json, rereading it when `pair` or `revoke` changed it.
	fn authorized(&self, token: &str) -> bool {
		let mtime = store::clients_mtime();
		let stale = self.clients.read().map(|c| c.mtime != mtime || mtime.is_none()).unwrap_or(true);
		if stale && let Ok(mut c) = self.clients.write() {
			c.hashes = store::load_clients().into_iter().map(|c| c.token_hash).collect();
			c.mtime = mtime;
		}
		let hash = store::sha256_hex(token.as_bytes());
		self.clients
			.read()
			.map(|c| c.hashes.iter().any(|h| store::same(h.as_bytes(), hash.as_bytes())))
			.unwrap_or(false)
	}
}

pub fn router(hub: Arc<Hub>) -> Router {
	let private = Router::new()
		.route("/api/v1/info", get(info))
		.route("/api/v1/snapshot", get(snapshot))
		.route("/api/v1/history", get(history))
		.route("/api/v1/stream", get(stream))
		.route_layer(middleware::from_fn_with_state(hub.clone(), auth));
	Router::new()
		.route("/api/v1/hello", get(hello))
		.route("/api/v1/pair", post(pair))
		.merge(private)
		.fallback(|| async { error(StatusCode::NOT_FOUND, "not_found") })
		.with_state(hub)
}

fn error(status: StatusCode, code: &str) -> Response {
	(status, Json(json!({ "error": code }))).into_response()
}

async fn auth(State(hub): State<Arc<Hub>>, req: Request, next: Next) -> Response {
	let token = req
		.headers()
		.get(header::AUTHORIZATION)
		.and_then(|v| v.to_str().ok())
		.and_then(|v| v.strip_prefix("Bearer "))
		.map(str::trim);
	match token {
		Some(t) if hub.authorized(t) => next.run(req).await,
		_ => error(StatusCode::UNAUTHORIZED, "unauthorized"),
	}
}

/// Lets the app check that an address is an Elamon service, and which one, before pairing.
async fn hello(State(hub): State<Arc<Hub>>) -> Response {
	Json(json!({
		"app": "elamon",
		"api": crate::API_VERSION,
		"id": hub.id,
		"name": hub.name,
		"version": env!("CARGO_PKG_VERSION"),
	}))
	.into_response()
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
	// One attempt at a time, each wrong one slowed down, five per code.
	let _guard = hub.pair_lock.lock().await;
	let Some(mut pending) = store::load_pairing() else {
		tokio::time::sleep(Duration::from_millis(500)).await;
		return error(StatusCode::FORBIDDEN, "no_pairing");
	};
	let code: String = req.code.chars().filter(char::is_ascii_digit).collect();
	if !store::same(code.as_bytes(), pending.code.as_bytes()) {
		pending.attempts += 1;
		if pending.attempts >= store::PAIRING_ATTEMPTS {
			store::clear_pairing();
		} else {
			let _ = store::save_pairing(&pending);
		}
		eprintln!("pairing: wrong code from {}", peer.ip());
		tokio::time::sleep(Duration::from_millis(1000)).await;
		return error(StatusCode::FORBIDDEN, "wrong_code");
	}
	store::clear_pairing();
	match store::add_client(&req.device) {
		Ok((client, token)) => {
			eprintln!("pairing: added \"{}\" ({}) from {}", client.name, client.id, peer.ip());
			Json(json!({ "token": token, "client_id": client.id, "id": hub.id, "name": hub.name })).into_response()
		}
		Err(e) => {
			eprintln!("pairing: can't save the client: {e}");
			error(StatusCode::INTERNAL_SERVER_ERROR, "storage")
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
}

/// Counts an app as watching for as long as it is held.
struct Watch {
	hub: Arc<Hub>,
	procs: bool,
}

impl Watch {
	fn new(hub: &Arc<Hub>, procs: bool) -> Self {
		hub.watchers.fetch_add(1, Ordering::SeqCst);
		if procs {
			hub.proc_watchers.fetch_add(1, Ordering::SeqCst);
		}
		Watch { hub: hub.clone(), procs }
	}
}

impl Drop for Watch {
	fn drop(&mut self) {
		self.hub.watchers.fetch_sub(1, Ordering::SeqCst);
		if self.procs {
			self.hub.proc_watchers.fetch_sub(1, Ordering::SeqCst);
		}
	}
}

async fn snapshot(State(hub): State<Arc<Hub>>, Query(p): Query<StreamParams>) -> Response {
	let procs = p.procs.unwrap_or(0).min(10_000);
	let mut rx = hub.frame.clone();
	let _watch = Watch::new(&hub, procs > 0);
	// A frame from before this request may lack processes or GPU data; wait for the next ones.
	let mut frame = rx.borrow_and_update().clone();
	for _ in 0..2 {
		if procs == 0 || frame.processes.is_some() {
			break;
		}
		if tokio::time::timeout(hub.interval * 3, rx.changed()).await.is_err() {
			break;
		}
		frame = rx.borrow_and_update().clone();
	}
	json_response(render_sample(&frame, procs, p.sort.as_deref(), false))
}

async fn stream(State(hub): State<Arc<Hub>>, Query(p): Query<StreamParams>) -> Response {
	let procs = p.procs.unwrap_or(0).min(10_000);
	let interval = Duration::from_millis(p.interval.unwrap_or(0)).clamp(hub.interval, Duration::from_secs(60));
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
		interval: Duration,
		procs: usize,
		sort: Option<String>,
		_watch: Watch,
	}
	let st = St {
		rx,
		pending,
		info: frame.info.clone(),
		last: None,
		interval,
		procs,
		sort: p.sort,
		_watch: Watch::new(&hub, procs > 0),
	};

	let body = futures_util::stream::unfold(st, |mut st| async move {
		loop {
			if let Some(line) = st.pending.pop_front() {
				return Some((Ok::<_, Infallible>(Bytes::from(line)), st));
			}
			st.rx.changed().await.ok()?;
			let frame = st.rx.borrow_and_update().clone();
			if !Arc::ptr_eq(&frame.info, &st.info) && frame.info != st.info {
				st.info = frame.info.clone();
				st.pending.push_back(tagged("info", &frame.info));
			}
			// Ticks jitter a little; a tenth of the interval of slack keeps 2 s from becoming 3 s.
			let due = st.last.is_none_or(|l| l.elapsed() + st.interval / 10 >= st.interval);
			if due {
				st.last = Some(Instant::now());
				st.pending.push_back(render_sample(&frame, st.procs, st.sort.as_deref(), true));
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

fn render_sample(frame: &Frame, procs: usize, sort: Option<&str>, line: bool) -> String {
	let mut s = frame.sample.clone();
	if procs > 0
		&& let Some(list) = &frame.processes
	{
		let mut list: Vec<&Process> = list.iter().collect();
		let key = |p: &Process| -> f64 {
			match sort {
				Some("mem") => p.mem as f64,
				Some("gpu") => p.gpu.unwrap_or(0.0) as f64 * 1e12 + p.gpu_mem.unwrap_or(0) as f64,
				Some("io") => p.read + p.write,
				// CPU first, memory to order the many idle ones.
				_ => p.cpu as f64 * 1e15 + p.mem as f64,
			}
		};
		list.sort_by(|a, b| key(b).total_cmp(&key(a)));
		list.truncate(procs);
		let total = frame.processes.as_ref().map_or(0, |l| l.len());
		let procs_json = serde_json::to_string(&list).unwrap_or_else(|_| "[]".into());
		s.pop();
		s.push_str(&format!(",\"process_count\":{total},\"processes\":{procs_json}}}"));
	}
	if line { tagged("sample", &s) } else { s }
}
