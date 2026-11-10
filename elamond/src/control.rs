//! Acting on the computer from the app: power, profiles, processes, actions, media, services.
//!
//! Everything that changes the computer needs `control = true` in elamond.toml; the list of
//! web services is the exception, it only changes what the app shows. Every action is
//! logged with the name of the phone that asked, once the request has been checked.

use std::collections::VecDeque;
use std::io::Read;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Extension, Path, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::bus::{self, Bus, Player, PowerCaps, Profiles, Scope, UnitInfo};
use crate::consts::cadence::{self, Ticks};
use crate::consts::{limit, timeout};
use crate::errors::{reason, ErrorCode};
use crate::model::Sample;
use crate::server::{error, failed, ClientName, Hub};
use crate::services::{self, ServiceDef, ServiceStatus};
use crate::store;

fn denied(hub: &Hub) -> Option<Response> {
	(!hub.config().control).then(|| error(ErrorCode::ControlDisabled))
}

fn done(result: Result<(), String>) -> Response {
	match result {
		Ok(()) => Json(json!({ "ok": true })).into_response(),
		Err(e) if e == ErrorCode::NotPermitted.as_str() => error(ErrorCode::NotPermitted),
		Err(e) => failed(e),
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
	if Bus::power_method(&r.action).is_none() {
		return failed(reason::UNKNOWN_ACTION);
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
	let Some(profiles) = hub.bus.profiles().await else {
		return failed(reason::NO_POWER_PROFILES);
	};
	if !profiles.available.contains(&r.profile) {
		return failed(reason::UNKNOWN_PROFILE);
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
		_ => return error(ErrorCode::UnknownSignal),
	};
	if protected(r.pid) {
		return error(ErrorCode::NotPermitted);
	}
	eprintln!("control: {} sent {} to pid {}", who.0, r.signal, r.pid);
	// SAFETY: kill(2) with a plain pid and signal number has no memory effects.
	let rc = unsafe { libc::kill(r.pid, sig) };
	if rc == 0 {
		return Json(json!({ "ok": true })).into_response();
	}
	match std::io::Error::last_os_error().raw_os_error() {
		Some(libc::EPERM) => error(ErrorCode::NotPermitted),
		Some(libc::ESRCH) => error(ErrorCode::NoSuchProcess),
		_ => error(ErrorCode::Failed),
	}
}

/// Processes a phone may not signal: init, the service itself (any of its threads) and the
/// processes it runs under, like `systemd --user`, which would take the service down too.
fn protected(pid: i32) -> bool {
	if pid <= 1 {
		return true;
	}
	// A thread's id signals its whole process.
	let target = proc_field(pid, "status").and_then(|s| tgid(&s)).unwrap_or(pid);
	let mut p = std::process::id() as i32;
	loop {
		if p == target {
			return true;
		}
		match proc_field(p, "stat").and_then(|s| ppid(&s)) {
			Some(parent) if parent > 1 && parent != p => p = parent,
			_ => return target == 1,
		}
	}
}

fn proc_field(pid: i32, file: &str) -> Option<String> {
	std::fs::read_to_string(format!("/proc/{pid}/{file}")).ok()
}

/// The parent from /proc/<pid>/stat; the name before it is in parentheses and may hold anything.
fn ppid(stat: &str) -> Option<i32> {
	stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()
}

fn tgid(status: &str) -> Option<i32> {
	status.lines().find_map(|l| l.strip_prefix("Tgid:"))?.trim().parse().ok()
}

#[derive(Deserialize)]
pub struct ActionReq {
	id: String,
}

#[derive(Serialize, Debug)]
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
		return error(ErrorCode::NoSuchAction);
	};
	let Some((program, args)) = a.command.split_first() else {
		return error(ErrorCode::EmptyCommand);
	};
	eprintln!("control: {} ran \"{}\"", who.0, a.name);
	let limit = Duration::from_secs(a.timeout).clamp(timeout::ACTION_MIN, timeout::ACTION_MAX);
	let result = run_action(program, args, limit).await;
	hub.refresh_soon();
	match result {
		Ok(r) => Json(r).into_response(),
		Err(e) => failed(e.to_string()),
	}
}

