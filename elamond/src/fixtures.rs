//! Every JSON shape the service sends, with each optional field filled in, kept in
//! elamond/fixtures/. The app's tests read the same files, so a field renamed or dropped on
//! either side fails a build instead of reading as zero on a phone.
//!
//! After a deliberate change to the protocol: `ELAMON_BLESS=1 cargo test`, then commit the
//! files with the change.

use std::path::Path;
use std::sync::Arc;

use serde::Serialize;

use crate::bus::{Player, PowerCaps, Profiles, UnitInfo};
use crate::control::{ActionInfo, ActionResult, ControlInfo, Media, Volume};
use crate::errors::ErrorCode;
use crate::links::{LinkStatus, Listener};
use crate::model::*;
use crate::server::{hello_body, paired_body, render_sample, Frame, Want};

fn check(name: &str, value: &impl Serialize) {
	let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
	let path = dir.join(format!("{name}.json"));
	let text = serde_json::to_string_pretty(value).unwrap() + "\n";
	if std::env::var_os("ELAMON_BLESS").is_some() {
		std::fs::create_dir_all(&dir).unwrap();
		std::fs::write(&path, &text).unwrap();
		return;
	}
	let kept = std::fs::read_to_string(&path).unwrap_or_default();
	assert!(kept == text, "elamond/fixtures/{name}.json is out of date: run `ELAMON_BLESS=1 cargo test` and commit it");
}

fn some(s: &str) -> Option<String> {
	Some(s.to_string())
}

fn mode(width: u32, height: u32, refresh: f32) -> Mode {
	Mode { width, height, refresh }
}

pub fn info() -> Info {
	Info {
		id: "0123456789abcdef0123456789abcdef".into(),
		name: "fixture".into(),
		service_version: "9.9.9",
		service_version: "9.9.9",
		api: crate::consts::protocol::API_VERSION,
		min_app: "9.9.0",
		interval_ms: 1000,
		history_seconds: 600,
		os: "NixOS 26.11".into(),
		kernel: "7.2.10".into(),
		arch: "x86_64".into(),
		cpu_model: "Some CPU".into(),
		cpu_physical_cores: Some(8),
		cpu_logical_cores: 12,
		memory_total: 16_000_000_000,
		boot_time: 1_700_000_000,
		gpus: vec![GpuInfo {
			id: "0000:01:00.0".into(),
			vendor: "nvidia",
			name: "Some GPU".into(),
			driver: "nvidia".into(),
			boot_display: false,
		}],
		capabilities: vec!["processes", "displays", "history", "sensors", "links", "services", "media", "control"],
	}
}

pub fn sample() -> Sample {
	Sample {
		t: 1_700_000_000_123,
		uptime: 3600,
		cpu: Cpu { usage: 12.5, cores: vec![10.0, 15.0], freq: vec![2400, 3100], temp: Some(55.5), load: [0.5, 0.4, 0.3] },
		memory: Memory { total: 16_000_000_000, used: 8_000_000_000, available: 7_000_000_000, swap_total: 4_000_000_000, swap_used: 1_000_000 },
		gpus: vec![Gpu {
			id: "0000:01:00.0".into(),
			state: "active",
			usage: Some(33.0),
			mem_used: Some(1_000_000_000),
			mem_total: Some(4_000_000_000),
			temp: Some(61.0),
			power: Some(25.5),
			freq: Some(1500),
			freq_max: Some(2100),
			mem_freq: Some(6000),
			fan: Some(40),
		}],
		temps: vec![Temp {
			chip: "nvme".into(),
			kind: "storage",
			device: some("Some SSD"),
			label: "Composite".into(),
			value: 41.5,
			high: Some(80.0),
			crit: Some(85.0),
		}],
		fans: vec![Fan { chip: "hp".into(), label: "CPU".into(), rpm: 2500 }],
		battery: Some(Battery {
			name: "BAT0".into(),
			model: some("Some battery"),
			percent: Some(80.0),
			status: "discharging".into(),
			power: Some(12.3),
			energy_now: Some(40.0),
			energy_full: Some(50.0),
			energy_design: Some(52.5),
			cycles: Some(120),
			time_left: Some(7200),
		}),
		power: Power {
			on_ac: Some(false),
			profile: some("balanced"),
			profiles: vec!["quiet".into(), "balanced".into(), "performance".into()],
			governor: some("powersave"),
			epp: some("balance_performance"),
			ppd_profile: some("balanced"),
			ppd_profiles: vec!["power-saver".into(), "balanced".into(), "performance".into()],
		},
		disks: vec![Disk { device: "/dev/nvme0n1p2".into(), mount: "/".into(), fs: "btrfs".into(), total: 500_000_000_000, available: 100_000_000_000, removable: false }],
		disk_io: vec![DiskIo { name: "nvme0n1".into(), model: some("Some SSD"), read: 1024.5, write: 2048.5 }],
		net: vec![Net {
			name: "eno1".into(),
			kind: "wired",
			up: true,
			addrs: vec!["192.0.2.10".into(), "2001:db8::10".into()],
			rx: 1500.5,
			tx: 300.5,
			rx_total: 9_000_000,
			tx_total: 1_000_000,
			speed: Some(1000),
		}],
		displays: vec![Display {
			connector: "DP-1".into(),
			gpu: "0000:00:02.0".into(),
			connected: true,
			active: true,
			dpms: some("On"),
			internal: false,
			vendor: some("Some vendor"),
			model: some("Some monitor"),
			year: Some(2024),
			width_mm: Some(530),
			height_mm: Some(300),
			mode: Some(mode(1920, 1080, 180.0)),
			preferred: Some(mode(1920, 1080, 60.0)),
			max_refresh: Some(180.0),
			position: Some([1920, 0]),
		}],
		links: vec![link()],
		services: vec![link()],
	}
}

