//! `GET /api/v1/stream` and `POST /api/v1/stream/{id}` (docs/protocol.md): what a client reads
//! while a computer is on its screen. An `open` line, then a `data` line for the dashboard on
//! screen every interval, and a `ui` line whenever the client's `ui` changes. Each stream runs on a
//! task of its own; `POST /api/v1/stream/{id}` reaches it through the hub.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::extract::{ConnectInfo, Extension, Path, Query, State};
use axum::http::{header, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;

use crate::access::Caller;
use crate::consts::{cadence, limit};
use crate::errors::ErrorCode;
use crate::server::{self, body_of, data_of, dashboard_in, draws_from, error, needs_of, ClientApp, Hub, TokenHash, Want};
use crate::ui::{Obj, Sources, Ui, J};

/// An open stream, as the hub knows it.
pub struct Open {
	/// The client it is for.
	pub client: String,
	opened: Instant,
	ctl: mpsc::UnboundedSender<Ctl>,
	/// The `ui` it sends for now, to check changes against.
	ui: Arc<Mutex<Arc<Ui>>>,
}

/// What reaches a stream from outside.
enum Ctl {
	Change(Change),
	/// End it, saying why in the journal.
	End(&'static str),
}

/// `POST /api/v1/stream/{id}`: each key optional.
#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct Change {
	pub dashboard: Option<String>,
	pub interval: Option<u64>,
	/// Another of a table's `sorts`, by the table's id; `null` for the table's own order back.
	pub sort: BTreeMap<String, Option<String>>,
}

#[derive(Deserialize, Default)]
pub struct StreamQuery {
	dashboard: Option<String>,
	interval: Option<u64>,
	/// The widget types the client draws, as `ui` takes them; without it, those of its last `ui`.
	widgets: Option<String>,
}

/// The `open` line.
pub fn open_line(stream: &str, ui: &str) -> Obj {
	let mut o = Obj::new();
	o.put("type", "open");
	o.put("stream", stream);
	o.put("ui", ui);
	o
}

/// The `ui` line, when the client's `ui` changed.
pub fn ui_line(ui: &str) -> Obj {
	let mut o = Obj::new();
	o.put("type", "ui");
	o.put("ui", ui);
	o
}

fn line(o: Obj) -> Bytes {
	let mut s = serde_json::to_string(&J::Obj(o)).unwrap_or_default();
	s.push('\n');
	Bytes::from(s)
}

/// The interval a client asked for, within the service's tick and `cadence::STREAM_INTERVAL_MAX`.
fn interval_of(hub: &Hub, asked: Option<u64>, profile_ms: u64) -> Duration {
	Duration::from_millis(asked.unwrap_or(profile_ms)).clamp(hub.interval, cadence::STREAM_INTERVAL_MAX.max(hub.interval))
}

/// Opens a stream: its `open` line at once, the rest from a task of its own.
pub async fn open(
	State(hub): State<Arc<Hub>>,
	Extension(who): Extension<Caller>,
	Extension(app): Extension<ClientApp>,
	Extension(token): Extension<TokenHash>,
	ConnectInfo(peer): ConnectInfo<SocketAddr>,
	Query(q): Query<StreamQuery>,
) -> Response {
	let fixed = q.widgets.as_deref().map(draws_from);
	let draws = fixed.clone().or_else(|| hub.draws_of(&who.id, &app.kind));
	let frame = hub.frame.borrow().clone();
	let ui = hub.ui(&hub.ui_key(&who, &app.kind, draws), &frame);
	// A dashboard the client no longer gets is the first tab: the data lines say which they carry.
	let dashboard = dashboard_in(&ui, q.dashboard.as_deref()).or_else(|| dashboard_in(&ui, None)).map(|d| d.id.clone());
	let interval = interval_of(&hub, q.interval, ui.profile.interval_ms);
	let id = data_encoding::HEXLOWER.encode(&crate::store::random_bytes::<8>());
	let (ctl, ctl_rx) = mpsc::unbounded_channel();
	let shared = Arc::new(Mutex::new(ui.clone()));
	register(&hub, &id, Open { client: who.id.clone(), opened: Instant::now(), ctl, ui: shared.clone() });

	let mut words = vec![format!("dashboard {}", dashboard.as_deref().unwrap_or("none")), format!("every {} ms", interval.as_millis())];
	let need = dashboard.as_deref().and_then(|d| ui.dashboard(d)).map(|d| needs_of(&ui, d)).unwrap_or_default();
	words.extend(need.words().into_iter().map(String::from));
	let logged = hub.stream_opened(&who, peer.ip(), &words.join(", "));
	let run = Run {
		want: hub.want(need),
		hub: hub.clone(),
		id: id.clone(),
		token,
		kind: app.kind,
		fixed,
		ui: ui.clone(),
		shared,
		dashboard,
		need,
		interval,
		sorts: HashMap::new(),
		last: None,
		skipped: 0,
		history: true,
		log: Log { hub: hub.clone(), id, who, opened: Instant::now(), logged, why: "connection closed" },
	};
	let (tx, rx) = mpsc::channel::<Bytes>(4);
	// Sent before the task starts, so it is the first line whatever happens after.
	let _ = tx.try_send(line(open_line(&run.id, &ui.version)));
	tokio::spawn(run.run(tx, ctl_rx));

	let body = futures_util::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|b| (Ok::<_, Infallible>(b), rx)) });
	let mut resp = Body::from_stream(body).into_response();
	let h = resp.headers_mut();
	h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/x-ndjson"));
	h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
	resp
}