/// Runs an action's command in a process group of its own: on a timeout, or when the request
/// goes away mid-run, everything it started is killed with it rather than left orphaned. The
/// output is kept within `limit::OUTPUT_MAX` as it comes, stdout and stderr in the order they
/// arrive, and what came before a timeout is returned too.
async fn run_action(program: &str, args: &[String], limit: Duration) -> std::io::Result<ActionResult> {
	let mut child = tokio::process::Command::new(program)
		.args(args)
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.process_group(0)
		.kill_on_drop(true)
		.spawn()?;
	let mut group = Group(child.id().map(|id| id as i32));
	let mut out = child.stdout.take();
	let mut err = child.stderr.take();
	let (mut out_buf, mut err_buf) = ([0u8; 4096], [0u8; 4096]);
	let mut capture = Capture::default();
	let mut until = tokio::time::Instant::now() + limit;
	let mut status = None;
	while status.is_none() || out.is_some() || err.is_some() {
		tokio::select! {
			_ = tokio::time::sleep_until(until) => break,
			n = read_some(&mut out, &mut out_buf) => capture.push(&out_buf[..n]),
			n = read_some(&mut err, &mut err_buf) => capture.push(&err_buf[..n]),
			s = child.wait(), if status.is_none() => {
				status = Some(s?);
				// What it left running may hold the output open; that isn't worth waiting for.
				until = until.min(tokio::time::Instant::now() + timeout::ACTION_DRAIN);
			}
		}
	}
	let timed_out = status.is_none();
	if !timed_out {
		// It finished; what it started on purpose and let go of the output may stay.
		group.0 = None;
	}
	drop(group);
	Ok(ActionResult { exit_code: status.and_then(|s| s.code()), timed_out, output: capture.text() })
}

/// Kills a process group when dropped, unless its number has been taken out.
struct Group(Option<i32>);

impl Drop for Group {
	fn drop(&mut self) {
		if let Some(pgid) = self.0.take() {
			// SAFETY: killpg(2) with a plain group id and signal number has no memory effects.
			unsafe { libc::killpg(pgid, libc::SIGKILL) };
		}
	}
}

/// The next bytes from a pipe; at its end the pipe is let go of, and then this never finishes.
async fn read_some<R: AsyncRead + Unpin>(pipe: &mut Option<R>, buf: &mut [u8]) -> usize {
	let Some(r) = pipe else { return std::future::pending().await };
	match r.read(buf).await {
		Ok(n) if n > 0 => n,
		_ => {
			*pipe = None;
			0
		}
	}
}

/// The start and the end of a command's output, however much it writes.
#[derive(Default)]
struct Capture {
	head: Vec<u8>,
	tail: VecDeque<u8>,
	dropped: usize,
}

impl Capture {
	fn push(&mut self, bytes: &[u8]) {
		let room = limit::OUTPUT_HEAD.saturating_sub(self.head.len()).min(bytes.len());
		self.head.extend_from_slice(&bytes[..room]);
		self.tail.extend(&bytes[room..]);
		let over = self.tail.len().saturating_sub(limit::OUTPUT_MAX - limit::OUTPUT_HEAD);
		self.tail.drain(..over);
		self.dropped += over;
	}

