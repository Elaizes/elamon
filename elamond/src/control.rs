//! Acting on the computer from the app: power, profiles, processes, actions, media, links.
//!
//! Every acting request goes through `Hub::act`: the phone's rate, its role, then control
//! (`control = true` in elamond.toml and no `elamond control off`) for all but the list of
//! links, which only changes what the app shows. Each is recorded in the activity log, let
//! through or not; those let through are also logged to the journal with the phone's name.

use std::collections::VecDeque;
use std::future::Future;
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
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

use crate::access::{self, Can, Caller, Rate, Role};
use crate::activity::{self, Entry};
use crate::bus::{self, Bus, Player, PowerCaps, Profiles, Scope, UnitInfo};
use crate::consts::cadence::{self, Ticks};
use crate::consts::{limit, timeout};
use crate::errors::{reason, ErrorCode};
use crate::model::Sample;
use crate::server::{error, failed, Hub};
use crate::links::{self, LinkDef, LinkStatus};
use crate::store;

/// What an acting request answered, and a word on how it went for the activity log.
pub struct Acted {
	response: Response,
	note: Option<String>,
}

impl Acted {
	fn ok() -> Acted {
		Json(json!({ "ok": true })).into_response().into()
	}
}

impl From<Response> for Acted {
	fn from(response: Response) -> Acted {
		Acted { response, note: None }
	}
}

/// An acting request turned down or failed: the code it is answered and logged with, and for
/// `failed` what went wrong.
#[derive(Debug, PartialEq)]
pub struct Refusal {
	code: ErrorCode,
	message: Option<String>,
}

impl Refusal {
	fn failed(message: impl Into<String>) -> Refusal {
		Refusal { code: ErrorCode::Failed, message: Some(message.into()) }
	}
}

impl From<ErrorCode> for Refusal {
	fn from(code: ErrorCode) -> Refusal {
		Refusal { code, message: None }
	}
}

impl IntoResponse for Refusal {
	fn into_response(self) -> Response {
		match self.message {
			Some(m) if self.code == ErrorCode::Failed => failed(m),
			_ => error(self.code),
		}
	}
}

fn done(result: Result<(), String>) -> Result<Acted, Refusal> {
	match result {
		Ok(()) => Ok(Acted::ok()),
		Err(e) if e == ErrorCode::NotPermitted.as_str() => Err(ErrorCode::NotPermitted.into()),
		Err(e) => Err(Refusal::failed(e)),
	}
}

impl Hub {
	/// 403 `not_allowed` unless the phone's role lets it see `can`.
	pub fn refuse_read(&self, who: &Caller, can: Can) -> Option<Response> {
		(!who.role.may(can)).then(|| error(ErrorCode::NotAllowed))
	}

	/// Whether phones may act right now: the settings, and `elamond control off`'s file looked
	/// at afresh, so holding control off takes effect from the next request.
	pub fn control_now(&self) -> bool {
		!self.sync_control_off() && self.config().control
	}

	/// An acting request from `who`, described for the log as `what`: refused when the phone
	/// acts too often (429 `too_many_requests`), when its role doesn't allow `can` (403
	/// `not_allowed`) or when `can` needs control and it is off (403 `control_disabled`), in
	/// that order; otherwise `work` runs. Either way it goes in the activity log, except the
	/// refusals for acting too often after the first in a row, which would only crowd it.
	pub async fn act(&self, who: &Caller, can: Can, what: String, work: impl Future<Output = Result<Acted, Refusal>>) -> Response {
		let rate = self.rate.lock().map(|mut r| r.take(&who.id, Instant::now())).unwrap_or(Rate::Allowed);
		let outcome = match rate {
			Rate::Refused { first: false } => return error(ErrorCode::TooManyRequests),
			Rate::Refused { first: true } => Err(ErrorCode::TooManyRequests.into()),
			Rate::Allowed if !who.role.may(can) => Err(ErrorCode::NotAllowed.into()),
			Rate::Allowed if can.needs_control() && !self.control_now() => Err(ErrorCode::ControlDisabled.into()),
			Rate::Allowed => work.await,
		};
		let (result, message, response) = match outcome {
			Ok(a) => ("ok".to_string(), a.note, a.response),
			Err(r) => (r.code.as_str().to_string(), r.message.clone(), r.into_response()),
		};
		self.activity.record(Entry {
			t: activity::now_ms(),
			phone: who.name.clone(),
			client_id: who.id.clone(),
			what: activity::clean(&what),
			result,
			message: message.map(|m| activity::clean(&m)),
		});
		response
	}
}

