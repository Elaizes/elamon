//! Small readers for /sys and /proc files: hwmon sensors, batteries, power settings.

use std::fs;
use std::path::{Path, PathBuf};

use crate::model::{r1, Battery, Fan, Power, Temp};

pub fn read(path: impl AsRef<Path>) -> Option<String> {
	let s = fs::read_to_string(path).ok()?;
	let s = s.trim();
	(!s.is_empty()).then(|| s.to_string())
}

pub fn read_num<T: std::str::FromStr>(path: impl AsRef<Path>) -> Option<T> {
	read(path)?.parse().ok()
}

/// Last component of a symlink's target, like the driver name behind device/driver.
pub fn link_name(path: impl AsRef<Path>) -> Option<String> {
	Some(fs::read_link(path).ok()?.file_name()?.to_string_lossy().into_owned())
}

pub fn dir_entries(path: impl AsRef<Path>) -> Vec<(String, PathBuf)> {
	let Ok(rd) = fs::read_dir(path) else { return Vec::new() };
	let mut v: Vec<_> = rd
		.flatten()
		.map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
		.collect();
	v.sort_by(|a, b| natural_cmp(&a.0, &b.0));
	v
}

/// "temp10" after "temp2", "card10" after "card2".
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
	let split = |s: &str| {
		let i = s.trim_end_matches(|c: char| c.is_ascii_digit()).len();
		(s[..i].to_string(), s[i..].parse::<u64>().unwrap_or(0))
	};
	split(a).cmp(&split(b)).then_with(|| a.cmp(b))
}

/// Runtime power state of a PCI device: "active", "suspended", ... Reading it never wakes the device.
pub fn runtime_status(device: &Path) -> Option<String> {
	read(device.join("power/runtime_status"))
}

#[derive(Clone, Default)]
pub struct Sensors {
	pub temps: Vec<Temp>,
	pub fans: Vec<Fan>,
	pub cpu_temp: Option<f32>,
}

/// Where each hwmon reading lives, found once: labels and limits don't change, and every
/// sysfs read costs something (some are ACPI calls or commands sent to a disk).
pub struct SensorSet {
	temps: Vec<(PathBuf, Option<PathBuf>, Temp)>,
	fans: Vec<(PathBuf, Fan)>,
}

impl SensorSet {
	pub fn scan() -> Self {
		let mut temps = Vec::new();
		let mut fans = Vec::new();
		for (_, hw) in dir_entries("/sys/class/hwmon") {
			let Some(chip) = read(hw.join("name")) else { continue };
			let device = fs::canonicalize(hw.join("device")).ok();
			let kind = chip_kind(&chip, device.as_deref());
			let model = device.as_ref().and_then(|d| read(d.join("model")));
			for (file, path) in dir_entries(&hw) {
				if let Some(n) = file.strip_prefix("temp").and_then(|s| s.strip_suffix("_input")) {
					let label = read(hw.join(format!("temp{n}_label"))).unwrap_or_else(|| format!("temp{n}"));
					let limit = |suffix: &str| {
						read_num::<i64>(hw.join(format!("temp{n}_{suffix}")))
							.map(|m| m as f32 / 1000.0)
							.filter(|v| *v > 0.0 && *v < 200.0)
					};
					let temp = Temp {
						chip: chip.clone(),
						kind,
						device: model.clone(),
						label,
						value: 0.0,
						high: limit("max"),
						crit: limit("crit"),
					};
					temps.push((path, device.clone(), temp));
				} else if let Some(n) = file.strip_prefix("fan").and_then(|s| s.strip_suffix("_input")) {
					let label = read(hw.join(format!("fan{n}_label"))).unwrap_or_else(|| format!("fan{n}"));
					fans.push((path, Fan { chip: chip.clone(), label, rpm: 0 }));
				}
			}
		}
		SensorSet { temps, fans }
	}

	/// GPUs read their own hwmon chips (amdgpu); those are skipped while that GPU sleeps,
	/// since reading them would wake it.
	pub fn read(&self, sleeping_devices: &[PathBuf]) -> Sensors {
		let mut temps = Vec::new();
		let mut cpu_temp = None;
		let mut cpu_fallback = None;
		for (path, device, t) in &self.temps {
			if let Some(dev) = device
				&& sleeping_devices.iter().any(|s| dev.starts_with(s))
			{
				continue;
			}
			let Some(milli) = read_num::<i64>(path) else { continue };
			let value = milli as f32 / 1000.0;
			// Disconnected thermistors read as -273 or 0; nothing on a running machine is that cold.
			if value <= 0.0 || value > 200.0 {
				continue;
			}
			if t.kind == "cpu" {
				let l = t.label.to_ascii_lowercase();
				if l.starts_with("package") || l == "tctl" || l == "tdie" || l == "cpu" {
					cpu_temp.get_or_insert(value);
				}
				cpu_fallback.get_or_insert(value);
			}
			temps.push(Temp { value: r1(value), ..t.clone() });
		}
		let fans = self
			.fans
			.iter()
			.filter_map(|(path, f)| Some(Fan { rpm: read_num(path)?, ..f.clone() }))
			.collect();
		// Without a CPU hwmon chip, ACPI's thermal zone tracks the CPU on most laptops.
		let cpu_temp = cpu_temp.or(cpu_fallback).or_else(|| {
			temps.iter().find(|t| t.chip == "acpitz" || t.chip == "x86_pkg_temp").map(|t| t.value)
		});
		Sensors { temps, fans, cpu_temp: cpu_temp.map(r1) }
	}
}