	fn text(self) -> String {
		let mut tail: Vec<u8> = self.tail.into();
		if self.dropped == 0 {
			let mut all = self.head;
			all.append(&mut tail);
			return String::from_utf8_lossy(&all).into_owned();
		}
		// Cut where a character starts, not in the middle of one.
		let head = match std::str::from_utf8(&self.head) {
			Err(e) if e.error_len().is_none() => &self.head[..e.valid_up_to()],
			_ => &self.head[..],
		};
		let start = tail.iter().position(|b| b & 0xc0 != 0x80).unwrap_or(tail.len());
		format!("{}\n…\n{}", String::from_utf8_lossy(head), String::from_utf8_lossy(&tail[start..]))
	}
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

pub async fn media(State(hub): State<Arc<Hub>>, Extension(who): Extension<ClientName>, Json(r): Json<MediaReq>) -> Response {
	if let Some(e) = denied(&hub) {
		return e;
	}
	if !bus::is_player(&r.player) {
		return failed(reason::UNKNOWN_PLAYER);
	}
	if !Bus::is_player_op(&r.op) {
		return failed(reason::UNKNOWN_OPERATION);
	}
	match r.value {
		Some(v) => eprintln!("control: {} asked {} to {} {v}", who.0, r.player, r.op),
		None => eprintln!("control: {} asked {} to {}", who.0, r.player, r.op),
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

pub async fn volume(State(hub): State<Arc<Hub>>, Extension(who): Extension<ClientName>, Json(r): Json<VolumeReq>) -> Response {
	if let Some(e) = denied(&hub) {
		return e;
	}
	if let Some(l) = r.level {
		eprintln!("control: {} set the volume to {:.0} %", who.0, l.clamp(0.0, 1.0) * 100.0);
	}
	if let Some(m) = r.muted {
		eprintln!("control: {} {} the sound", who.0, if m { "muted" } else { "unmuted" });
	}
	let res = tokio::task::spawn_blocking(move || volume_set(r.level, r.muted)).await.unwrap_or_else(|e| Err(e.to_string()));
	hub.refresh_soon();
	done(res)
}

#[derive(Deserialize)]
pub struct ArtQuery {
	player: String,
}

/// A player's cover. Media needs control, so its art does too.
pub async fn art(State(hub): State<Arc<Hub>>, Query(q): Query<ArtQuery>) -> Response {
	if let Some(e) = denied(&hub) {
		return e;
	}
	match hub.bus.player_art(&q.player).await {
		Some((bytes, kind)) => ([(header::CONTENT_TYPE, kind), (header::CACHE_CONTROL, "no-store")], bytes).into_response(),
		None => error(ErrorCode::NoArt),
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
	// "Mute: yes", in English thanks to LC_ALL=C.
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
		ok.ok_or(reason::VOLUME_FAILED)?;
	}
	if let Some(m) = muted {
		let v = if m { "1" } else { "0" };
		let ok = if wp {
			run(&["wpctl", "set-mute", "@DEFAULT_AUDIO_SINK@", v])
		} else {
			run(&["pactl", "set-sink-mute", "@DEFAULT_SINK@", v])
		};
		ok.ok_or(reason::MUTE_FAILED)?;
	}
	Ok(())
}

/// Runs a short command and returns its output, or nothing if it failed or took longer than
/// `timeout::COMMAND`: the collector calls this, and must not hang on a stuck sound server.
fn run(argv: &[&str]) -> Option<String> {
	use std::os::unix::process::CommandExt;
	let mut child = std::process::Command::new(argv[0])
		.args(&argv[1..])
		// pactl translates its answers.
		.env("LC_ALL", "C")
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::null())
		.process_group(0)
		.spawn()
		.ok()?;
	let deadline = Instant::now() + timeout::COMMAND;
	let status = loop {
		match child.try_wait() {
			Ok(Some(s)) => break s,
			Ok(None) if Instant::now() < deadline => std::thread::sleep(cadence::COMMAND_POLL),
			_ => {
				drop(Group(Some(child.id() as i32)));
				let _ = child.wait();
				return None;
			}
		}
	};
	let mut out = String::new();
	child.stdout.take()?.read_to_string(&mut out).ok()?;
	status.success().then_some(out)
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
		return error(code);
	}
	let mut list = hub.app_services.lock().await;
	let def = ServiceDef {
		id: r.id.clone().unwrap_or_else(|| data_encoding::HEXLOWER.encode(&store::random_bytes::<{ crate::consts::protocol::SERVICE_ID_BYTES }>())),
		name: r.name.trim().to_string(),
		url: r.url.trim().to_string(),
		unit: unit.map(str::to_string),
		scope: r.scope.clone(),
	};
	match list.iter_mut().find(|s| s.id == def.id) {
		Some(s) => *s = def.clone(),
		None if r.id.is_none() => list.push(def.clone()),
		None => return error(ErrorCode::NoSuchService),
	}
	if let Err(e) = services::save(&list) {
		eprintln!("services: can't save: {e}");
		return error(ErrorCode::Storage);
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
		return error(ErrorCode::NoSuchService);
	}
	if let Err(e) = services::save(&list) {
		eprintln!("services: can't save: {e}");
		return error(ErrorCode::Storage);
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
		return error(ErrorCode::NoSuchService);
	};
	let Some(unit) = def.unit else { return error(ErrorCode::NoUnit) };
	if Bus::unit_method(&r.op).is_none() {
		return failed(reason::UNKNOWN_OPERATION);
	}
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
		Err(e) => failed(e),
	}
}

// ---- what the stream carries -------------------------------------------------------------

/// The slower, D-Bus-backed parts of a sample, refreshed by the collector thread.
pub struct Extras {
	services: Vec<ServiceStatus>,
	profiles: Option<Profiles>,
	volume: Option<Volume>,
	every: Ticks,
	tick: u32,
	/// What the previous tick was read for, to read at once what an app just asked for.
	was_watched: bool,
	had_media: bool,
}

impl Extras {
	pub fn new(every: Ticks) -> Self {
		Extras { services: Vec::new(), profiles: None, volume: None, every, tick: 0, was_watched: false, had_media: false }
	}