#[derive(Serialize)]
pub(crate) struct ControlInfo {
	/// Control is on: `control = true` and no `elamond control off`.
	pub(crate) enabled: bool,
	pub(crate) power: PowerCaps,
	pub(crate) profiles: Option<Profiles>,
	pub(crate) actions: Vec<ActionInfo>,
	pub(crate) volume: bool,
	/// The asking phone's role, and what it may do now (`access::Can` by name).
	pub(crate) role: Role,
	pub(crate) allowed: Vec<&'static str>,
}

#[derive(Serialize)]
pub(crate) struct ActionInfo {
	pub(crate) id: String,
	pub(crate) name: String,
	pub(crate) confirm: bool,
}

/// What the app may offer on this computer right now, to this phone: power, profiles, actions
/// and the volume only as far as its role allows them, so even an app from before roles shows
/// no more than the phone may use.
pub async fn info(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>) -> Response {
	let enabled = hub.control_now();
	let may = |c: Can| enabled && who.role.may(c);
	let power = if may(Can::Power) { hub.bus.power_caps().await } else { PowerCaps::default() };
	let profiles = if may(Can::Profile) { hub.bus.profiles().await } else { None };
	let actions = if may(Can::Actions) {
		hub.config().actions.iter().map(|a| ActionInfo { id: a.id(), name: a.name.clone(), confirm: a.confirm }).collect()
	} else {
		Vec::new()
	};
	let volume = may(Can::Volume) && tokio::task::spawn_blocking(volume_get).await.ok().flatten().is_some();
	let allowed = access::allowed(who.role, enabled);
	Json(ControlInfo { enabled, power, profiles, actions, volume, role: who.role, allowed }).into_response()
}

#[derive(Deserialize)]
pub struct LogQuery {
	limit: Option<usize>,
}

/// The activity log, newest first.
pub async fn log(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Query(q): Query<LogQuery>) -> Response {
	if let Some(r) = hub.refuse_read(&who, Can::Log) {
		return r;
	}
	Json(hub.activity.newest(q.limit.unwrap_or(limit::ACTIVITY_MAX))).into_response()
}

#[derive(Deserialize)]
pub struct PowerReq {
	action: String,
}

pub async fn power(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Json(r): Json<PowerReq>) -> Response {
	hub.act(&who, Can::Power, format!("power {}", r.action), async {
		if Bus::power_method(&r.action).is_none() {
			return Err(Refusal::failed(reason::UNKNOWN_ACTION));
		}
		eprintln!("control: {} asked to {}", who.name, r.action);
		done(hub.bus.power(&r.action).await)
	})
	.await
}

#[derive(Deserialize)]
pub struct ProfileReq {
	profile: String,
}

pub async fn profile(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Json(r): Json<ProfileReq>) -> Response {
	hub.act(&who, Can::Profile, format!("profile {}", r.profile), async {
		let Some(profiles) = hub.bus.profiles().await else {
			return Err(Refusal::failed(reason::NO_POWER_PROFILES));
		};
		if !profiles.available.contains(&r.profile) {
			return Err(Refusal::failed(reason::UNKNOWN_PROFILE));
		}
		eprintln!("control: {} set the power profile to {}", who.name, r.profile);
		let res = hub.bus.set_profile(&r.profile).await;
		hub.refresh_soon();
		done(res)
	})
	.await
}

#[derive(Deserialize)]
pub struct SignalReq {
	pid: i32,
	/// "term" asks the process to quit, "kill" ends it outright.
	signal: String,
	/// When the process started (Unix s, as in samples): if the pid now belongs to a process
	/// that started at another time, nothing is signalled.
	start: Option<u64>,
}

