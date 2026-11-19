//! Hyprland, through `hyprctl`: which instance runs (`hyprctl -j instances`, no session
//! environment needed), whether its settings are Lua (`eval` answers `ok`), its outputs
//! (`-j monitors all`, the disabled ones too) and the Lua that turns one on or off or puts its
//! picture to sleep. Every call is a program with arguments, never a shell, killed after
//! `timeout::HYPRCTL`; a Lua string holds only an output's name that passed `fit_name`, and
//! numbers the service read from Hyprland itself.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::consts::{cadence, timeout};

/// What `eval` answers when the Lua ran.
const OK: &str = "ok";

/// An output as `hyprctl -j monitors all` tells of it. Hyprland keeps a disabled one's last mode,
/// position, scale and transform, which is what it is turned on with again.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Output {
	pub name: String,
	#[serde(default)]
	pub disabled: bool,
	/// The picture is on (DPMS); true for a disabled one too.
	#[serde(default = "yes")]
	pub dpms_status: bool,
	#[serde(default)]
	pub width: u32,
	#[serde(default)]
	pub height: u32,
	#[serde(default)]
	pub refresh_rate: f64,
	#[serde(default)]
	pub x: i32,
	#[serde(default)]
	pub y: i32,
	#[serde(default = "one")]
	pub scale: f64,
	#[serde(default)]
	pub transform: u8,
	/// The output it mirrors, `none` for none.
	#[serde(default)]
	pub mirror_of: Option<String>,
	#[serde(default)]
	pub hardware_details: Option<Details>,
}

#[derive(Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Details {
	/// `drm` for a screen, `headless` for an output without one.
	#[serde(default)]
	pub backend: Option<String>,
}

fn yes() -> bool {
	true
}

fn one() -> f64 {
	1.0
}

impl Output {
	/// An output without a screen: a stream's, one made with `hyprctl output create headless`,
	/// Hyprland's fallback.
	pub fn headless(&self) -> bool {
		match self.hardware_details.as_ref().and_then(|d| d.backend.as_deref()) {
			Some(b) => b == "headless",
			None => self.name.starts_with("HEADLESS") || self.name == "FALLBACK",
		}
	}

	/// In use and showing a picture.
	pub fn lit(&self) -> bool {
		!self.disabled && self.dpms_status
	}
}

/// Whether `name` may be acted on: 1 to 64 of `A-Z a-z 0-9 _ . -`, as Hyprland names screens
/// (`DP-1`, `eDP-1`, `HDMI-A-1`). Any other name is shown but never reaches a command.
pub fn fit_name(name: &str) -> bool {
	(1..=64).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}

/// An instance's signature as `--instance` takes it: letters, digits and `_`.
fn fit_signature(s: &str) -> bool {
	(1..=128).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// A number for Lua, as short as it reads back the same, without a fraction when it has none:
/// `1`, `1.25`, `1.3333333333333333`.
fn number(x: f64) -> String {
	format!("{x}")
}

/// The Lua that turns `name` off: the `hl.monitor` a key bound to turning a monitor off runs, so
/// such a key finds it off.
pub fn lua_off(name: &str) -> String {
	format!("hl.monitor({{ output = '{name}', disabled = true }})")
}

/// The Lua that turns `o` on again with what it had: its mode, position, scale and transform (and
/// the output it mirrored); an output never shown yet gets its preferred mode, placed by Hyprland.
pub fn lua_on(o: &Output) -> String {
	let shown = o.width > 0 && o.height > 0 && o.refresh_rate.is_finite() && o.refresh_rate > 0.0;
	let (mode, position) = if shown {
		(format!("{}x{}@{:.3}", o.width, o.height, o.refresh_rate), format!("{}x{}", o.x, o.y))
	} else {
		("preferred".to_string(), "auto".to_string())
	};
	let scale = if o.scale.is_finite() && o.scale > 0.0 { number(clean_scale(o)) } else { "1".into() };
	let mirror = match o.mirror_of.as_deref() {
		Some(m) if m != "none" && fit_name(m) => format!(", mirror = '{m}'"),
		_ => String::new(),
	};
	format!("hl.monitor({{ output = '{}', mode = '{mode}', position = '{position}', scale = {scale}, transform = {}{mirror}, disabled = false }})", o.name, o.transform.min(7))
}

/// The scale Hyprland had, as precise as it takes: `monitors` prints two decimals, so 1.333333 reads
/// as 1.33, which divides neither side; the scale that gives the same whole logical size does.
fn clean_scale(o: &Output) -> f64 {
	if o.width == 0 || o.height == 0 {
		return o.scale;
	}
	// The logical widths within a hundredth of the scale, the nearest whose scale divides the height too.
	let (w, h) = (o.width as f64, o.height as f64);
	let near = |l: &f64| (w / l - o.scale).abs();
	let (lo, hi) = ((w / (o.scale + 0.01)).floor().max(1.0), (w / (o.scale - 0.01).max(0.01)).ceil());
	let mut best: Option<f64> = None;
	let mut l = lo;
	while l <= hi {
		let s = w / l;
		if ((h / s) - (h / s).round()).abs() < 1e-6 && (s - o.scale).abs() < 0.01 && best.is_none_or(|b| near(&l) < near(&b)) {
			best = Some(l);
		}
		l += 1.0;
	}
	best.map_or(o.scale, |l| w / l)
}

/// The Lua that puts `name`'s picture to sleep or wakes it: that output alone.
pub fn lua_dpms(name: &str, on: bool) -> String {
	format!("hl.dispatch(hl.dsp.dpms({{ action = '{}', monitor = '{name}' }}))", if on { "on" } else { "off" })
}

/// A running Hyprland as `hyprctl -j instances` lists it.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Instance {
	pub instance: String,
	#[serde(default)]
	pub time: u64,
	#[serde(default)]
	pub pid: u32,
}

