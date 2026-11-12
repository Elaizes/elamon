//! Acting on the computer: `POST /api/v1/actions/{id}` (docs/protocol.md) runs a built-in action
//! (power, the power profile, signals, the links' units, media, the volume) or one the settings
//! add; and what the collector reads through D-Bus and commands for the values.
//!
//! Every acting request goes through `Hub::act_as`: the phone's rate, its role, then control
//! (`control = true` in the settings and no `elamond control off`). Each is recorded in the
//! activity log, let through or not; those let through are also logged to the journal with the
//! phone's name.

use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::access::{Caller, Rate, Role};
use crate::actions::{self, Caps, Spec};
use crate::activity::{self, Entry};
use crate::bus::{Player, PowerCaps, Profiles, Scope};
use crate::consts::cadence::{self, Ticks};
use crate::consts::{limit, timeout};
use crate::errors::{reason, ErrorCode};
use crate::links::{self, LinkStatus};
use crate::model::Sample;
use crate::server::{body_of, error, error_with, ClientApp, Hub};
use crate::ui::Sources;

/// What an acting request answered, and a word on how it went for the activity log.
pub struct Acted {
	pub(crate) response: Response,
	pub(crate) note: Option<String>,
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

/// An acting request turned down or failed: the code it is answered and logged with, and what
/// failed (`failed`) or which parameter is wrong (`bad_param`).
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
			Some(m) => error_with(self.code, m),
			None => error(self.code),
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
	/// Whether phones may act right now: the settings, and `elamond control off`'s file looked
	/// at afresh, so holding control off takes effect from the next request.
	pub fn control_now(&self) -> bool {
		self.sync_control_off();
		self.control_on()
	}

	/// An acting request from `who`, described for the log as `what`: refused when the phone acts
	/// too often (429 `too_many_requests`), when its role is below `need` (403 `not_allowed`) or,
	/// with `control`, when control is off (403 `control_disabled`), in that order; otherwise
	/// `work` runs. Either way it goes in the activity log, except the refusals for acting too
	/// often after the first in a row, which would only crowd it.
	pub async fn act_as(&self, who: &Caller, need: Option<Role>, control: bool, what: String, work: impl Future<Output = Result<Acted, Refusal>>) -> Response {
		// One rate for a phone and the keys of their own its token gave.
		let rate = self.rate.lock().map(|mut r| r.take(&who.family, Instant::now())).unwrap_or(Rate::Allowed);
		let outcome = match rate {
			Rate::Refused { first: false } => return error(ErrorCode::TooManyRequests),
			Rate::Refused { first: true } => Err(ErrorCode::TooManyRequests.into()),
			Rate::Allowed if need.is_some_and(|r| who.role < r) => Err(ErrorCode::NotAllowed.into()),
			Rate::Allowed if control && !self.control_now() => Err(ErrorCode::ControlDisabled.into()),
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
			family: (who.family != who.id).then(|| who.family.clone()),
		});
		response
	}
}

// ---- actions ---------------------------------------------------------------------------

/// `POST /api/v1/actions/{id}`.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct ActionRequest {
	pub params: Map<String, Value>,
}

/// What an action the settings add answered: how its command ended, and what it wrote.
#[derive(Serialize, Debug)]
pub(crate) struct ActionResult {
	pub(crate) ok: bool,
	pub(crate) exit_code: Option<i32>,
	pub(crate) timed_out: bool,
	pub(crate) output: String,
}

