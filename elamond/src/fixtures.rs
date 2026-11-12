//! Every JSON shape the service sends, with each optional field filled in, kept in
//! elamond/fixtures/, and the requests clients send checked against what the service reads. The
//! app's tests read the same files, so a field missing or under another name on either side fails
//! a build rather than reading as zero on a phone.
//!
//! After a deliberate change to the protocol: `ELAMON_BLESS=1 cargo test`, then commit the
//! files with the change.

use std::path::Path;

use serde::Serialize;

use crate::access::Role;
use crate::activity::Entry;
use crate::bus::Player;
use crate::control::{ActionResult, Media, Volume};
use crate::errors::ErrorCode;
use crate::links::LinkStatus;
use crate::model::*;
use crate::server::{features_of, hello_body, paired_body};

fn check(name: &str, value: &impl Serialize) {
	write_or_compare(name, serde_json::to_string_pretty(value).unwrap() + "\n");
}

/// As `check`, written the way people write JSON by hand: what fits on a line of 140 columns
/// stays on one, the keys in the order the service sends them.
fn check_compact(name: &str, value: &crate::ui::J) {
	let mut text = String::new();
	compact(value, 0, 0, &mut text);
	write_or_compare(name, text + "\n");
}

fn write_or_compare(name: &str, text: String) {
	let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
	let path = dir.join(format!("{name}.json"));
	if std::env::var_os("ELAMON_BLESS").is_some() {
		std::fs::create_dir_all(&dir).unwrap();
		std::fs::write(&path, &text).unwrap();
		return;
	}
	let kept = std::fs::read_to_string(&path).unwrap_or_default();
	assert!(kept == text, "elamond/fixtures/{name}.json is out of date: run `ELAMON_BLESS=1 cargo test` and commit it");
}

/// `v` on one line: `{ "a": 1, "b": [1, 2] }`.
fn one_line(v: &crate::ui::J) -> String {
	use crate::ui::J;
	match v {
		J::Arr(a) if a.is_empty() => "[]".into(),
		J::Arr(a) => format!("[{}]", a.iter().map(one_line).collect::<Vec<_>>().join(", ")),
		J::Obj(o) if o.is_empty() => "{}".into(),
		J::Obj(o) => {
			let parts: Vec<String> = o.keys().map(|k| format!("{}: {}", serde_json::to_string(k).unwrap(), one_line(o.get(k).unwrap()))).collect();
			format!("{{ {} }}", parts.join(", "))
		}
		other => serde_json::to_string(other).unwrap(),
	}
}

/// `v` at `indent`, after `used` columns of its line: on one line if it fits, else one entry to a
/// line. The outermost is always spread.
fn compact(v: &crate::ui::J, indent: usize, used: usize, out: &mut String) {
	use crate::ui::J;
	let line = one_line(v);
	if indent > 0 && used + line.chars().count() <= 140 {
		out.push_str(&line);
		return;
	}
	let pad = "  ".repeat(indent + 1);
	match v {
		J::Arr(a) if !a.is_empty() => {
			out.push_str("[\n");
			for (i, e) in a.iter().enumerate() {
				out.push_str(&pad);
				compact(e, indent + 1, pad.len(), out);
				out.push_str(if i + 1 < a.len() { ",\n" } else { "\n" });
			}
			out.push_str(&"  ".repeat(indent));
			out.push(']');
		}
		J::Obj(o) if !o.is_empty() => {
			out.push_str("{\n");
			let keys: Vec<&str> = o.keys().collect();
			for (i, k) in keys.iter().enumerate() {
				let key = format!("{}: ", serde_json::to_string(k).unwrap());
				out.push_str(&pad);
				out.push_str(&key);
				compact(o.get(k).unwrap(), indent + 1, pad.len() + key.chars().count(), out);
				out.push_str(if i + 1 < keys.len() { ",\n" } else { "\n" });
			}
			out.push_str(&"  ".repeat(indent));
			out.push('}');
		}
		_ => out.push_str(&line),
	}
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
	}
}

