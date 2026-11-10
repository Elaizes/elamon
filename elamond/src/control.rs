//! Acting on the computer from the app: power, profiles, processes, actions, media, services.
//!
//! Everything that changes the computer needs `control = true` in elamond.toml; the list of
//! web services is the exception, it only changes what the app shows. Every action is
//! logged with the name of the phone that asked.

use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::Json;
use axum::extract::{Extension, Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::io::AsyncReadExt;

use crate::bus::{Player, PowerCaps, Profiles, Scope, UnitInfo};
use crate::model::Sample;
use crate::server::{ClientName, Hub, error};
use crate::services::{self, ServiceDef, ServiceStatus};
use crate::store;

/// Output kept from an action, so a chatty command can't flood the phone.
const OUTPUT_MAX: usize = 16 * 1024;

fn denied(hub: &Hub) -> Option<Response> {
	(!hub.config().control).then(|| error(StatusCode::FORBIDDEN, "control_disabled"))
}

fn done(result: Result<(), String>) -> Response {
	match result {
		Ok(()) => Json(json!({ "ok": true })).into_response(),
		Err(e) if e == "not_permitted" => error(StatusCode::FORBIDDEN, "not_permitted"),
		Err(e) => (StatusCode::BAD_GATEWAY, Json(json!({ "error": "failed", "message": e }))).into_response(),
	}
}

#[derive(Serialize)]
pub(crate) struct ControlInfo {
	pub(crate) enabled: bool,
	pub(crate) power: PowerCaps,
	pub(crate) profiles: Option<Profiles>,
	pub(crate) actions: Vec<ActionInfo>,
	pub(crate) volume: bool,
}

#[derive(Serialize)]
pub(crate) struct ActionInfo {
	pub(crate) id: String,
	pub(crate) name: String,
	pub(crate) confirm: bool,
}

/// What the app may offer on this computer right now.
pub async fn info(State(hub): State<Arc<Hub>>) -> Response {
	let enabled = hub.config().control;
	let (power, profiles) = if enabled { (hub.bus.power_caps().await, hub.bus.profiles().await) } else { Default::default() };
	let actions = if enabled {
		hub.config().actions.iter().map(|a| ActionInfo { id: a.id(), name: a.name.clone(), confirm: a.confirm }).collect()
	} else {
		Vec::new()
	};
	let volume = enabled && tokio::task::spawn_blocking(volume_get).await.ok().flatten().is_some();
	Json(ControlInfo { enabled, power, profiles, actions, volume }).into_response()
}

#[derive(Deserialize)]
pub struct PowerReq {
	action: String,
}

pub async fn power(State(hub): State<Arc<Hub>>, Extension(who): Extension<ClientName>, Json(r): Json<PowerReq>) -> Response {
	if let Some(e) = denied(&hub) {
		return e;
	}
	eprintln!("control: {} asked to {}", who.0, r.action);
	done(hub.bus.power(&r.action).await)
}

#[derive(Deserialize)]
pub struct ProfileReq {
	profile: String,
}

pub async fn profile(State(hub): State<Arc<Hub>>, Extension(who): Extension<ClientName>, Json(r): Json<ProfileReq>) -> Response {
	if let Some(e) = denied(&hub) {
		return e;
	}
	eprintln!("control: {} set the power profile to {}", who.0, r.profile);
	let res = hub.bus.set_profile(&r.profile).await;
	hub.refresh_soon();
	done(res)
}

#[derive(Deserialize)]
pub struct SignalReq {
	pid: i32,
	/// "term" asks the process to quit, "kill" ends it outright.
	signal: String,
}

pub async fn signal(State(hub): State<Arc<Hub>>, Extension(who): Extension<ClientName>, Json(r): Json<SignalReq>) -> Response {
	if let Some(e) = denied(&hub) {
		return e;
	}
	let sig = match r.signal.as_str() {
		"term" => libc::SIGTERM,
		"kill" => libc::SIGKILL,
		_ => return error(StatusCode::BAD_REQUEST, "unknown_signal"),
	};
	if r.pid <= 1 || r.pid as u32 == std::process::id() {
		return error(StatusCode::FORBIDDEN, "not_permitted");
	}
	eprintln!("control: {} sent {} to pid {}", who.0, r.signal, r.pid);
	// SAFETY: kill(2) with a plain pid and signal number has no memory effects.
	let rc = unsafe { libc::kill(r.pid, sig) };
	if rc == 0 {
		return Json(json!({ "ok": true })).into_response();
	}
	match std::io::Error::last_os_error().raw_os_error() {
		Some(libc::EPERM) => error(StatusCode::FORBIDDEN, "not_permitted"),
		Some(libc::ESRCH) => error(StatusCode::NOT_FOUND, "no_such_process"),
		_ => error(StatusCode::BAD_GATEWAY, "failed"),
	}
}

#[derive(Deserialize)]
pub struct ActionReq {
	id: String,
}

#[derive(Serialize)]
pub(crate) struct ActionResult {
	pub(crate) exit_code: Option<i32>,
	pub(crate) timed_out: bool,
	pub(crate) output: String,
}

pub async fn action(State(hub): State<Arc<Hub>>, Extension(who): Extension<ClientName>, Json(r): Json<ActionReq>) -> Response {
	if let Some(e) = denied(&hub) {
		return e;
	}
	let config = hub.config();
	let Some(a) = config.actions.iter().find(|a| a.id() == r.id) else {
		return error(StatusCode::NOT_FOUND, "no_such_action");
	};
	let Some((program, args)) = a.command.split_first() else {
		return error(StatusCode::BAD_REQUEST, "empty_command");
	};
	eprintln!("control: {} ran \"{}\"", who.0, a.name);
	let child = tokio::process::Command::new(program)
		.args(args)
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.kill_on_drop(true)
		.spawn();
	let mut child = match child {
		Ok(c) => c,
		Err(e) => {
			return (StatusCode::BAD_GATEWAY, Json(json!({ "error": "failed", "message": e.to_string() }))).into_response();
		}
	};
	let mut stdout = child.stdout.take().expect("piped");
	let mut stderr = child.stderr.take().expect("piped");
	let read_all = async {
		let mut out = Vec::new();
		let mut err = Vec::new();
		let _ = tokio::join!(stdout.read_to_end(&mut out), stderr.read_to_end(&mut err));
		let status = child.wait().await.ok();
		(status, out, err)
	};
	let (exit_code, timed_out, output) = match tokio::time::timeout(Duration::from_secs(a.timeout.clamp(1, 3600)), read_all).await {
		Ok((status, out, err)) => {
			let mut text = String::from_utf8_lossy(&out).into_owned();
			if !err.is_empty() {
				if !text.is_empty() && !text.ends_with('\n') {
					text.push('\n');
				}
				text.push_str(&String::from_utf8_lossy(&err));
			}
			(status.and_then(|s| s.code()), false, text)
		}
		// Dropping the future drops the child, which kill_on_drop ends.
		Err(_) => (None, true, String::new()),
	};
	let mut output = output;
	if output.len() > OUTPUT_MAX {
		let mut cut = output.len() - OUTPUT_MAX;
		while !output.is_char_boundary(cut) {
			cut += 1;
		}
		output = format!("…{}", &output[cut..]);
	}
	hub.refresh_soon();
	Json(ActionResult { exit_code, timed_out, output }).into_response()
}

// ---- media ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Debug, Default)]
pub struct Media {
	pub players: Vec<Player>,
	pub volume: Option<Volume>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Volume {
	/// 1.0 is 100 %.
	pub level: f32,
	pub muted: bool,
}

#[derive(Deserialize)]
pub struct MediaReq {
	player: String,
	op: String,
	value: Option<f64>,
}

pub async fn media(State(hub): State<Arc<Hub>>, Json(r): Json<MediaReq>) -> Response {
	if let Some(e) = denied(&hub) {
		return e;
	}
	let res = hub.bus.player_op(&r.player, &r.op, r.value).await;
	hub.refresh_soon();
	done(res)
}

#[derive(Deserialize)]
pub struct VolumeReq {
	level: Option<f32>,
	muted: Option<bool>,
}

pub async fn volume(State(hub): State<Arc<Hub>>, Json(r): Json<VolumeReq>) -> Response {
	if let Some(e) = denied(&hub) {
		return e;
	}
	let res = tokio::task::spawn_blocking(move || volume_set(r.level, r.muted)).await.unwrap_or_else(|e| Err(e.to_string()));
	hub.refresh_soon();
	done(res)
}

#[derive(Deserialize)]
pub struct ArtQuery {
	player: String,
}

pub async fn art(State(hub): State<Arc<Hub>>, Query(q): Query<ArtQuery>) -> Response {
	match hub.bus.player_art(&q.player).await {
		Some((bytes, kind)) => ([(header::CONTENT_TYPE, kind), (header::CACHE_CONTROL, "no-store")], bytes).into_response(),
		None => error(StatusCode::NOT_FOUND, "no_art"),
	}
}

/// The default output's volume through PipeWire's wpctl, or PulseAudio's pactl.
pub fn volume_get() -> Option<Volume> {
	if let Some(out) = run(&["wpctl", "get-volume", "@DEFAULT_AUDIO_SINK@"]) {
		// "Volume: 0.25" or "Volume: 0.25 [MUTED]"
		let level = out.split_whitespace().nth(1)?.parse().ok()?;
		return Some(Volume { level, muted: out.contains("MUTED") });
	}
	let vol = run(&["pactl", "get-sink-volume", "@DEFAULT_SINK@"])?;
	let pct: f32 = vol.split('/').nth(1)?.trim().trim_end_matches('%').trim().parse().ok()?;
	let muted = run(&["pactl", "get-sink-mute", "@DEFAULT_SINK@"]).is_some_and(|m| m.contains("yes"));
	Some(Volume { level: pct / 100.0, muted })
}

fn volume_set(level: Option<f32>, muted: Option<bool>) -> Result<(), String> {
	let wp = run(&["wpctl", "get-volume", "@DEFAULT_AUDIO_SINK@"]).is_some();
	if let Some(l) = level {
		let l = l.clamp(0.0, 1.0);
		let ok = if wp {
			run(&["wpctl", "set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{l:.2}")])
		} else {
			run(&["pactl", "set-sink-volume", "@DEFAULT_SINK@", &format!("{}%", (l * 100.0).round())])
		};
		ok.ok_or("volume_failed")?;
	}
	if let Some(m) = muted {
		let v = if m { "1" } else { "0" };
		let ok = if wp {
			run(&["wpctl", "set-mute", "@DEFAULT_AUDIO_SINK@", v])
		} else {
			run(&["pactl", "set-sink-mute", "@DEFAULT_SINK@", v])
		};
		ok.ok_or("mute_failed")?;
	}
	Ok(())
}

fn run(argv: &[&str]) -> Option<String> {
	let out = std::process::Command::new(argv[0]).args(&argv[1..]).stdin(Stdio::null()).output().ok()?;
	out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

// ---- services --------------------------------------------------------------------------

pub async fn services_list(State(hub): State<Arc<Hub>>) -> Response {
	Json(hub.service_statuses().await).into_response()
}

#[derive(Deserialize)]
pub struct ServiceReq {
	id: Option<String>,
	name: String,
	url: String,
	unit: Option<String>,
	scope: Option<String>,
}

pub async fn service_put(State(hub): State<Arc<Hub>>, Extension(who): Extension<ClientName>, Json(r): Json<ServiceReq>) -> Response {
	let unit = r.unit.as_deref().map(str::trim).filter(|u| !u.is_empty());
	if let Err(code) = services::validate(&r.name, &r.url, unit, r.scope.as_deref()) {
		return error(StatusCode::BAD_REQUEST, code);
	}
	let mut list = hub.app_services.lock().await;
	let def = ServiceDef {
		id: r.id.clone().unwrap_or_else(|| data_encoding::HEXLOWER.encode(&store::random_bytes::<6>())),
		name: r.name.trim().to_string(),
		url: r.url.trim().to_string(),
		unit: unit.map(str::to_string),
		scope: r.scope.clone(),
	};
	match list.iter_mut().find(|s| s.id == def.id) {
		Some(s) => *s = def.clone(),
		None if r.id.is_none() => list.push(def.clone()),
		None => return error(StatusCode::NOT_FOUND, "no_such_service"),
	}
	if let Err(e) = services::save(&list) {
		eprintln!("services: can't save: {e}");
		return error(StatusCode::INTERNAL_SERVER_ERROR, "storage");
	}
	eprintln!("services: {} saved \"{}\"", who.0, def.name);
	drop(list);
	hub.refresh_soon();
	Json(def).into_response()
}

pub async fn service_delete(State(hub): State<Arc<Hub>>, Extension(who): Extension<ClientName>, Path(id): Path<String>) -> Response {
	let mut list = hub.app_services.lock().await;
	let before = list.len();
	list.retain(|s| s.id != id);
	if list.len() == before {
		return error(StatusCode::NOT_FOUND, "no_such_service");
	}
	if services::save(&list).is_err() {
		return error(StatusCode::INTERNAL_SERVER_ERROR, "storage");
	}
	eprintln!("services: {} removed {id}", who.0);
	drop(list);
	hub.refresh_soon();
	Json(json!({ "ok": true })).into_response()
}

#[derive(Deserialize)]
pub struct UnitReq {
	op: String,
}

pub async fn service_unit(
	State(hub): State<Arc<Hub>>,
	Extension(who): Extension<ClientName>,
	Path(id): Path<String>,
	Json(r): Json<UnitReq>,
) -> Response {
	if let Some(e) = denied(&hub) {
		return e;
	}
	let app = hub.app_services.lock().await.clone();
	let Some((def, _)) = services::all(&hub.config(), &app).into_iter().find(|(s, _)| s.id == id) else {
		return error(StatusCode::NOT_FOUND, "no_such_service");
	};
	let Some(unit) = def.unit else { return error(StatusCode::BAD_REQUEST, "no_unit") };
	eprintln!("control: {} asked to {} {unit}", who.0, r.op);
	let res = hub.bus.unit_op(Scope::parse(def.scope.as_deref()), &unit, &r.op).await;
	hub.refresh_soon();
	done(res)
}

pub async fn detected(State(_hub): State<Arc<Hub>>) -> Response {
	let list = tokio::task::spawn_blocking(services::detect).await.unwrap_or_default();
	Json(list).into_response()
}

#[derive(Deserialize)]
pub struct UnitsQuery {
	scope: Option<String>,
}

pub async fn units(State(hub): State<Arc<Hub>>, Query(q): Query<UnitsQuery>) -> Response {
	match hub.bus.units(Scope::parse(q.scope.as_deref())).await {
		Ok(list) => Json::<Vec<UnitInfo>>(list).into_response(),
		Err(e) => (StatusCode::BAD_GATEWAY, Json(json!({ "error": "failed", "message": e }))).into_response(),
	}
}

// ---- what the stream carries -------------------------------------------------------------

/// The slower, D-Bus-backed parts of a sample, refreshed by the collector thread.
pub struct Extras {
	services: Vec<ServiceStatus>,
	profiles: Option<Profiles>,
	media: Option<Media>,
	tick: u64,
}

impl Extras {
	pub fn new() -> Self {
		Extras { services: Vec::new(), profiles: None, media: None, tick: 0 }
	}

	/// Fills in services, power profiles and media; called once per tick off the async runtime.
	pub fn apply(&mut self, hub: &Hub, rt: &tokio::runtime::Handle, watched: bool, want_media: bool, sample: &mut Sample) {
		let urgent = hub.refresh.swap(false, Ordering::SeqCst);
		if watched && (self.tick.is_multiple_of(5) || urgent) {
			self.services = rt.block_on(hub.service_statuses());
			self.profiles = rt.block_on(hub.bus.profiles());
		}
		if want_media {
			let players = rt.block_on(hub.bus.players());
			// The volume costs a process; every other second is plenty.
			let volume = if self.tick.is_multiple_of(2) || urgent || self.media.is_none() {
				if hub.config().control { volume_get() } else { None }
			} else {
				self.media.as_ref().and_then(|m| m.volume.clone())
			};
			self.media = Some(Media { players, volume });
		} else {
			self.media = None;
		}
		self.tick += 1;
		if watched {
			sample.services = self.services.clone();
			if let Some(p) = &self.profiles {
				sample.power.ppd_profile = Some(p.active.clone());
				sample.power.ppd_profiles = p.available.clone();
			}
		}
		sample.media = self.media.clone();
	}
}

impl Hub {
	/// Service statuses with their units' states.
	pub async fn service_statuses(&self) -> Vec<ServiceStatus> {
		let app = self.app_services.lock().await.clone();
		let config = self.config();
		let mut list = tokio::task::spawn_blocking(move || services::statuses(&config, &app)).await.unwrap_or_default();
		for s in &mut list {
			if let Some(u) = &s.unit {
				s.unit_state = self.bus.unit_state(Scope::parse(Some(&s.scope)), u).await;
			}
		}
		list
	}
}