	/// Fills in services and power profiles while an app is connected, and returns the media
	/// for the apps that asked for it. Called once per tick off the async runtime.
	pub fn apply(&mut self, hub: &Hub, rt: &tokio::runtime::Handle, watched: bool, want_media: bool, sample: &mut Sample) -> Option<Media> {
		// An action asked for a fresh look; that waits for someone to look.
		let urgent = watched && hub.refresh.swap(false, Ordering::SeqCst);
		if watched && (urgent || !self.was_watched || self.tick.is_multiple_of(self.every.services)) {
			self.services = rt.block_on(hub.service_statuses());
			self.profiles = rt.block_on(hub.bus.profiles());
		}
		let media = if want_media {
			let players = rt.block_on(hub.bus.players());
			if urgent || !self.had_media || self.tick.is_multiple_of(self.every.volume) {
				self.volume = if hub.config().control { volume_get() } else { None };
			}
			Some(Media { players, volume: self.volume.clone() })
		} else {
			None
		};
		self.was_watched = watched;
		self.had_media = want_media;
		self.tick = self.tick.wrapping_add(1);
		if watched {
			sample.services = self.services.clone();
			if let Some(p) = &self.profiles {
				sample.power.ppd_profile = Some(p.active.clone());
				sample.power.ppd_profiles = p.available.clone();
			}
		}
		media
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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn output_keeps_its_start_and_end() {
		let mut c = Capture::default();
		c.push(b"first line\n");
		for _ in 0..10_000 {
			c.push(b"middle middle middle\n");
		}
		c.push("last line ✓\n".as_bytes());
		let text = c.text();
		assert!(text.len() <= limit::OUTPUT_MAX + 8, "{}", text.len());
		assert!(text.starts_with("first line\n"));
		assert!(text.contains("\n…\n"));
		assert!(text.ends_with("last line ✓\n"));
		assert!(!text.contains('\u{fffd}'));

		let mut short = Capture::default();
		short.push("a ✓".as_bytes());
		short.push(b"\nb");
		assert_eq!(short.text(), "a ✓\nb");
	}

	#[test]
	fn proc_files() {
		assert_eq!(ppid("4242 (a) b (c)) S 17 4242 4242 0 -1"), Some(17));
		assert_eq!(tgid("Name:\tx\nTgid:\t99\nPid:\t100\n"), Some(99));
	}

	#[test]
	fn the_service_and_its_ancestors_are_protected() {
		assert!(protected(1));
		assert!(protected(0));
		assert!(protected(-5));
		assert!(protected(std::process::id() as i32));
		let parent = proc_field(std::process::id() as i32, "stat").and_then(|s| ppid(&s)).unwrap();
		assert!(protected(parent));
		let mut child = std::process::Command::new("sleep").arg("5").spawn().unwrap();
		assert!(!protected(child.id() as i32));
		child.kill().unwrap();
		child.wait().unwrap();
	}

	fn gone(pid: i32) -> bool {
		// A zombie waiting for its new parent counts as gone.
		proc_field(pid, "stat").is_none_or(|s| s.rsplit_once(')').is_some_and(|(_, r)| r.trim_start().starts_with('Z')))
	}

	#[tokio::test]
	async fn actions_report_exit_and_output() {
		let args = ["-c".to_string(), "echo out; echo err >&2; exit 3".to_string()];
		let r = run_action("sh", &args, Duration::from_secs(5)).await.unwrap();
		assert_eq!(r.exit_code, Some(3));
		assert!(!r.timed_out);
		assert!(r.output.contains("out\n") && r.output.contains("err\n"), "{:?}", r.output);
	}

	#[tokio::test]
	async fn a_timed_out_action_takes_what_it_started_with_it() {
		let pidfile = std::env::temp_dir().join(format!("elamond-action-{}", std::process::id()));
		let script = format!("echo started; sleep 30 & echo $! > {}; sleep 30", pidfile.display());
		let r = run_action("sh", &["-c".to_string(), script], Duration::from_secs(1)).await.unwrap();
		assert!(r.timed_out);
		assert_eq!(r.exit_code, None);
		assert_eq!(r.output, "started\n");
		let grandchild: i32 = std::fs::read_to_string(&pidfile).unwrap().trim().parse().unwrap();
		std::fs::remove_file(&pidfile).unwrap();
		let deadline = Instant::now() + Duration::from_secs(2);
		while !gone(grandchild) && Instant::now() < deadline {
			std::thread::sleep(Duration::from_millis(20));
		}
		assert!(gone(grandchild), "the background sleep outlived the action");
	}

	#[tokio::test]
	async fn chatty_actions_are_cut() {
		let args = ["-c".to_string(), "head -c 1000000 /dev/zero | tr '\\0' a".to_string()];
		let r = run_action("sh", &args, Duration::from_secs(5)).await.unwrap();
		assert_eq!(r.exit_code, Some(0));
		assert!(r.output.len() <= limit::OUTPUT_MAX + 8);
	}
}