fn chip_kind(chip: &str, device: Option<&Path>) -> &'static str {
	match chip {
		"coretemp" | "k10temp" | "zenpower" | "cpu_thermal" | "x86_pkg_temp" | "cpu" => "cpu",
		"amdgpu" | "radeon" | "nouveau" | "i915" | "xe" => "gpu",
		"nvme" | "drivetemp" => "storage",
		"acpitz" | "pch_cannonlake" | "pch_skylake" | "pch_cometlake" | "pch_tigerlake" => "board",
		c if c.starts_with("BAT") => "battery",
		c if c.starts_with("iwlwifi") || c.starts_with("mt7") || c.starts_with("ath") || c.starts_with("rtw") => {
			"wireless"
		}
		c if c.starts_with("pch_") => "board",
		_ => {
			let class = device.and_then(|d| read(d.join("class")));
			match class.as_deref() {
				Some(c) if c.starts_with("0x03") => "gpu",
				Some(c) if c.starts_with("0x01") => "storage",
				Some(c) if c.starts_with("0x0280") => "wireless",
				_ => "other",
			}
		}
	}
}

pub fn battery() -> Option<Battery> {
	for (name, dir) in dir_entries("/sys/class/power_supply") {
		if read(dir.join("type")).as_deref() != Some("Battery") || read(dir.join("scope")).as_deref() == Some("Device") {
			continue;
		}
		if read_num::<u8>(dir.join("present")) == Some(0) {
			continue;
		}
		let num = |f: &str| read_num::<f64>(dir.join(f));
		let volts = num("voltage_now").map(|v| v / 1e6);
		// Firmware reports either energy (µWh) or charge (µAh); charge × voltage gives energy.
		let energy = |e: &str, c: &str| num(e).map(|v| v / 1e6).or_else(|| Some(num(c)? / 1e6 * volts?));
		let energy_now = energy("energy_now", "charge_now");
		let energy_full = energy("energy_full", "charge_full");
		let energy_design = energy("energy_full_design", "charge_full_design");
		let power = num("power_now")
			.map(|p| p / 1e6)
			.or_else(|| Some(num("current_now")?.abs() / 1e6 * volts?))
			.map(|p| p.abs());
		let status = read(dir.join("status")).unwrap_or_default().to_ascii_lowercase().replace(' ', "_");
		let percent = num("capacity").or_else(|| Some(energy_now? / energy_full? * 100.0));
		let time_left = match (status.as_str(), power, energy_now, energy_full) {
			(_, Some(p), _, _) if p < 0.1 => None,
			("discharging", Some(p), Some(now), _) => Some(now / p * 3600.0),
			("charging", Some(p), Some(now), Some(full)) if full > now => Some((full - now) / p * 3600.0),
			_ => None,
		};
		return Some(Battery {
			name,
			model: read(dir.join("model_name")),
			percent: percent.map(|p| r1(p as f32)),
			status: if status.is_empty() { "unknown".into() } else { status },
			power: power.map(|p| r1(p as f32)),
			energy_now: energy_now.map(|e| r1(e as f32)),
			energy_full: energy_full.map(|e| r1(e as f32)),
			energy_design: energy_design.map(|e| r1(e as f32)),
			cycles: read_num::<u32>(dir.join("cycle_count")).filter(|c| *c > 0),
			time_left: time_left.map(|s| s as u64),
		});
	}
	None
}

pub fn power() -> Power {
	let mut on_ac = None;
	for (_, dir) in dir_entries("/sys/class/power_supply") {
		if read(dir.join("type")).as_deref() == Some("Mains") {
			let online = read_num::<u8>(dir.join("online")) == Some(1);
			on_ac = Some(on_ac.unwrap_or(false) || online);
		}
	}
	let cpufreq = Path::new("/sys/devices/system/cpu/cpu0/cpufreq");
	Power {
		on_ac,
		profile: read("/sys/firmware/acpi/platform_profile"),
		profiles: read("/sys/firmware/acpi/platform_profile_choices")
			.map(|s| s.split_whitespace().map(String::from).collect())
			.unwrap_or_default(),
		governor: read(cpufreq.join("scaling_governor")),
		epp: read(cpufreq.join("energy_performance_preference")),
	}
}

/// Pretty name of the distribution from os-release.
pub fn os_name() -> Option<String> {
	let text = fs::read_to_string("/etc/os-release").or_else(|_| fs::read_to_string("/usr/lib/os-release")).ok()?;
	let get = |key: &str| {
		text.lines()
			.find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
			.map(|v| v.trim_matches('"').to_string())
	};
	get("PRETTY_NAME").or_else(|| get("NAME"))
}