pub async fn signal(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Json(r): Json<SignalReq>) -> Response {
	let what = match proc_field(r.pid, "comm") {
		Some(name) if r.pid > 0 => format!("signal {} {} {}", r.signal, r.pid, name.trim()),
		_ => format!("signal {} {}", r.signal, r.pid),
	};
	hub.act(&who, Can::Signal, what, async {
		let sig = match r.signal.as_str() {
			"term" => libc::SIGTERM,
			"kill" => libc::SIGKILL,
			_ => return Err(ErrorCode::UnknownSignal.into()),
		};
		if protected(r.pid) {
			return Err(ErrorCode::NotPermitted.into());
		}
		send_signal(r.pid, sig, r.start, hub.boot_time, || eprintln!("control: {} sent {} to pid {}", who.name, r.signal, r.pid))?;
		Ok(Acted::ok())
	})
	.await
}

/// Signals `pid`, if it is still the process that started at `start` (when given) by the
/// clock that counts from `boot_time`. A pidfd holds the process between the look and the
/// signal, so the pid can't come to mean another one in between; a thread's id, or a kernel
/// without pidfds, falls back to kill(2). `sending` is called once it is decided to send.
fn send_signal(pid: i32, sig: i32, start: Option<u64>, boot_time: u64, sending: impl FnOnce()) -> Result<(), Refusal> {
	let errno = || std::io::Error::last_os_error();
	let refusal = |e: std::io::Error| -> Refusal {
		match e.raw_os_error() {
			Some(libc::EPERM) => ErrorCode::NotPermitted.into(),
			Some(libc::ESRCH) => ErrorCode::NoSuchProcess.into(),
			_ => Refusal::failed(e.to_string()),
		}
	};
	// SAFETY: pidfd_open(2) with a plain pid and no flags; a new descriptor is returned or -1.
	let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
	let pidfd = if fd >= 0 {
		// SAFETY: the descriptor was just opened and nothing else owns it.
		Some(unsafe { OwnedFd::from_raw_fd(fd as i32) })
	} else {
		match errno() {
			e if matches!(e.raw_os_error(), Some(libc::ENOSYS | libc::EINVAL)) => None,
			e => return Err(refusal(e)),
		}
	};
	if let Some(start) = start {
		match process_start(pid, boot_time) {
			None => return Err(ErrorCode::NoSuchProcess.into()),
			Some(s) if s != start => return Err(ErrorCode::ProcessChanged.into()),
			Some(_) => {}
		}
	}
	sending();
	let rc = match &pidfd {
		// SAFETY: pidfd_send_signal(2) on a pidfd we own, with no siginfo and no flags.
		Some(fd) => unsafe { libc::syscall(libc::SYS_pidfd_send_signal, fd.as_raw_fd(), sig, std::ptr::null::<libc::siginfo_t>(), 0) },
		// SAFETY: kill(2) with a plain pid and signal number has no memory effects.
		None => (unsafe { libc::kill(pid, sig) }) as libc::c_long,
	};
	if rc == 0 { Ok(()) } else { Err(refusal(errno())) }
}