/// Adds a stream to those open; a client's oldest beyond `limit::STREAMS_PER_CLIENT` ends.
fn register(hub: &Hub, id: &str, open: Open) {
	let mut streams = hub.streams.lock().unwrap_or_else(|p| p.into_inner());
	let client = open.client.clone();
	streams.insert(id.to_string(), open);
	let mut mine: Vec<(Instant, String)> = streams.iter().filter(|(_, o)| o.client == client).map(|(k, o)| (o.opened, k.clone())).collect();
	mine.sort();
	let over = mine.len().saturating_sub(limit::STREAMS_PER_CLIENT);
	for (_, old) in mine.into_iter().take(over) {
		if let Some(o) = streams.remove(&old) {
			let _ = o.ctl.send(Ctl::End("the phone opened more streams than it may hold"));
		}
	}
}

/// A stream as it runs.
struct Run {
	hub: Arc<Hub>,
	id: String,
	token: TokenHash,
	kind: String,
	/// The widget types the stream was opened with, if it was.
	fixed: Option<BTreeSet<String>>,
	ui: Arc<Ui>,
	shared: Arc<Mutex<Arc<Ui>>>,
	dashboard: Option<String>,
	need: Sources,
	want: Want,
	interval: Duration,
	sorts: HashMap<String, String>,
	/// When the last `data` line went.
	last: Option<Instant>,
	/// Readings passed over while waiting for one that has what the dashboard needs.
	skipped: u32,
	/// The next `data` line carries `history`.
	history: bool,
	log: Log,
}

impl Run {
	async fn run(mut self, tx: mpsc::Sender<Bytes>, mut ctl: mpsc::UnboundedReceiver<Ctl>) {
		let mut frames = self.hub.frame.clone();
		frames.borrow_and_update();
		loop {
			tokio::select! {
				_ = tx.closed() => break,
				c = ctl.recv() => match c {
					// A new dashboard or order goes out at once, from the reading there is, where it has
					// what they need; otherwise with the next one, as it would anyway.
					Some(Ctl::Change(c)) => {
						self.apply(c);
						if self.last.is_some() {
							continue;
						}
					}
					Some(Ctl::End(why)) => {
						self.log.why = why;
						break;
					}
					None => break,
				},
				r = frames.changed() => if r.is_err() {
					self.log.why = "the service is stopping";
					break;
				},
			}
			let frame = frames.borrow_and_update().clone();
			// Revoking a phone ends its streams within a tick, not when it next reconnects.
			let Some(who) = self.hub.authorized(&self.token.0) else {
				self.log.why = "the phone was unpaired";
				break;
			};
			self.log.who = who.clone();
			let draws = self.fixed.clone().or_else(|| self.hub.draws_of(&who.id, &self.kind));
			let ui = self.hub.ui(&self.hub.ui_key(&who, &self.kind, draws), &frame);
			if ui.version != self.ui.version {
				if tx.send(line(ui_line(&ui.version))).await.is_err() {
					break;
				}
				self.set_ui(ui);
			}
			let Some(dashboard) = self.dashboard.as_deref().and_then(|d| self.ui.dashboard(d)) else { continue };
			// The first line of a dashboard waits for a reading made with what it needs, a few ticks.
			if self.last.is_none() && !frame.read.covers(self.need) && self.skipped < cadence::FIRST_FRAME_TICKS {
				self.skipped += 1;
				continue;
			}
			// Ticks jitter a little; a tenth of the interval of slack keeps 2 s from becoming 3 s.
			let due = self.last.is_none_or(|l| l.elapsed() + self.interval / 10 >= self.interval);
			if !due {
				continue;
			}
			let data = data_of(&self.hub, &self.ui, dashboard, &who, &frame, &self.sorts, self.history);
			if tx.send(line(data)).await.is_err() {
				break;
			}
			self.last = Some(Instant::now());
			self.history = false;
			self.skipped = 0;
		}
		self.hub.streams.lock().unwrap_or_else(|p| p.into_inner()).remove(&self.id);
	}