/// `hyprctl` where the service finds it: on its PATH or where programs live.
#[derive(Clone, Debug)]
pub struct Hyprctl {
	pub program: PathBuf,
}

/// How a call went wrong: `hyprctl` couldn't be started, took too long, or said so.
#[derive(Debug, PartialEq)]
pub enum Failed {
	Missing,
	TimedOut,
	Said(String),
}

impl std::fmt::Display for Failed {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Failed::Missing => f.write_str("hyprctl isn't there"),
			Failed::TimedOut => f.write_str("hyprctl took too long"),
			Failed::Said(s) => f.write_str(s),
		}
	}
}

impl Hyprctl {
	pub fn find() -> Option<Hyprctl> {
		crate::control::find_program("hyprctl").map(|program| Hyprctl { program })
	}

	/// The running instances, the newest first.
	pub fn instances(&self) -> Result<Vec<Instance>, Failed> {
		let out = run(&self.program, &["-j", "instances"], timeout::HYPRCTL)?;
		let mut list: Vec<Instance> = serde_json::from_str(&out).map_err(|e| Failed::Said(format!("instances: {e}")))?;
		list.retain(|i| fit_signature(&i.instance));
		list.sort_by_key(|i| std::cmp::Reverse(i.time));
		Ok(list)
	}

	/// Whether the instance's settings are Lua: only then is there `eval` to act with.
	pub fn lua(&self, instance: &str) -> Result<bool, Failed> {
		match run(&self.program, &["--instance", instance, "eval", "return"], timeout::HYPRCTL) {
			Ok(out) => Ok(out.trim() == OK),
			Err(Failed::Said(_)) => Ok(false),
			Err(e) => Err(e),
		}
	}

	pub fn outputs(&self, instance: &str) -> Result<Vec<Output>, Failed> {
		let out = run(&self.program, &["--instance", instance, "-j", "monitors", "all"], timeout::HYPRCTL)?;
		serde_json::from_str(&out).map_err(|e| Failed::Said(format!("monitors: {e}")))
	}

	/// Runs `lua`; `Err` with what Hyprland said when it didn't answer `ok`.
	pub fn eval(&self, instance: &str, lua: &str) -> Result<(), Failed> {
		let out = run(&self.program, &["--instance", instance, "eval", lua], timeout::HYPRCTL)?;
		if out.trim() == OK { Ok(()) } else { Err(Failed::Said(out.trim().chars().take(200).collect())) }
	}
}