/// Runs an action of the client's `ui`: 404 `no_such_action` for one the computer doesn't have
/// (or can't do), 403 `not_allowed` for one the client's role may not run, 403 `control_disabled`
/// while control is off, 400 `bad_param` naming the first parameter that is wrong.
pub async fn action(
	State(hub): State<Arc<Hub>>,
	Extension(who): Extension<Caller>,
	Extension(app): Extension<ClientApp>,
	Path(id): Path<String>,
	body: Bytes,
) -> Response {
	let req: ActionRequest = match body_of(&body) {
		Ok(r) => r,
		Err(r) => return *r,
	};
	let config = hub.config();
	let frame = hub.frame.borrow().clone();
	let ui = hub.ui(&hub.ui_key(&who, &app.kind, hub.draws_of(&who.id, &app.kind)), &frame);
	// One the computer can do that the role may not run is refused as such, not as unknown.
	let spec = ui.actions.iter().find(|s| s.id == id).cloned().or_else(|| actions::available(&config, &frame.caps).into_iter().find(|s| s.id == id));
	let what = what_of(&id, spec.as_ref(), &req.params, &config);
	let (need, control) = match &spec {
		Some(s) => (Some(s.role), true),
		None => (None, false),
	};
	hub.act_as(&who, need, control, what, async {
		let Some(spec) = spec else {
			return Err(ErrorCode::NoSuchAction.into());
		};
		let params = actions::check_params(&spec, &req.params).map_err(|p| Refusal { code: ErrorCode::BadParam, message: Some(p) })?;
		let result = run_spec(&hub, &who, &spec, &params, &config).await;
		hub.refresh_soon();
		result
	})
	.await
}

/// A parameter's value as words: `server`, `40`, `true`.
fn word(v: &Value) -> String {
	match v {
		Value::String(s) => s.clone(),
		other => other.to_string(),
	}
}

/// What the activity log says was asked: `power reboot`, `profile balanced`,
/// `signal term 1234 firefox`, `unit restart nginx.service`, `media play_pause`, `volume 40 %`,
/// `action Mode server`.
fn what_of(id: &str, spec: Option<&Spec>, params: &Map<String, Value>, config: &crate::settings::Config) -> String {
	let p = |k: &str| params.get(k).map(word).unwrap_or_default();
	if let Some(name) = id.strip_prefix("power.") {
		return format!("power {name}");
	}
	if let Some(op) = id.strip_prefix("unit.") {
		let link = p("link");
		let unit = config.links.get(&link).and_then(|l| l.unit.clone()).unwrap_or(link);
		return format!("unit {op} {unit}");
	}
	match id {
		"profile.set" => format!("profile {}", p("profile")),
		"process.signal" => {
			let pid = params.get("pid").and_then(Value::as_i64).and_then(|p| i32::try_from(p).ok());
			match pid.filter(|p| *p > 0).and_then(|p| proc_field(p, "comm")) {
				Some(name) => format!("signal {} {} {}", p("signal"), p("pid"), name.trim()),
				None => format!("signal {} {}", p("signal"), p("pid")),
			}
		}
		"media.seek" => format!("media seek {}", p("position")),
		"volume.set" => {
			let mut what = "volume".to_string();
			if let Some(l) = params.get("level").and_then(Value::as_f64) {
				what.push_str(&format!(" {l:.0} %"));
			}
			if let Some(m) = params.get("muted").and_then(Value::as_bool) {
				what.push_str(if m { " muted" } else { " unmuted" });
			}
			what
		}
		_ if id.starts_with("media.") => format!("media {}", &id["media.".len()..]),
		_ => match (config.actions.get(id), spec) {
			(Some(a), Some(s)) => {
				let mut words = vec![format!("action {}", a.name)];
				for param in &s.params {
					if let Some(v) = params.get(&param.id).or(param.default.as_ref()) {
						words.push(word(v));
					}
				}
				words.join(" ")
			}
			(Some(a), None) => format!("action {}", a.name),
			_ => format!("action {id}"),
		},
	}
}