	/// Takes a new `ui`: the dashboard on screen stays, or becomes the first tab when it is gone.
	fn set_ui(&mut self, ui: Arc<Ui>) {
		*self.shared.lock().unwrap_or_else(|p| p.into_inner()) = ui.clone();
		self.ui = ui;
		let kept = self.dashboard.as_deref().is_some_and(|d| self.ui.dashboard(d).is_some());
		if !kept {
			self.dashboard = dashboard_in(&self.ui, None).map(|d| d.id.clone());
			self.anew();
		}
		self.update_need();
	}

	/// The next reading sends a line, with its history.
	fn anew(&mut self) {
		self.last = None;
		self.skipped = 0;
		self.history = true;
	}

	fn update_need(&mut self) {
		let need = self.dashboard.as_deref().and_then(|d| self.ui.dashboard(d)).map(|d| needs_of(&self.ui, d)).unwrap_or_default();
		if need != self.need {
			self.need = need;
			self.want.set(need);
		}
	}

	/// What `POST /api/v1/stream/{id}` asked for, checked there, from the next line on.
	fn apply(&mut self, c: Change) {
		if let Some(d) = c.dashboard.filter(|d| self.ui.dashboard(d).is_some()) {
			// Said in the journal where its opening was, so a stream's dashboards can be followed there.
			if self.log.logged && self.dashboard.as_deref() != Some(d.as_str()) {
				eprintln!("stream: \"{}\" now on dashboard {d}", self.log.who.name);
			}
			self.dashboard = Some(d);
			self.anew();
		}
		if let Some(ms) = c.interval {
			self.interval = interval_of(&self.hub, Some(ms), ms);
		}
		if !c.sort.is_empty() {
			for (table, field) in c.sort {
				match field {
					Some(f) => self.sorts.insert(table, f),
					None => self.sorts.remove(&table),
				};
			}
			// The new order at once, as the dashboard would come.
			self.last = None;
		}
		self.update_need();
	}
}

/// Says in the journal when a stream ends, how long it lasted and why, if its start was.
struct Log {
	hub: Arc<Hub>,
	id: String,
	who: Caller,
	opened: Instant,
	logged: bool,
	why: &'static str,
}

impl Drop for Log {
	fn drop(&mut self) {
		self.hub.streams.lock().unwrap_or_else(|p| p.into_inner()).remove(&self.id);
		self.hub.stream_closed(&self.who, self.logged, self.opened.elapsed(), self.why);
	}
}

/// Changes what an open stream of this client sends, from its next line.
pub async fn change(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Path(id): Path<String>, body: Bytes) -> Response {
	let c: Change = match body_of(&body) {
		Ok(c) => c,
		Err(r) => return *r,
	};
	let found = {
		let streams = hub.streams.lock().unwrap_or_else(|p| p.into_inner());
		streams.get(&id).filter(|o| o.client == who.id).map(|o| (o.ctl.clone(), o.ui.lock().unwrap_or_else(|p| p.into_inner()).clone()))
	};
	let Some((ctl, ui)) = found else { return error(ErrorCode::NoSuchStream) };
	if c.dashboard.as_deref().is_some_and(|d| ui.dashboard(d).is_none()) {
		return error(ErrorCode::NoSuchDashboard);
	}
	for (table, field) in &c.sort {
		if !sorts_by(&ui, table, field.as_deref()) {
			return server::error_with(ErrorCode::BadSort, table.clone());
		}
	}
	if ctl.send(Ctl::Change(c)).is_err() {
		return error(ErrorCode::NoSuchStream);
	}
	Json(json!({ "ok": true })).into_response()
}

/// Whether the table `table` of `ui` may be sorted by `field`: one of its `sorts`, or its own
/// sort; `None`, its own order back, any table may.
pub(crate) fn sorts_by(ui: &Ui, table: &str, field: Option<&str>) -> bool {
	ui.dashboards.iter().flat_map(|d| d.all_widgets()).filter(|w| w.id == table && w.kind == "table").any(|w| match field {
		None => true,
		Some(f) => w.def.sorts.iter().flatten().any(|s| s == f) || w.def.sort.as_ref().is_some_and(|s| s.field == f),
	})
}