/// When a process started, in Unix seconds, as the process list counts it: clock ticks after
/// boot from /proc/<pid>/stat, whole seconds of them, plus the boot time.
fn process_start(pid: i32, boot_time: u64) -> Option<u64> {
	if pid <= 0 {
		return None;
	}
	let stat = proc_field(pid, "stat")?;
	let ticks: u64 = stat.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()?;
	// SAFETY: sysconf(3) only reads a value.
	let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
	(hz > 0).then(|| ticks / hz as u64 + boot_time)
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

pub async fn action(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Json(r): Json<ActionReq>) -> Response {
	let config = hub.config();
	let found = config.actions.iter().find(|a| a.id() == r.id);
	let what = format!("action {}", found.map_or(r.id.as_str(), |a| a.name.as_str()));
	hub.act(&who, Can::Actions, what, async {
		let Some(a) = found else {
			return Err(ErrorCode::NoSuchAction.into());
		};
		let Some((program, args)) = a.command.split_first() else {
			return Err(ErrorCode::EmptyCommand.into());
		};
		eprintln!("control: {} ran \"{}\"", who.name, a.name);
		let limit = Duration::from_secs(a.timeout).clamp(timeout::ACTION_MIN, timeout::ACTION_MAX);
		let result = run_action(program, args, limit).await;
		hub.refresh_soon();
		match result {
			Ok(r) => {
				let note = match (r.timed_out, r.exit_code) {
					(true, _) => "timed out".to_string(),
					(false, Some(c)) => format!("exit {c}"),
					(false, None) => "ended by a signal".to_string(),
				};
				Ok(Acted { response: Json(r).into_response(), note: Some(note) })
			}
			Err(e) => Err(Refusal::failed(e.to_string())),
		}
	})
	.await
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

pub async fn media(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Json(r): Json<MediaReq>) -> Response {
	let what = match r.value {
		Some(v) => format!("media {} {v}", r.op),
		None => format!("media {}", r.op),
	};
	hub.act(&who, Can::Media, what, async {
		if !bus::is_player(&r.player) {
			return Err(Refusal::failed(reason::UNKNOWN_PLAYER));
		}
		if !Bus::is_player_op(&r.op) {
			return Err(Refusal::failed(reason::UNKNOWN_OPERATION));
		}
		match r.value {
			Some(v) => eprintln!("control: {} asked {} to {} {v}", who.name, r.player, r.op),
			None => eprintln!("control: {} asked {} to {}", who.name, r.player, r.op),
		}
		let res = hub.bus.player_op(&r.player, &r.op, r.value).await;
		hub.refresh_soon();
		done(res)
	})
	.await
}

#[derive(Deserialize)]
pub struct VolumeReq {
	level: Option<f32>,
	muted: Option<bool>,
}

pub async fn volume(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Json(r): Json<VolumeReq>) -> Response {
	let mut what = "volume".to_string();
	if let Some(l) = r.level {
		what.push_str(&format!(" {:.0} %", l.clamp(0.0, 1.0) * 100.0));
	}
	if let Some(m) = r.muted {
		what.push_str(if m { " muted" } else { " unmuted" });
	}
	hub.act(&who, Can::Volume, what, async {
		if let Some(l) = r.level {
			eprintln!("control: {} set the volume to {:.0} %", who.name, l.clamp(0.0, 1.0) * 100.0);
		}
		if let Some(m) = r.muted {
			eprintln!("control: {} {} the sound", who.name, if m { "muted" } else { "unmuted" });
		}
		let res = tokio::task::spawn_blocking(move || volume_set(r.level, r.muted)).await.unwrap_or_else(|e| Err(e.to_string()));
		hub.refresh_soon();
		done(res)
	})
	.await
}

#[derive(Deserialize)]
pub struct ArtQuery {
	player: String,
}

/// A player's cover. Seeing it is reading the media, like the players in samples, so it needs
/// the role for that but not control.
pub async fn art(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Query(q): Query<ArtQuery>) -> Response {
	if let Some(r) = hub.refuse_read(&who, Can::MediaRead) {
		return r;
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
	run_within(argv, timeout::COMMAND)
}

/// Runs a command that prints little and returns its output, or nothing if it failed, isn't there
/// or took longer than `limit`; then it is killed with whatever it started. The output is read once
/// the command has ended, so it has to fit in a pipe (64 KiB on Linux).
pub fn run_within(argv: &[&str], limit: Duration) -> Option<String> {
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
	let deadline = Instant::now() + limit;
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

// ---- links -----------------------------------------------------------------------------

pub async fn links_list(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>) -> Response {
	if let Some(r) = hub.refuse_read(&who, Can::Links) {
		return r;
	}
	Json(hub.link_statuses().await).into_response()
}

#[derive(Deserialize)]
pub struct LinkReq {
	id: Option<String>,
	name: String,
	url: String,
	unit: Option<String>,
	scope: Option<String>,
}

pub async fn link_put(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Json(r): Json<LinkReq>) -> Response {
	let what = format!("link {} {}", if r.id.is_some() { "change" } else { "add" }, r.name.trim());
	hub.act(&who, Can::LinksEdit, what, async {
		let unit = r.unit.as_deref().map(str::trim).filter(|u| !u.is_empty());
		links::validate(&r.name, &r.url, unit, r.scope.as_deref())?;
		let mut list = hub.app_links.lock().await;
		let def = LinkDef {
			id: r.id.clone().unwrap_or_else(|| data_encoding::HEXLOWER.encode(&store::random_bytes::<{ crate::consts::protocol::LINK_ID_BYTES }>())),
			name: r.name.trim().to_string(),
			url: r.url.trim().to_string(),
			unit: unit.map(str::to_string),
			scope: r.scope.clone(),
		};
		match list.iter_mut().find(|s| s.id == def.id) {
			Some(s) => *s = def.clone(),
			None if r.id.is_none() => list.push(def.clone()),
			None => return Err(ErrorCode::NoSuchLink.into()),
		}
		if let Err(e) = links::save_in(&hub.dir, &list) {
			eprintln!("links: can't save: {e}");
			return Err(ErrorCode::Storage.into());
		}
		eprintln!("links: {} saved \"{}\"", who.name, def.name);
		drop(list);
		hub.refresh_soon();
		Ok(Json(def).into_response().into())
	})
	.await
}

pub async fn link_delete(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Path(id): Path<String>) -> Response {
	let name = hub.app_links.lock().await.iter().find(|l| l.id == id).map(|l| l.name.clone());
	let what = format!("link remove {}", name.as_deref().unwrap_or(&id));
	hub.act(&who, Can::LinksEdit, what, async {
		let mut list = hub.app_links.lock().await;
		let before = list.len();
		list.retain(|s| s.id != id);
		if list.len() == before {
			return Err(ErrorCode::NoSuchLink.into());
		}
		if let Err(e) = links::save_in(&hub.dir, &list) {
			eprintln!("links: can't save: {e}");
			return Err(ErrorCode::Storage.into());
		}
		eprintln!("links: {} removed {id}", who.name);
		drop(list);
		hub.refresh_soon();
		Ok(Acted::ok())
	})
	.await
}

#[derive(Deserialize)]
pub struct UnitReq {
	op: String,
}

pub async fn link_unit(
	State(hub): State<Arc<Hub>>,
	Extension(who): Extension<Caller>,
	Path(id): Path<String>,
	Json(r): Json<UnitReq>,
) -> Response {
	let app = hub.app_links.lock().await.clone();
	let found = links::all(&hub.config(), &app).into_iter().find(|(s, _)| s.id == id).map(|(def, _)| def);
	let what = format!("unit {} {}", r.op, found.as_ref().and_then(|d| d.unit.as_deref()).unwrap_or(&id));
	hub.act(&who, Can::Units, what, async {
		let Some(def) = found else {
			return Err(ErrorCode::NoSuchLink.into());
		};
		let Some(unit) = def.unit else { return Err(ErrorCode::NoUnit.into()) };
		if Bus::unit_method(&r.op).is_none() {
			return Err(Refusal::failed(reason::UNKNOWN_OPERATION));
		}
		eprintln!("control: {} asked to {} {unit}", who.name, r.op);
		let res = hub.bus.unit_op(Scope::parse(def.scope.as_deref()), &unit, &r.op).await;
		hub.refresh_soon();
		done(res)
	})
	.await
}

pub async fn detected(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>) -> Response {
	if let Some(r) = hub.refuse_read(&who, Can::Links) {
		return r;
	}
	let list = tokio::task::spawn_blocking(links::detect).await.unwrap_or_default();
	Json(list).into_response()
}

#[derive(Deserialize)]
pub struct UnitsQuery {
	scope: Option<String>,
}

pub async fn units(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Query(q): Query<UnitsQuery>) -> Response {
	if let Some(r) = hub.refuse_read(&who, Can::Links) {
		return r;
	}
	match hub.bus.units(Scope::parse(q.scope.as_deref())).await {
		Ok(list) => Json::<Vec<UnitInfo>>(list).into_response(),
		Err(e) => failed(e),
	}
}

// ---- what the stream carries -------------------------------------------------------------

/// The slower, D-Bus-backed parts of a sample, refreshed by the collector thread.
pub struct Extras {
	links: Vec<LinkStatus>,
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
		Extras { links: Vec::new(), profiles: None, volume: None, every, tick: 0, was_watched: false, had_media: false }
	}

	/// Fills in links and power profiles while an app is connected, and returns the media
	/// for the apps that asked for it. Called once per tick off the async runtime.
	pub fn apply(&mut self, hub: &Hub, rt: &tokio::runtime::Handle, watched: bool, want_media: bool, sample: &mut Sample) -> Option<Media> {
		// An action asked for a fresh look; that waits for someone to look.
		let urgent = watched && hub.refresh.swap(false, Ordering::SeqCst);
		if watched && (urgent || !self.was_watched || self.tick.is_multiple_of(self.every.links)) {
			self.links = rt.block_on(hub.link_statuses());
			self.profiles = rt.block_on(hub.bus.profiles());
		}
		let media = if want_media {
			let players = rt.block_on(hub.bus.players());
			if urgent || !self.had_media || self.tick.is_multiple_of(self.every.volume) {
				self.volume = if hub.control_on() { volume_get() } else { None };
			}
			Some(Media { players, volume: self.volume.clone() })
		} else {
			None
		};
		self.was_watched = watched;
		self.had_media = want_media;
		self.tick = self.tick.wrapping_add(1);
		if watched {
			sample.links = self.links.clone();
			sample.services = self.links.clone();
			if let Some(p) = &self.profiles {
				sample.power.ppd_profile = Some(p.active.clone());
				sample.power.ppd_profiles = p.available.clone();
			}
		}
		media
	}
}

impl Hub {
	/// Link statuses with their units' states.
	pub async fn link_statuses(&self) -> Vec<LinkStatus> {
		let app = self.app_links.lock().await.clone();
		let config = self.config();
		let mut list = tokio::task::spawn_blocking(move || links::statuses(&config, &app)).await.unwrap_or_default();
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

	#[test]
	fn start_times_count_as_the_process_list_does() {
		let boot = sysinfo::System::boot_time().unwrap();
		let pid = std::process::id();
		let mut sys = sysinfo::System::new().unwrap();
		let p = sysinfo::Pid::from_u32(pid);
		sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[p]), true);
		assert_eq!(process_start(pid as i32, boot), Some(sys.process(p).unwrap().start_time()));
		assert_eq!(process_start(0, boot), None);
		assert_eq!(process_start(-1, boot), None);
	}

	#[test]
	fn a_signal_with_a_start_time_only_reaches_that_process() {
		use std::os::unix::process::ExitStatusExt;
		let boot = sysinfo::System::boot_time().unwrap();
		let mut child = std::process::Command::new("sleep").arg("30").spawn().unwrap();
		let pid = child.id() as i32;
		let start = process_start(pid, boot).unwrap();
		let mut sent = false;
		// The pid now names a process that started at another time: nothing is sent.
		assert_eq!(send_signal(pid, libc::SIGKILL, Some(start + 1), boot, || sent = true), Err(ErrorCode::ProcessChanged.into()));
		assert!(!sent);
		assert!(child.try_wait().unwrap().is_none());
		send_signal(pid, libc::SIGKILL, Some(start), boot, || sent = true).unwrap();
		assert!(sent);
		assert_eq!(child.wait().unwrap().signal(), Some(libc::SIGKILL));
		// Gone, with or without a start time; signal 0 only asks, should the pid be reused.
		assert_eq!(send_signal(pid, 0, Some(start), boot, || ()), Err(ErrorCode::NoSuchProcess.into()));
		assert_eq!(send_signal(pid, 0, None, boot, || ()), Err(ErrorCode::NoSuchProcess.into()));
	}
}