/// Runs an action whose parameters `params` were checked.
async fn run_spec(hub: &Hub, who: &Caller, spec: &Spec, params: &BTreeMap<String, Value>, config: &crate::settings::Config) -> Result<Acted, Refusal> {
	let text = |k: &str| params.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
	let id = spec.id.as_str();
	if let Some(name) = id.strip_prefix("power.") {
		eprintln!("control: {} asked to {name}", who.name);
		return done(hub.bus.power(name).await);
	}
	if let Some(op) = id.strip_prefix("unit.") {
		let Some(link) = config.links.get(&text("link")) else {
			return Err(Refusal { code: ErrorCode::BadParam, message: Some("link".into()) });
		};
		let Some(unit) = link.unit.clone() else {
			return Err(Refusal { code: ErrorCode::BadParam, message: Some("link".into()) });
		};
		if let Some(why) = links::unit_refusal(config, &unit, Some(link.scope())) {
			eprintln!("control: {} asked to {op} {unit}, refused: {why}", who.name);
			return Err(ErrorCode::NotAllowed.into());
		}
		eprintln!("control: {} asked to {op} {unit}", who.name);
		return done(hub.bus.unit_op(Scope::parse(Some(link.scope())), &unit, op).await);
	}
	match id {
		"profile.set" => {
			let profile = text("profile");
			eprintln!("control: {} set the power profile to {profile}", who.name);
			done(hub.bus.set_profile(&profile).await)
		}
		"process.signal" => {
			let Some(pid) = params.get("pid").and_then(Value::as_i64).and_then(|p| i32::try_from(p).ok()) else {
				return Err(Refusal { code: ErrorCode::BadParam, message: Some("pid".into()) });
			};
			let signal = text("signal");
			let sig = match signal.as_str() {
				"term" => libc::SIGTERM,
				"kill" => libc::SIGKILL,
				_ => return Err(ErrorCode::UnknownSignal.into()),
			};
			if protected(pid) {
				return Err(ErrorCode::NotPermitted.into());
			}
			let start = params.get("start").and_then(Value::as_u64);
			send_signal(pid, sig, start, hub.boot_time, || eprintln!("control: {} sent {signal} to pid {pid}", who.name))?;
			Ok(Acted::ok())
		}
		"media.play_pause" | "media.next" | "media.previous" => {
			let (player, op) = (text("player"), &id["media.".len()..]);
			eprintln!("control: {} asked {player} to {op}", who.name);
			done(hub.bus.player_op(&player, op, None).await)
		}
		"media.seek" => {
			let (player, position) = (text("player"), params.get("position").and_then(Value::as_f64));
			eprintln!("control: {} asked {player} to seek to {} s", who.name, position.unwrap_or_default());
			done(hub.bus.player_op(&player, "seek", position).await)
		}
		"volume.set" => {
			let level = params.get("level").and_then(Value::as_f64).map(|l| (l / 100.0) as f32);
			let muted = params.get("muted").and_then(Value::as_bool);
			if level.is_none() && muted.is_none() {
				return Err(Refusal::failed(reason::NO_VALUE));
			}
			if let Some(l) = level {
				eprintln!("control: {} set the volume to {:.0} %", who.name, l * 100.0);
			}
			if let Some(m) = muted {
				eprintln!("control: {} {} the sound", who.name, if m { "muted" } else { "unmuted" });
			}
			done(tokio::task::spawn_blocking(move || volume_set(level, muted)).await.unwrap_or_else(|e| Err(e.to_string())))
		}
		_ => {
			let Some(a) = config.actions.get(id) else {
				return Err(ErrorCode::NoSuchAction.into());
			};
			let command = actions::command(a, params);
			let Some((program, args)) = command.split_first() else {
				return Err(Refusal::failed("the action has no command"));
			};
			eprintln!("control: {} ran \"{}\"", who.name, a.name);
			let limit = Duration::from_secs(a.timeout).clamp(timeout::ACTION_MIN, timeout::ACTION_MAX);
			match run_action(program, args, limit).await {
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
		}
	}
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
	Ok(ActionResult { ok: true, exit_code: status.and_then(|s| s.code()), timed_out, output: capture.text() })
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
pub struct ArtQuery {
	player: String,
}

/// A player's cover. Seeing it is reading the media, like the players in `media` widgets, so it
/// needs the role for that but not control.
pub async fn art(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Query(q): Query<ArtQuery>) -> Response {
	if who.role < Role::Monitor {
		return error(ErrorCode::NotAllowed);
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
	let spawn = |program: &std::path::Path| {
		std::process::Command::new(program)
			.args(&argv[1..])
			// pactl translates its answers.
			.env("LC_ALL", "C")
			.stdin(Stdio::null())
			.stdout(Stdio::piped())
			.stderr(Stdio::null())
			.process_group(0)
			.spawn()
	};
	let mut child = match spawn(std::path::Path::new(argv[0])) {
		Ok(c) => c,
		// Not on the service's PATH, which a unit may keep short (NixOS's user units): where
		// programs live anyway.
		Err(e) if e.kind() == std::io::ErrorKind::NotFound && !argv[0].contains('/') => spawn(&locate(argv[0], &program_dirs())?).ok()?,
		Err(_) => return None,
	};
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

/// Where programs live whether or not the service's PATH says so: the setuid wrappers and the
/// profiles of NixOS (the user's, the system's, Nix's), then the usual places.
fn program_dirs() -> Vec<std::path::PathBuf> {
	let mut dirs = vec![std::path::PathBuf::from("/run/wrappers/bin")];
	if let Some(home) = std::env::var_os("HOME") {
		dirs.push(std::path::Path::new(&home).join(".nix-profile/bin"));
	}
	if let Some(user) = std::env::var_os("USER").or_else(|| std::env::var_os("LOGNAME")) {
		dirs.push(std::path::Path::new("/etc/profiles/per-user").join(user).join("bin"));
	}
	dirs.extend(["/nix/var/nix/profiles/default/bin", "/run/current-system/sw/bin", "/usr/local/bin", "/usr/bin", "/bin"].map(Into::into));
	dirs
}

/// The program `name` in the first of `dirs` that has it.
fn locate(name: &str, dirs: &[std::path::PathBuf]) -> Option<std::path::PathBuf> {
	dirs.iter().map(|d| d.join(name)).find(|p| p.is_file())
}


impl Hub {
	/// Link statuses with their units' states.
	pub async fn link_statuses(&self) -> Vec<LinkStatus> {
		let config = self.config();
		let given = config.clone();
		let mut list = tokio::task::spawn_blocking(move || links::statuses(&given)).await.unwrap_or_default();
		for s in &mut list {
			if let Some(u) = &s.unit
				&& links::unit_refusal(&config, u, Some(&s.scope)).is_none()
			{
				s.unit_state = self.bus.unit_state(Scope::parse(Some(&s.scope)), u).await;
			}
		}
		list
	}
}


// ---- what the collector reads through D-Bus and commands ---------------------------------

/// The slower parts of a reading, each at its own pace: the links' states, the power profiles, the
/// players and the volume, and what the computer can do. What isn't read on a tick stays as last
/// read, so the values (and a client's `ui`) don't come and go as dashboards do.
pub struct Extras {
	links: Vec<LinkStatus>,
	profiles: Option<Profiles>,
	power: PowerCaps,
	media: Option<Media>,
	/// The volume could be read, last it was tried.
	volume: bool,
	every: Ticks,
	tick: u32,
	/// What the previous tick was read for, to read at once what a client just asked for.
	was_any: bool,
	was: Sources,
}

/// What `Extras::apply` gives a reading.
pub struct Got {
	pub media: Option<Media>,
	pub caps: Caps,
	/// What of `Sources` it read afresh.
	pub read: Sources,
}

impl Extras {
	pub fn new(every: Ticks) -> Self {
		Extras {
			links: Vec::new(),
			profiles: None,
			power: PowerCaps::default(),
			media: None,
			volume: false,
			every,
			tick: 0,
			was_any: false,
			was: Sources::default(),
		}
	}

	/// Reads what is due while clients look (`any`) at dashboards that need `need`, and puts the
	/// links and the power profiles in `sample`. Called once per tick off the async runtime.
	pub fn apply(&mut self, hub: &Hub, rt: &tokio::runtime::Handle, any: bool, need: Sources, sample: &mut Sample) -> Got {
		// After an action, or new settings, a fresh look at everything.
		let urgent = hub.refresh.swap(false, Ordering::SeqCst);
		let rescan = self.tick == 0 || urgent || self.tick.is_multiple_of(self.every.rescan);
		let every = |n: u32| self.tick.is_multiple_of(n);
		if rescan {
			self.power = rt.block_on(hub.bus.power_caps());
		}
		if rescan || (need.links && (!self.was.links || every(self.every.links))) {
			self.links = rt.block_on(hub.link_statuses());
		}
		if rescan || (any && (!self.was_any || every(self.every.links))) {
			self.profiles = rt.block_on(hub.bus.profiles());
		}
		let mut read = Sources { links: need.links, ..Default::default() };
		let sound = need.volume || need.media;
		let mut media = self.media.take().unwrap_or_default();
		if need.media {
			media.players = rt.block_on(hub.bus.players());
			read.media = true;
		}
		if rescan || (sound && (!(self.was.volume || self.was.media) || every(self.every.volume))) {
			media.volume = volume_get();
			self.volume = media.volume.is_some();
		}
		read.volume = sound;
		self.media = Some(media);
		self.was_any = any;
		self.was = need;
		self.tick = self.tick.wrapping_add(1);

		sample.links = self.links.clone();
		if let Some(p) = &self.profiles {
			sample.power.ppd_profile = Some(p.active.clone());
			sample.power.ppd_profiles = p.available.clone();
		}
		let mut caps = Caps::from_power(&self.power);
		caps.power_profiles = self.profiles.as_ref().map(|p| p.available.clone()).unwrap_or_default();
		caps.units = systemd_runs();
		caps.media = session_bus();
		caps.volume = self.volume;
		Got { media: self.media.clone(), caps, read }
	}
}

/// Whether systemd runs the computer, for the links' units.
fn systemd_runs() -> bool {
	std::path::Path::new("/run/systemd/system").exists()
}

/// Whether there is a session bus to find media players on.
fn session_bus() -> bool {
	std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some()
		|| std::env::var_os("XDG_RUNTIME_DIR").is_some_and(|d| std::path::Path::new(&d).join("bus").exists())
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
	fn programs_off_the_path_are_found_where_they_live() {
		let dir = std::env::temp_dir().join(format!("elamond-bin-{}", data_encoding::HEXLOWER.encode(&crate::store::random_bytes::<6>())));
		std::fs::create_dir_all(dir.join("b")).unwrap();
		std::fs::write(dir.join("b/wpctl"), "").unwrap();
		std::fs::create_dir_all(dir.join("a/wpctl")).unwrap();
		let dirs = [dir.join("missing"), dir.join("a"), dir.join("b")];
		// The first that has it as a file, not a folder of that name.
		assert_eq!(locate("wpctl", &dirs), Some(dir.join("b/wpctl")));
		assert_eq!(locate("pactl", &dirs), None);
		assert!(program_dirs().contains(&"/run/wrappers/bin".into()) && program_dirs().contains(&"/run/current-system/sw/bin".into()));
		// What is on the PATH runs as it is; a path that isn't there isn't looked for elsewhere.
		assert_eq!(run_within(&["echo", "hi"], Duration::from_secs(5)).as_deref(), Some("hi\n"));
		assert_eq!(run_within(&["/nonexistent/echo", "hi"], Duration::from_secs(5)), None);
		std::fs::remove_dir_all(dir).unwrap();
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
		// The background sleep is started and named before anything is written, so a slow start
		// under load leaves nothing half done when the time is up.
		let script = format!("sleep 30 & echo $! > {}; echo started; exec sleep 30", pidfile.display());
		let r = run_action("sh", &["-c".to_string(), script], Duration::from_secs(3)).await.unwrap();
		assert!(r.timed_out);
		assert_eq!(r.exit_code, None);
		assert_eq!(r.output, "started\n");
		let grandchild: i32 = std::fs::read_to_string(&pidfile).unwrap().trim().parse().unwrap();
		std::fs::remove_file(&pidfile).unwrap();
		// SIGKILL is at once; being reaped, by whoever adopted it, may take a moment on a busy machine.
		let deadline = Instant::now() + Duration::from_secs(20);
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
