//! A monitor's brightness: a laptop's panel through its backlight (read in sysfs, set through
//! logind, which lets the session's user do it without root), an external monitor through DDC/CI
//! with the system's `ddcutil` (read and set over the monitor's I²C bus, which its connector's
//! `ddc` link in sysfs names). Neither adds a crate; `ddcutil` runs without a shell, one call at a
//! time, killed after `timeout::DDCUTIL`.

use std::path::{Path, PathBuf};

use super::hypr::{run, Failed};
use crate::consts::timeout;

/// How a monitor's brightness is reached.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Way {
	/// The backlight device of a laptop's panel (`intel_backlight`).
	Backlight(String),
	/// The I²C bus of a monitor's DDC/CI.
	Ddc(u32),
}

/// A brightness as read: the device's own number and its most.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Level {
	pub raw: u32,
	pub max: u32,
}

impl Level {
	pub fn percent(self) -> f64 {
		if self.max == 0 { 0.0 } else { (self.raw as f64 * 100.0 / self.max as f64).clamp(0.0, 100.0) }
	}

	/// The device's number for `percent` of its most.
	pub fn raw_of(self, percent: f64) -> u32 {
		((percent.clamp(0.0, 100.0) * self.max as f64 / 100.0).round() as u32).min(self.max)
	}
}

/// The folder in `<sys>/class/drm` of the connector Hyprland names `output` (`card2-eDP-1`).
pub fn connector_dir(sys: &Path, output: &str) -> Option<PathBuf> {
	let drm = sys.join("class/drm");
	let mut found: Vec<PathBuf> = std::fs::read_dir(&drm)
		.ok()?
		.flatten()
		.filter(|e| {
			let n = e.file_name();
			let n = n.to_string_lossy();
			n.strip_prefix("card").and_then(|r| r.split_once('-')).is_some_and(|(card, conn)| !card.is_empty() && card.bytes().all(|b| b.is_ascii_digit()) && conn == output)
		})
		.map(|e| e.path())
		.collect();
	found.sort();
	found.into_iter().next()
}

/// How the brightness of the connector in `dir` is reached: a backlight device in its folder (a
/// panel's), else its DDC bus.
pub fn way_of(sys: &Path, dir: &Path) -> Option<Way> {
	let backlights = sys.join("class/backlight");
	if let Ok(entries) = std::fs::read_dir(dir) {
		let mut names: Vec<String> = entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| backlights.join(n).exists()).collect();
		names.sort();
		if let Some(n) = names.into_iter().next() {
			return Some(Way::Backlight(n));
		}
	}
	let link = std::fs::read_link(dir.join("ddc")).ok()?;
	let bus = link.file_name()?.to_str()?.strip_prefix("i2c-")?.parse().ok()?;
	Some(Way::Ddc(bus))
}

/// A backlight's brightness as sysfs has it: what it is set to (which a panel that is off keeps for
/// when it is on again), and its most.
pub fn backlight(sys: &Path, device: &str) -> Option<Level> {
	let dir = sys.join("class/backlight").join(device);
	let read = |f: &str| std::fs::read_to_string(dir.join(f)).ok()?.trim().parse::<u32>().ok();
	let max = read("max_brightness").filter(|m| *m > 0)?;
	Some(Level { raw: read("brightness")?.min(max), max })
}

/// Whether the service's user may read and write the I²C device of `bus` in `dev` (`/dev`).
pub fn i2c_open(dev: &Path, bus: u32) -> bool {
	use std::os::unix::ffi::OsStrExt;
	let path = std::ffi::CString::new(dev.join(format!("i2c-{bus}")).as_os_str().as_bytes()).unwrap_or_default();
	// SAFETY: access(2) reads a NUL-terminated path and has no other effect.
	unsafe { libc::access(path.as_ptr(), libc::R_OK | libc::W_OK) == 0 }
}