pub fn sample() -> Sample {
	Sample {
		t: 1_700_000_000_123,
		uptime: 3600,
		cpu: Cpu { usage: 12.5, cores: vec![10.0, 15.0], freq: vec![2400, 3100], temp: Some(55.5), load: [0.75, 0.5, 0.25] },
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
	}
}

fn link() -> LinkStatus {
	LinkStatus {
		id: "config-0123456789ab".into(),
		name: "Some link".into(),
		url: "http://{host}:8080".into(),
		unit: some("some.service"),
		scope: "system".into(),
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

#[test]
fn hello() {
	check_compact("hello", &crate::ui::J::from(hello_body("0123456789abcdef0123456789abcdef", "fixture")));
}

/// Where the fixtures' computer is: its addresses on the local network and Tailscale, then names.
pub fn endpoints() -> Vec<String> {
	["192.0.2.10", "100.64.0.7", "desk.example.net", "desk.tail0000.ts.net"].map(String::from).to_vec()
}

/// What the fixtures' computer offers, as `ui`'s `service.features` says.
const FEATURES: [&str; 6] = ["displays", "media", "power", "power_profiles", "processes", "units"];

#[test]
fn paired() {
	let body = paired_body("token", "0123abcd", "0123456789abcdef0123456789abcdef", "fixture", Role::Admin, "galaxy-s25", &endpoints());
	check_compact("pair", &crate::ui::J::from(body));
}

#[test]
fn errors() {
	let list: Vec<serde_json::Value> =
		ErrorCode::ALL.iter().map(|c| serde_json::json!({ "error": c.as_str(), "status": c.status().as_u16() })).collect();
	check("errors", &list);
}

/// What an action the settings add answers.
#[test]
fn action_result() {
	check("action-result", &ActionResult { ok: true, exit_code: Some(0), timed_out: false, output: "done\n".into() });
}

/// The activity log, newest first.
pub fn log() -> Vec<Entry> {
	vec![
		Entry {
			t: 1_700_000_060_000,
			phone: "Some phone".into(),
			client_id: "0123abcd".into(),
			what: "action Some action".into(),
			result: "ok".into(),
			message: some("exit 0"),
			family: some("89ab0123"),
		},
		Entry {
			t: 1_700_000_000_000,
			phone: "Other phone".into(),
			client_id: "4567cdef".into(),
			what: "power reboot".into(),
			result: "not_allowed".into(),
			message: None,
			family: None,
		},
	]
}

// ---- API 2: ui and the stream's data lines ---------------------------------------------------

/// The fixture sample with the links of `config`, each answering, its unit active.
pub fn sample_for(config: &crate::settings::Config) -> Sample {
	let mut s = sample();
	s.links = config
		.links
		.iter()
		.map(|(id, l)| LinkStatus {
			id: id.clone(),
			name: l.name.clone(),
			url: l.url.clone(),
			unit: l.unit.clone(),
			scope: l.scope().into(),
			port: crate::links::port_of(&l.url),
			up: Some(true),
			local_only: false,
			unit_state: l.unit.as_ref().map(|_| "active".into()),
		})
		.collect();
	s
}

/// The values of the fixture readings for `config`: the sample, the media, the processes and the
/// activity log.
pub fn values(config: &crate::settings::Config) -> crate::values::Values {
	let (sample, info, media, procs, log) = (sample_for(config), info(), media(), processes(), log());
	crate::values::read(&crate::values::Reading {
		sample: &sample,
		info: &info,
		media: Some(&media),
		processes: Some(&procs),
		activity: &log,
		actions: &config.actions,
	})
}

/// The settings ui.json and stream-*.json describe.
fn ui_source() -> crate::settings::Config {
	crate::settings::read_sources(&[crate::settings::Source::new("ui-source.toml", include_str!("../fixtures/ui-source.toml"))]).unwrap()
}

/// `ui` for a client of `role` and `profile` that draws every widget type.
fn ui_for(config: &crate::settings::Config, values: &crate::values::Values, role: Role, profile: &str) -> crate::ui::Ui {
	let info = info();
	let caps = crate::actions::Caps::all(&values.power_profiles);
	crate::ui::build(&crate::ui::Input {
		config,
		values,
		caps: &caps,
		service: crate::ui::Service {
			id: info.id,
			name: info.name,
			version: info.service_version.into(),
			interval_ms: info.interval_ms,
			history_seconds: info.history_seconds,
			features: features_of(&FEATURES, config.control),
			endpoints: endpoints(),
		},
		control: crate::ui::Control { enabled: config.control, off_reason: (!config.control).then_some("settings") },
		role,
		profile,
		kind: "android",
		draws: None,
	})
}

/// The history ring the fixtures' graphs are drawn from: two points, a value missing at the second.
pub fn ring() -> crate::history::Ring {
	let mut ring = crate::history::Ring::new(600);
	let points: [(&str, [Option<f64>; 2]); 11] = [
		("cpu.usage", [Some(10.0), Some(11.0)]),
		("cpu.temp", [Some(50.0), None]),
		("mem.used", [Some(8_000_000_000.0), Some(8_100_000_000.0)]),
		("swap.used", [Some(0.0), Some(1_000_000.0)]),
		("net.rx", [Some(100.5), Some(200.5)]),
		("net.tx", [Some(10.5), Some(20.5)]),
		("disk.read", [Some(1.5), Some(2.5)]),
		("disk.write", [Some(3.5), Some(4.5)]),
		("battery.power", [Some(12.0), None]),
		("gpu.nvidia.usage", [Some(30.0), None]),
		("gpu.nvidia.temp", [Some(60.0), None]),
	];
	// The newest at the reading the data lines carry: a graph's history ends at the line's `t`.
	let t = sample().t;
	for (i, t) in [t - 1000, t].into_iter().enumerate() {
		ring.push_points(t, points.iter().map(|(a, x)| (a.to_string(), x[i])));
	}
	ring
}

#[test]
fn ui() {
	let config = ui_source();
	let values = values(&config);
	let ui = ui_for(&config, &values, Role::Admin, "owner");
	check_compact("ui", &ui.doc);
}

/// The first `data` line of the dashboard `home` (with its history), and a later one of `detail`.
#[test]
fn stream_data() {
	let config = ui_source();
	let values = values(&config);
	let ui = ui_for(&config, &values, Role::Admin, "owner");
	let sorts = std::collections::HashMap::new();
	let s = crate::states::States { view: crate::values::View::new(&values, Role::Admin, ui.units), control: true, sorts: &sorts, read: crate::ui::Sources::ALL };
	let header = ui.header.as_deref().and_then(|h| ui.dashboard(h));
	let home = ui.dashboard("home").unwrap();
	let t = sample().t;
	let history = crate::history::json(home, &ring(), ui.units, t);
	check_compact("stream-data", &crate::ui::J::Obj(crate::states::data(t, home, header, &s, Some(history))));
	let detail = ui.dashboard("detail").unwrap();
	check_compact("stream-data-detail", &crate::ui::J::Obj(crate::states::data(t + 1000, detail, header, &s, None)));
}

/// The `open` line of a stream with `ui.json`'s version, and the `ui` line a stream sends when its
/// client's `ui` changes: here its role, to `operate`.
#[test]
fn stream_open_and_ui() {
	let config = ui_source();
	let values = values(&config);
	let ui = ui_for(&config, &values, Role::Admin, "owner");
	check_compact("stream-open", &crate::ui::J::Obj(crate::stream::open_line("s1a2b3c4", &ui.version)));
	let operate = ui_for(&config, &values, Role::Operate, "owner");
	assert_ne!(operate.version, ui.version);
	check_compact("stream-ui", &crate::ui::J::Obj(crate::stream::ui_line(&operate.version, &endpoints())));
}

/// The requests the app sends read as the service reads them, and ask for what `ui.json` has.
#[test]
fn requests() {
	let read = |name: &str| std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(name)).unwrap();
	let config = ui_source();
	let values = values(&config);
	let ui = ui_for(&config, &values, Role::Admin, "owner");
	let change: crate::stream::Change = serde_json::from_slice(&read("subscribe.json")).unwrap();
	assert!(ui.dashboard(change.dashboard.as_deref().unwrap()).is_some());
	assert_eq!(change.interval, Some(2000));
	for (table, field) in &change.sort {
		assert!(crate::stream::sorts_by(&ui, table, field.as_deref()), "{table} by {field:?}");
	}
	let pair: crate::server::PairRequest = serde_json::from_slice(&read("pair-request.json")).unwrap();
	assert_eq!((pair.code.as_str(), pair.device.as_str()), ("123456", "Galaxy S25"));
	assert_eq!(pair.previous, Some(crate::server::Previous { client_id: "0123abcd".into(), token: "token".into() }));
	let request: crate::control::ActionRequest = serde_json::from_slice(&read("action-request.json")).unwrap();
	let mode = ui.actions.iter().find(|a| a.id == "mode").unwrap();
	let params = crate::actions::check_params(mode, &request.params).unwrap();
	assert_eq!(crate::actions::command(&config.actions["mode"], &params), ["modectl", "server"]);
}

/// The built-in dashboards and profile, for an admin, with the fixture readings: what the app
/// checks it shows as much as 0.5 did.
#[test]
fn ui_default() {
	// As the starter file has them, written as elamond.toml.
	let config = crate::settings::read_sources(&[crate::settings::Source::new("elamond.toml", crate::settings::DEFAULTS)]).unwrap();
	let values = values(&config);
	check_compact("ui-default", &ui_for(&config, &values, Role::Admin, "default").doc);
	// Every role gets a sound one: each tab there, each open leading to a dashboard it has, each
	// action widget's action among its actions, ids unique.
	for role in Role::ALL {
		let ui = ui_for(&config, &values, role, "default");
		let ids: Vec<&str> = ui.dashboards.iter().map(|d| d.id.as_str()).collect();
		assert!(ui.tabs.iter().all(|t| ids.contains(&t.as_str())), "{role:?}");
		assert!(!ui.tabs.is_empty(), "{role:?}");
		let mut seen = std::collections::HashSet::new();
		for d in &ui.dashboards {
			for w in d.all_widgets() {
				assert!(seen.insert(w.id.clone()), "{} twice for {role:?}", w.id);
				if let Some(o) = &w.def.open {
					assert!(ids.contains(&o.as_str()), "{} opens {o} for {role:?}", w.id);
				}
				if let Some(a) = &w.def.action {
					assert!(ui.actions.iter().any(|s| &s.id == a), "{} runs {a} for {role:?}", w.id);
				}
			}
		}
		let monitor = role >= Role::Monitor;
		assert_eq!(ui.tabs.iter().any(|t| t == "processes"), monitor, "{role:?}");
		assert_eq!(ui.actions.is_empty(), role < Role::Operate, "{role:?}");
		assert_eq!(ui.header.as_deref(), Some("header"));
	}
}

/// Three certificate rotations in a row, as `GET /api/v1/identity/rotations` serves them. Their
/// keys are new each time they are made, so the file is only written when blessed; otherwise it
/// is read back, checked to be what the service writes, and followed as the app follows it.
#[test]
fn rotations() {
	use crate::identity::{self, Identity};
	let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/rotations.json");
	if std::env::var_os("ELAMON_BLESS").is_some() {
		let dir = std::env::temp_dir().join(format!("elamond-fixture-{}", std::process::id()));
		std::fs::create_dir_all(&dir).unwrap();
		Identity::load_or_create(&dir).unwrap();
		for _ in 0..3 {
			identity::rotate(&dir).unwrap();
		}
		let current = Identity::load(&dir).unwrap().fingerprint;
		let chain = identity::rotations(&dir).unwrap();
		let text = serde_json::to_string_pretty(identity::chain_to(&chain, &current)).unwrap() + "\n";
		std::fs::write(&path, text).unwrap();
		std::fs::remove_dir_all(dir).unwrap();
	}
	let text = std::fs::read_to_string(&path).unwrap();
	let chain: Vec<identity::Rotation> = serde_json::from_str(&text).unwrap();
	let again = serde_json::to_string_pretty(&chain).unwrap() + "\n";
	assert!(again == text, "elamond/fixtures/rotations.json is out of date: run `ELAMON_BLESS=1 cargo test` and commit it");
	assert_eq!(chain.len(), 3);
	let last = chain[2].new_fp.clone();
	assert_eq!(identity::follow(&chain, &chain[0].old_fp, &last), Some(last));
}