/// `program` with `args`, its output if it ended well within `limit`, else why not; killed with
/// what it started once `limit` passes. The output has to fit in a pipe (64 KiB).
pub fn run(program: &Path, args: &[&str], limit: Duration) -> Result<String, Failed> {
	use std::os::unix::process::CommandExt;
	let mut child = std::process::Command::new(program)
		.args(args)
		.env("LC_ALL", "C")
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.process_group(0)
		.spawn()
		.map_err(|_| Failed::Missing)?;
	let deadline = Instant::now() + limit;
	let status = loop {
		match child.try_wait() {
			Ok(Some(s)) => break s,
			Ok(None) if Instant::now() < deadline => std::thread::sleep(cadence::COMMAND_POLL),
			_ => {
				drop(crate::control::Group(Some(child.id() as i32)));
				let _ = child.wait();
				return Err(Failed::TimedOut);
			}
		}
	};
	let mut out = String::new();
	if let Some(mut o) = child.stdout.take() {
		let _ = o.read_to_string(&mut out);
	}
	if status.success() {
		return Ok(out);
	}
	let mut err = String::new();
	if let Some(mut e) = child.stderr.take() {
		let _ = e.read_to_string(&mut err);
	}
	let said = if out.trim().is_empty() { err } else { out };
	let first = said.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("failed");
	Err(Failed::Said(first.chars().take(200).collect()))
}

#[cfg(test)]
mod tests {
	use super::*;

	fn output(json: &str) -> Output {
		serde_json::from_str(json).unwrap()
	}

	#[test]
	fn outputs_as_hyprland_lists_them() {
		let o = output(
			r#"{ "id": 1, "name": "DP-1", "width": 1920, "height": 1080, "refreshRate": 179.99899, "x": 1920, "y": 0, "scale": 1,
			     "transform": 0, "dpmsStatus": false, "disabled": false, "mirrorOf": "none", "hardwareDetails": { "backend": "drm" } }"#,
		);
		assert!(!o.headless() && !o.lit());
		assert_eq!(lua_on(&o), "hl.monitor({ output = 'DP-1', mode = '1920x1080@179.999', position = '1920x0', scale = 1, transform = 0, disabled = false })");
		assert_eq!(lua_off("DP-1"), "hl.monitor({ output = 'DP-1', disabled = true })");
		assert_eq!(lua_dpms("DP-1", false), "hl.dispatch(hl.dsp.dpms({ action = 'off', monitor = 'DP-1' }))");
		let h = output(r#"{ "name": "stream", "hardwareDetails": { "backend": "headless" }, "scale": 1.25, "mirrorOf": "eDP-1", "transform": 9 }"#);
		assert!(h.headless() && h.lit());
		assert_eq!(lua_on(&h), "hl.monitor({ output = 'stream', mode = 'preferred', position = 'auto', scale = 1.25, transform = 7, mirror = 'eDP-1', disabled = false })");
		// Two decimals of a scale that divides both sides only whole: put back as it was.
		let odd = output(r#"{ "name": "DP-2", "width": 2560, "height": 1440, "refreshRate": 60, "scale": 1.33 }"#);
		assert!(lua_on(&odd).contains("scale = 1.3333333333333333,"), "{}", lua_on(&odd));
		let plain = output(r#"{ "name": "DP-2", "width": 1920, "height": 1080, "refreshRate": 60, "scale": 1.25 }"#);
		assert!(lua_on(&plain).contains("scale = 1.25,"), "{}", lua_on(&plain));
		assert!(output(r#"{ "name": "HEADLESS-2" }"#).headless());
		assert!(!output(r#"{ "name": "HDMI-A-1", "disabled": true }"#).lit());
	}

	#[test]
	fn names_that_never_reach_a_command() {
		for n in ["DP-1", "eDP-1", "HDMI-A-1", "HEADLESS-2", "Virtual.1_a"] {
			assert!(fit_name(n), "{n}");
		}
		for n in ["", "a'b", "a b", "x)", "]]", "a\\b", "é", "a\nb", &"x".repeat(65)] {
			assert!(!fit_name(n), "{n:?}");
		}
		let weird = output(r#"{ "name": "x", "mirrorOf": "a'); os.exit() --" }"#);
		assert!(!lua_on(&weird).contains("os.exit"));
		assert!(fit_signature("0123abcd_1700000000_42") && !fit_signature("../x") && !fit_signature(""));
	}

	#[test]
	fn a_command_that_hangs_is_killed_in_time() {
		let started = Instant::now();
		assert_eq!(run(Path::new("sleep"), &["10"], Duration::from_millis(200)), Err(Failed::TimedOut));
		assert!(started.elapsed() < Duration::from_secs(2));
		assert_eq!(run(Path::new("/nonexistent/hyprctl"), &[], Duration::from_secs(1)), Err(Failed::Missing));
		assert_eq!(run(Path::new("sh"), &["-c", "echo 'error: no'; exit 7"], Duration::from_secs(2)), Err(Failed::Said("error: no".into())));
		assert_eq!(run(Path::new("sh"), &["-c", "echo ok"], Duration::from_secs(2)), Ok("ok\n".into()));
	}
}