/// `ddcutil getvcp 10 --brief`'s answer: `VCP 10 C <now> <most>`.
pub fn parse_vcp(out: &str) -> Option<Level> {
	let line = out.lines().find(|l| l.starts_with("VCP 10 "))?;
	let parts: Vec<&str> = line.split_whitespace().collect();
	match parts.as_slice() {
		["VCP", "10", "C", now, most] => {
			let max: u32 = most.parse().ok().filter(|m| *m > 0)?;
			Some(Level { raw: now.parse::<u32>().ok()?.min(max), max })
		}
		_ => None,
	}
}

/// The brightness of the monitor on `bus`, through `ddcutil` at `program`.
pub fn ddc_get(program: &Path, bus: u32) -> Result<Level, String> {
	let bus = bus.to_string();
	let out = run(program, &["--bus", &bus, "getvcp", "10", "--brief"], timeout::DDCUTIL).map_err(|e| failed(&e))?;
	parse_vcp(&out).ok_or_else(|| "the monitor didn't say its brightness".to_string())
}

/// Sets the brightness of the monitor on `bus` to `raw`, unverified (the next reading tells).
pub fn ddc_set(program: &Path, bus: u32, raw: u32) -> Result<(), String> {
	let (bus, raw) = (bus.to_string(), raw.to_string());
	run(program, &["--bus", &bus, "setvcp", "10", &raw, "--noverify"], timeout::DDCUTIL).map(|_| ()).map_err(|e| failed(&e))
}

fn failed(e: &Failed) -> String {
	match e {
		Failed::Missing => "ddcutil isn't there".into(),
		Failed::TimedOut => crate::errors::reason::TIMED_OUT.into(),
		Failed::Said(s) => s.clone(),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn levels_in_percent_and_back() {
		let l = Level { raw: 112800, max: 120000 };
		assert_eq!(l.percent(), 94.0);
		assert_eq!(l.raw_of(50.0), 60000);
		assert_eq!(l.raw_of(140.0), 120000);
		assert_eq!(Level { raw: 3, max: 0 }.percent(), 0.0);
		assert_eq!(parse_vcp("VCP 10 C 50 100\n"), Some(Level { raw: 50, max: 100 }));
		assert_eq!(parse_vcp("VCP 10 C 120 100"), Some(Level { raw: 100, max: 100 }));
		for bad in ["", "VCP 10 ERR", "VCP 10 C x 100", "VCP 10 C 5 0", "VCP 12 C 5 100"] {
			assert_eq!(parse_vcp(bad), None, "{bad}");
		}
	}

	#[test]
	fn a_connector_and_how_its_brightness_is_reached() {
		let sys = std::env::temp_dir().join(format!("elamond-light-{}", std::process::id()));
		let _ = std::fs::remove_dir_all(&sys);
		let edp = sys.join("class/drm/card2-eDP-1");
		let dp = sys.join("class/drm/card2-DP-1");
		std::fs::create_dir_all(edp.join("intel_backlight")).unwrap();
		std::fs::create_dir_all(&dp).unwrap();
		std::fs::create_dir_all(sys.join("class/drm/card2-DP-10")).unwrap();
		std::fs::create_dir_all(sys.join("class/backlight/intel_backlight")).unwrap();
		std::os::unix::fs::symlink("../../i2c-14", dp.join("ddc")).unwrap();
		std::fs::write(sys.join("class/backlight/intel_backlight/brightness"), "112800\n").unwrap();
		std::fs::write(sys.join("class/backlight/intel_backlight/max_brightness"), "120000\n").unwrap();
		assert_eq!(connector_dir(&sys, "DP-1"), Some(dp.clone()));
		assert_eq!(connector_dir(&sys, "HDMI-A-1"), None);
		assert_eq!(way_of(&sys, &edp), Some(Way::Backlight("intel_backlight".into())));
		assert_eq!(way_of(&sys, &dp), Some(Way::Ddc(14)));
		assert_eq!(way_of(&sys, &sys.join("class/drm/card2-DP-10")), None);
		assert_eq!(backlight(&sys, "intel_backlight"), Some(Level { raw: 112800, max: 120000 }));
		assert_eq!(backlight(&sys, "other"), None);
		std::fs::remove_dir_all(&sys).unwrap();
	}
}