fn link() -> LinkStatus {
	LinkStatus {
		id: "config-0123456789ab".into(),
		name: "Some link".into(),
		url: "http://{host}:8080".into(),
		unit: some("some.service"),
		scope: "system".into(),
		editable: false,
		port: Some(8080),
		up: Some(true),
		local_only: false,
		unit_state: some("active"),
	}
}

pub fn media() -> Media {
	Media {
		players: vec![Player {
			id: "org.mpris.MediaPlayer2.some".into(),
			name: "Some player".into(),
			status: "Playing".into(),
			title: some("Title"),
			artist: some("Artist"),
			album: some("Album"),
			length: Some(240_000_000),
			position: Some(60_000_000),
			volume: Some(0.5),
			has_art: true,
			can_next: true,
			can_previous: true,
			can_play: true,
			can_pause: true,
			can_seek: true,
			..Default::default()
		}],
		volume: Some(Volume { level: 0.75, muted: false }),
	}
}

pub fn processes() -> Vec<Process> {
	vec![Process {
		pid: 4242,
		ppid: Some(1),
		name: "some".into(),
		cmd: "/usr/bin/some --flag".into(),
		user: some("alice"),
		cpu: 3.5,
		mem: 123_456_789,
		status: "Sleeping".into(),
		start: 1_700_000_100,
		read: 10.5,
		write: 20.5,
		gpu: Some(5.0),
		gpu_mem: Some(64_000_000),
	}]
}

pub fn history() -> History {
	History {
		t: vec![1_700_000_000_000, 1_700_000_001_000],
		cpu: vec![10.0, 11.0],
		cpu_temp: vec![Some(50.0), None],
		mem: vec![8_000_000_000, 8_100_000_000],
		swap: vec![0, 1_000_000],
		rx: vec![100.5, 200.5],
		tx: vec![10.5, 20.5],
		read: vec![1.5, 2.5],
		write: vec![3.5, 4.5],
		battery_power: vec![Some(12.0), None],
		gpus: vec![GpuHistory { id: "0000:01:00.0".into(), usage: vec![Some(30.0), None], temp: vec![Some(60.0), None] }],
	}
}

#[test]
fn hello() {
	check("hello", &hello_body("0123456789abcdef0123456789abcdef", "fixture"));
}

#[test]
fn paired() {
	check("pair", &paired_body("token", "0123abcd", "0123456789abcdef0123456789abcdef", "fixture"));
}

#[test]
fn info_and_history() {
	check("info", &info());
	check("history", &history());
}

#[test]
fn sample_with_processes() {
	let frame = Frame {
		sample: serde_json::to_string(&sample()).unwrap(),
		media: Some(serde_json::to_string(&media()).unwrap()),
		processes: Some(Arc::new(processes())),
		info: Arc::new(String::new()),
		watched: true,
	};
	let want = Want { procs: 10, sort: None, media: true };
	let line: serde_json::Value = serde_json::from_str(&render_sample(&frame, &want, false)).unwrap();
	check("sample", &line);
}

#[test]
fn errors() {
	let list: Vec<serde_json::Value> =
		ErrorCode::ALL.iter().map(|c| serde_json::json!({ "error": c.as_str(), "status": c.status().as_u16() })).collect();
	check("errors", &list);
}

#[test]
fn control() {
	check(
		"control",
		&ControlInfo {
			enabled: true,
			power: PowerCaps { poweroff: "yes".into(), reboot: "yes".into(), suspend: "challenge".into(), lock: true },
			profiles: Some(Profiles { active: "balanced".into(), available: vec!["power-saver".into(), "balanced".into()] }),
			actions: vec![ActionInfo { id: "0123456789ab".into(), name: "Some action".into(), confirm: true }],
			volume: true,
		},
	);
	check("action-result", &ActionResult { exit_code: Some(0), timed_out: false, output: "done\n".into() });
	check("detected", &vec![Listener { port: 8080, local_only: true, process: some("some") }]);
	check("units", &vec![UnitInfo { name: "some.service".into(), description: "Some service".into(), active: "active".into() }]);
}
