//! Every JSON shape the service sends, with each optional field filled in, kept in
//! elamond/fixtures/, and the requests clients send checked against what the service reads. The
//! app's tests read the same files, so a field missing or under another name on either side fails
//! a build rather than reading as zero on a phone.
//!
//! After a deliberate change to the protocol: `ELAMON_BLESS=1 cargo test`, then commit the
//! files with the change.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

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
fn check_compact(name: &str, value: &Value) {
	let mut text = String::new();
	compact(value, 0, 0, &mut text);
	write_or_compare(name, text + "\n");
}

fn write_or_compare(name: &str, text: String) {
	check_text(&format!("{name}.json"), &text);
}

/// The fixture `file` is `text`, or is written as it with `ELAMON_BLESS=1`.
pub fn check_text(file: &str, text: &str) {
	let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
	let path = dir.join(file);
	if std::env::var_os("ELAMON_BLESS").is_some() {
		std::fs::create_dir_all(&dir).unwrap();
		std::fs::write(&path, text).unwrap();
		return;
	}
	let kept = std::fs::read_to_string(&path).unwrap_or_default();
	assert!(kept == text, "elamond/fixtures/{file} is out of date: run `ELAMON_BLESS=1 cargo test` and commit it");
}

/// `v` on one line: `{ "a": 1, "b": [1, 2] }`.
fn one_line(v: &Value) -> String {
	match v {
		Value::Array(a) if a.is_empty() => "[]".into(),
		Value::Array(a) => format!("[{}]", a.iter().map(one_line).collect::<Vec<_>>().join(", ")),
		Value::Object(o) if o.is_empty() => "{}".into(),
		Value::Object(o) => {
			let parts: Vec<String> = o.keys().map(|k| format!("{}: {}", serde_json::to_string(k).unwrap(), one_line(o.get(k).unwrap()))).collect();
			format!("{{ {} }}", parts.join(", "))
		}
		other => serde_json::to_string(other).unwrap(),
	}
}

/// `v` at `indent`, after `used` columns of its line: on one line if it fits, else one entry to a
/// line. The outermost is always spread.
fn compact(v: &Value, indent: usize, used: usize, out: &mut String) {
	let line = one_line(v);
	if indent > 0 && used + line.chars().count() <= 140 {
		out.push_str(&line);
		return;
	}
	let pad = "  ".repeat(indent + 1);
	match v {
		Value::Array(a) if !a.is_empty() => {
			out.push_str("[\n");
			for (i, e) in a.iter().enumerate() {
				out.push_str(&pad);
				compact(e, indent + 1, pad.len(), out);
				out.push_str(if i + 1 < a.len() { ",\n" } else { "\n" });
			}
			out.push_str(&"  ".repeat(indent));
			out.push(']');
		}
		Value::Object(o) if !o.is_empty() => {
			out.push_str("{\n");
			for (i, (k, e)) in o.iter().enumerate() {
				let key = format!("{}: ", serde_json::to_string(k).unwrap());
				out.push_str(&pad);
				out.push_str(&key);
				compact(e, indent + 1, pad.len() + key.chars().count(), out);
				out.push_str(if i + 1 < o.len() { ",\n" } else { "\n" });
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
	let body = hello_body("0123456789abcdef0123456789abcdef", "fixture");
	check_compact("hello", &body);
	// As 0.8 has it, until it moves up beside the others.
	check_compact("0.8/hello", &body);
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
	check_compact("pair", &body);
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
	// The fixture station's source does what the station offers as far as it allows.
	let caps = if config.sources.contains_key("eco") { ups_caps(config, values) } else { crate::actions::Caps::all(&values.power_profiles) };
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
	let history = crate::history::json(home, &ring(), &values, ui.units, t);
	check_compact("stream-data", &Value::Object(crate::states::data(t, home, header, &s, Some(history))));
	let detail = ui.dashboard("detail").unwrap();
	check_compact("stream-data-detail", &Value::Object(crate::states::data(t + 1000, detail, header, &s, None)));
}

/// The `open` line of a stream with `ui.json`'s version, and the `ui` line a stream sends when its
/// client's `ui` changes: here its role, to `operate`.
#[test]
fn stream_open_and_ui() {
	let config = ui_source();
	let values = values(&config);
	let ui = ui_for(&config, &values, Role::Admin, "owner");
	check_compact("stream-open", &Value::Object(crate::stream::open_line("s1a2b3c4", &ui.version)));
	let operate = ui_for(&config, &values, Role::Operate, "owner");
	assert_ne!(operate.version, ui.version);
	check_compact("stream-ui", &Value::Object(crate::stream::ui_line(&operate.version, &endpoints())));
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

// ---- 0.7: the long history -------------------------------------------------------------------

/// `GET /api/v1/history` of the fixture station's graph over `all` (0.7/history.json), from the
/// hourly points 0.7/ui-ups-source.toml lists, written and read again, at the fixture sample's `t`.
#[test]
fn history() {
	use crate::history::{api, disk, Store};
	let dir = disk::tests::scratch("fixture");
	let series = ["eco.input.realpower", "eco.ups.realpower"];
	let hours: [(u64, [f64; 2]); 5] =
		[(472_216, [228.4, 205.1]), (472_217, [214.0, 207.6]), (472_218, [209.5, 203.9]), (472_220, [231.2, 210.4]), (472_221, [212.8, 208.0])];
	let store = Store::new(dir.clone());
	for (slot, x) in hours {
		store.push((slot * 3600 + 1800) * 1000, series.into_iter().zip(x));
	}
	store.flush(sample().t / 1000, true);
	let store = Store::new(dir.clone());
	let addresses = series.map(String::from);
	let answer = api::graph(&store, "ups.eco/watts", "all", &addresses, &ups_values(&ui_ups_source()), crate::values::Units::default(), sample().t).unwrap();
	check_compact("0.7/history", &Value::Object(answer));
	std::fs::remove_dir_all(&dir).unwrap();
}

/// `GET /api/v1/history` of the fixture station's timeline over `7d` (0.7/history-timeline.json),
/// from its power's changes, at the fixture sample's `t`.
#[test]
fn history_timeline() {
	let values = ups_values(&ui_ups_source());
	let answer = crate::history::api::timeline(&values.timelines["eco"], "ups.eco/power", "7d", sample().t).unwrap();
	check_compact("0.7/history-timeline", &Value::Object(answer));
}

/// A client that doesn't draw timelines gets the fixture station's as a text: its status, how long
/// it has been so, and the status's level.
#[test]
fn a_timeline_reaches_an_older_app_as_its_status() {
	let config = ui_ups_source();
	let values = ups_values(&config);
	let caps = crate::actions::Caps::all(&values.power_profiles);
	let draws: std::collections::BTreeSet<String> = crate::settings::WIDGET_TYPES.iter().filter(|t| **t != "timeline").map(|t| t.to_string()).collect();
	let ui = crate::ui::build(&crate::ui::tests::input(&config, &values, &caps, Role::Glance, "owner", Some(&draws)));
	let d = ui.dashboard("ups.eco").unwrap();
	let w = d.widgets.iter().find(|w| w.id == "ups.eco/power").unwrap();
	let json = &ui.doc["dashboards"][0]["widgets"].as_array().unwrap().iter().find(|w| w["id"] == "ups.eco/power").unwrap().clone();
	assert_eq!(json, &serde_json::json!({ "id": "ups.eco/power", "type": "text", "span": 4, "label": "Power", "icon": "power", "level": "normal" }));
	let sorts = std::collections::HashMap::new();
	let s = crate::states::States { view: crate::values::View::new(&values, Role::Glance, ui.units), control: false, sorts: &sorts, read: crate::ui::Sources::ALL };
	assert_eq!(Value::Object(crate::states::state(w, &s).unwrap()), serde_json::json!({ "text": "On battery · 12 min", "level": "warning" }));
	// Unreachable, it has been so for no time the service knows of: the status alone.
	let mut gone = values.clone();
	gone.scalars.remove("eco.status.since");
	gone.scalars.insert("eco.status".into(), crate::values::source_scalar("status", crate::values::V::Text("Unreachable".into()), Some(crate::values::Level::Critical)));
	let s = crate::states::States { view: crate::values::View::new(&gone, Role::Glance, ui.units), control: false, sorts: &sorts, read: crate::ui::Sources::ALL };
	assert_eq!(Value::Object(crate::states::state(w, &s).unwrap()), serde_json::json!({ "text": "Unreachable", "level": "critical" }));
}

// ---- 0.7: a UPS source -----------------------------------------------------------------------

/// The settings 0.7/ui-ups.json and 0.7/stream-ups.json describe.
fn ui_ups_source() -> crate::settings::Config {
	crate::settings::read_sources(&[crate::settings::Source::new("ui-ups-source.toml", include_str!("../fixtures/0.7/ui-ups-source.toml"))]).unwrap()
}

/// The fixture station's variables, as 0.7/ui-ups-source.toml's comments list them, with `status`
/// for `ups.status`.
fn ups_vars(status: &str) -> Vec<(String, String)> {
	let list = [
		("battery.charge", "87"),
		("battery.charge.low", "20"),
		("battery.charger.status", "discharging"),
		("battery.current", "-4.2"),
		("battery.runtime", "5400"),
		("battery.runtime.low", "300"),
		("battery.temperature", "31"),
		("battery.voltage", "52.4"),
		("device.mfr", "EcoFlow"),
		("device.model", "DELTA 3 Max Plus"),
		("device.type", "ups"),
		("driver.name", "nutflow"),
		("driver.version", "0.4.0"),
		("experimental.battery.charge.max", "100"),
		("experimental.output.xboost", "enabled"),
		("input.realpower", "0"),
		("outlet.count", "9"),
		("outlet.1.desc", "AC outlets, group 1"),
		("outlet.1.id", "1"),
		("outlet.1.realpower", "190"),
		("outlet.1.status", "on"),
		("outlet.1.switchable", "yes"),
		("outlet.1.voltage", "230.1"),
		("outlet.2.desc", "AC outlets, group 2"),
		("outlet.2.id", "2"),
		("outlet.2.realpower", "0"),
		("outlet.2.status", "off"),
		("outlet.2.switchable", "yes"),
		("outlet.9.current", "0.57"),
		("outlet.9.desc", "12 V port 2"),
		("outlet.9.id", "9"),
		("outlet.9.realpower", "8"),
		("outlet.9.status", "on"),
		("outlet.9.switchable", "yes"),
		("outlet.9.voltage", "12.6"),
		("output.realpower", "190"),
		("output.voltage", "230.1"),
		("ups.beeper.status", "enabled"),
		("ups.firmware", "2.1.1.104"),
		("ups.load", "6.3"),
		("ups.mfr", "EcoFlow"),
		("ups.model", "DELTA 3 Max Plus"),
		("ups.realpower", "210"),
		("ups.realpower.nominal", "3000"),
		("ups.status", status),
		("ups.temperature", "48.6"),
	];
	list.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

/// What upsd says the fixture station offers, as 0.7/ui-ups-source.toml's comments list it: its
/// commands in upsd's order with their descriptions, and its writable variables.
fn ups_offers(st: &mut crate::nut::Station) {
	use upst_core::nut::{CmdInfo, VarInfo};
	let cmds = [
		("beeper.enable", "Turn the beeper on"),
		("beeper.disable", "Turn the beeper off"),
		("outlet.1.load.on", "Turn on AC outlets, group 1"),
		("outlet.1.load.off", "Turn off AC outlets, group 1"),
		("outlet.2.load.on", "Turn on AC outlets, group 2"),
		("outlet.2.load.off", "Turn off AC outlets, group 2"),
	];
	let cmds = cmds.iter().map(|(name, desc)| CmdInfo { name: name.to_string(), desc: Some(desc.to_string()) }).collect();
	let max = "experimental.battery.charge.max".to_string();
	let xboost = "experimental.output.xboost".to_string();
	// Described, as upsd would be asked to, whatever the source's `settings` allow.
	st.offers(cmds, vec![max.clone(), xboost.clone()]);
	let words = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
	st.described(max, VarInfo { desc: Some("Charge limit (percent)".into()), kind: words(&["RW", "RANGE", "NUMBER"]), enum_values: Vec::new(), ranges: vec![("50".into(), "100".into())] });
	st.described(
		xboost,
		VarInfo {
			desc: Some("X-Boost: run heavy AC loads at a lower voltage".into()),
			kind: words(&["RW", "ENUM", "STRING:32"]),
			enum_values: words(&["enabled", "disabled"]),
			ranges: Vec::new(),
		},
	);
}

/// The fixture station as its source's task leaves it at the fixture sample's `t`, its power as its
/// timeline has it: on line from 1699977600, nothing known from 1699988400, on line again from
/// 1699992000, on battery since 1699999280; what it offers told as upsd is first connected to.
pub fn ups(config: &crate::settings::Config) -> std::collections::BTreeMap<String, std::sync::Arc<crate::nut::Snapshot>> {
	let at = |secs: u64| std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
	let mut st = crate::nut::Station::new("eco", &config.sources["eco"]);
	ups_offers(&mut st);
	st.polled(ups_vars("OL"), at(1_699_977_600));
	st.lost(upst_core::Link::Down("connection refused".into()), at(1_699_988_400));
	st.polled(ups_vars("OL"), at(1_699_992_000));
	st.polled(ups_vars("OB DISCHRG"), at(1_699_999_280));
	let now = std::time::UNIX_EPOCH + std::time::Duration::from_millis(sample().t);
	st.polled(ups_vars("OB DISCHRG"), now);
	[("eco".to_string(), std::sync::Arc::new(st.snapshot(now)))].into()
}

/// The fixture readings for `config`, with its UPS sources as the fixture station.
pub fn ups_values(config: &crate::settings::Config) -> crate::values::Values {
	let mut v = values(config);
	crate::nut::put(&mut v, config, &ups(config));
	v
}

/// What the computer can do with the fixture station of `config`: everything, and the station's
/// actions.
pub fn ups_caps(config: &crate::settings::Config, values: &crate::values::Values) -> crate::actions::Caps {
	crate::actions::Caps { nut: crate::nut::actions::of_sources(config, &ups(config)), ..crate::actions::Caps::all(&values.power_profiles) }
}

/// `ui` for the fixture station's settings, as an admin `android` client of profile `owner` that
/// draws every widget type gets it.
fn ui_ups() -> crate::ui::Ui {
	let config = ui_ups_source();
	let values = ups_values(&config);
	let mut features = FEATURES.to_vec();
	features.push("ups");
	let info = info();
	let caps = ups_caps(&config, &values);
	crate::ui::build(&crate::ui::Input {
		config: &config,
		values: &values,
		caps: &caps,
		service: crate::ui::Service {
			id: info.id,
			name: info.name,
			version: info.service_version.into(),
			interval_ms: info.interval_ms,
			history_seconds: info.history_seconds,
			features: features_of(&features, config.control),
			endpoints: endpoints(),
		},
		control: crate::ui::Control { enabled: true, off_reason: None },
		role: Role::Admin,
		profile: "owner",
		kind: "android",
		draws: None,
	})
}

/// The `data` line of the fixture station's dashboard.
fn stream_ups() -> Value {
	let config = ui_ups_source();
	let values = ups_values(&config);
	let ui = ui_ups();
	let sorts = std::collections::HashMap::new();
	let s = crate::states::States { view: crate::values::View::new(&values, Role::Admin, ui.units), control: true, sorts: &sorts, read: crate::ui::Sources::ALL };
	Value::Object(crate::states::data(sample().t, ui.dashboard("ups.eco").unwrap(), None, &s, None))
}

/// What the service builds of the fixture station is 0.7/ui-ups.json and 0.7/stream-ups.json.
#[test]
fn ui_ups_whole() {
	check_compact("0.7/ui-ups", &ui_ups().doc);
}

#[test]
fn stream_ups_whole() {
	check_compact("0.7/stream-ups", &stream_ups());
}

/// The typed request the app sends (0.7/action-request-typed.json) reads as the service reads it,
/// and passes the checks of the station's command it names, which without its text it doesn't.
#[test]
fn a_typed_request_is_the_command_and_its_text() {
	let read = |name: &str| std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(name)).unwrap();
	let request: crate::control::ActionRequest = serde_json::from_slice(&read("0.7/action-request-typed.json")).unwrap();
	let ui = ui_ups();
	let spec = ui.actions.iter().find(|a| a.id == "nut.command.eco").unwrap();
	crate::nut::actions::offered(spec, &request.params).unwrap();
	let params = crate::actions::check_params(spec, &request.params).unwrap();
	let text = request.confirm_text.as_deref();
	assert_eq!(text, Some("outlet.1.load.off"));
	crate::nut::actions::typed_in(spec, &params, text).unwrap();
	crate::nut::actions::still_allowed(&ui_ups_source(), Role::Admin, &spec.id, &params, text).unwrap();
	assert_eq!(crate::nut::actions::typed_in(spec, &params, None), Err((ErrorCode::ConfirmRequired, "outlet.1.load.off".into())));
}

/// The starter `ups` dashboard, with a source of the fixture station added to the starter settings:
/// a tab of the default profile, showing what the station has and leaving out what it hasn't.
#[test]
fn the_starter_ups_dashboard_shows_what_a_station_has() {
	let config = crate::settings::read_sources(&[
		crate::settings::Source::new("elamond.toml", crate::settings::DEFAULTS),
		crate::settings::Source::new("elamond.d/ups.toml", "[sources.eco]\nkind = \"nut\"\nups = \"ups\"\n"),
	])
	.unwrap();
	let values = ups_values(&config);
	for role in Role::ALL {
		let ui = ui_for(&config, &values, role, "default");
		assert_eq!(ui.tabs[..2], ["overview", "ups.eco"], "{role:?}");
		let ups = ui.dashboard("ups.eco").unwrap();
		assert_eq!(ups.title, "DELTA 3 Max Plus");
		let ids: Vec<&str> = ups.all_widgets().iter().map(|w| w.id.as_str()).collect();
		assert_eq!(
			ids,
			[
				"ups.eco/status",
				"ups.eco/charge",
				"ups.eco/power",
				"ups.eco/now",
				"ups.eco/now/grid",
				"ups.eco/now/output",
				"ups.eco/now/ac",
				"ups.eco/now/load",
				"ups.eco/battery",
				"ups.eco/outlets",
				"ups.eco/electrical",
				"ups.eco/charge_graph",
				"ups.eco/watts",
				"ups.eco/details",
			],
			"{role:?}"
		);
		let rows = |id: &str| -> Vec<String> {
			let w = ups.widgets.iter().find(|w| w.id == id).unwrap();
			w.def.items.iter().flatten().filter_map(|i| i.label.clone()).collect()
		};
		assert_eq!(rows("ups.eco/battery"), ["Charge", "Runtime", "Charge limit", "Voltage", "Current"]);
		assert_eq!(rows("ups.eco/electrical"), ["Output voltage", "Battery temperature", "Inverter"]);
		assert_eq!(rows("ups.eco/details"), ["Model", "Firmware", "Rated power", "Low battery at", "Low runtime at"]);
	}
	// Its graphs keep their history.
	let tracked = crate::ui::tracked(&config, &values);
	for a in ["eco.battery.charge", "eco.input.realpower", "eco.ups.realpower", "cpu.usage"] {
		assert!(tracked.contains(a), "{a}");
	}
	// The station's numbers are kept besides, once each.
	let recorded: Vec<&str> = crate::history::recorded(&values, &tracked).map(|(a, _)| a).collect();
	for a in ["eco.battery.charge", "eco.outlet.9.current", "eco.ups.temperature", "cpu.usage"] {
		assert_eq!(recorded.iter().filter(|r| **r == a).count(), 1, "{a}");
	}
	assert!(!recorded.iter().any(|a| *a == "eco.driver.version" || *a == "eco.outlet.1.id" || *a == "eco.status"));
	// Allowed to run the station's commands and settings, an admin gets its switches and settings: an
	// outlet's switch where the station offers its commands, and of the settings those it has.
	let config = crate::settings::read_sources(&[
		crate::settings::Source::new("elamond.toml", crate::settings::DEFAULTS),
		crate::settings::Source::new(
			"elamond.d/ups.toml",
			"[sources.eco]\nkind = \"nut\"\nups = \"ups\"\nuser = \"elamon\"\npassword_file = \"/etc/elamon/nut.password\"\ncommands = [\"outlet.*.load.*\", \"beeper.*\"]\nsettings = [\"experimental.*\"]\n",
		),
	])
	.unwrap();
	let values = ups_values(&config);
	for role in Role::ALL {
		let ui = ui_for(&config, &values, role, "default");
		let ups = ui.dashboard("ups.eco").unwrap();
		let control: Vec<(&str, &str)> =
			ups.all_widgets().into_iter().filter(|w| w.def.action.is_some() || ["ups.eco/switches", "ups.eco/station"].contains(&w.id.as_str())).map(|w| (w.id.as_str(), w.kind.as_str())).collect();
		if role < Role::Admin {
			assert!(control.is_empty(), "{role:?}: {control:?}");
			continue;
		}
		assert_eq!(
			control,
			[
				("ups.eco/switches", "group"),
				("ups.eco/switches/outlet.eco.1", "toggle"),
				("ups.eco/switches/outlet.eco.2", "toggle"),
				("ups.eco/station", "group"),
				("ups.eco/station/charge_max", "slider"),
				("ups.eco/station/xboost", "select"),
				("ups.eco/station/beeper", "toggle"),
			]
		);
		// The beeper's state reads as the command that turns it to what it is.
		let sorts = std::collections::HashMap::new();
		let s = crate::states::States { view: crate::values::View::new(&values, role, ui.units), control: true, sorts: &sorts, read: crate::ui::Sources::ALL };
		let beeper = ups.all_widgets().into_iter().find(|w| w.id == "ups.eco/station/beeper").unwrap();
		assert_eq!(Value::Object(crate::states::state(beeper, &s).unwrap()), serde_json::json!({ "v": "beeper.enable", "enabled": true }));
	}
	// Without a source the tab isn't there.
	let config = crate::settings::read_sources(&[crate::settings::Source::new("elamond.toml", crate::settings::DEFAULTS)]).unwrap();
	let bare = self::values(&config);
	let ui = ui_for(&config, &bare, Role::Admin, "default");
	assert_eq!(ui.tabs, ["overview", "processes", "sensors", "control"]);
}

// ---- 0.8: a command, a web service and a file ------------------------------------------------

/// The settings 0.8/ui-sources.json, 0.8/stream-sources.json and 0.8/stream-log-next.json describe.
const UI_SOURCES: &str = include_str!("../fixtures/0.8/ui-sources.toml");

/// Those settings without what needs the journal: the `log` widgets, the row action and the button
/// that open one, and the journal's dashboards.
fn ui_sources_without_journal() -> crate::settings::Config {
	let mut t: toml::Table = UI_SOURCES.parse().unwrap();
	let dashboards = t.get_mut("dashboards").and_then(toml::Value::as_table_mut).unwrap();
	dashboards.remove("journal");
	dashboards.remove("unit_log");
	for (_, d) in dashboards.iter_mut() {
		let widgets = d.get_mut("widgets").and_then(toml::Value::as_array_mut).unwrap();
		widgets.retain(|w| w.get("type").and_then(toml::Value::as_str) != Some("log") && w.get("open").is_none());
		for actions in widgets.iter_mut().filter_map(|w| w.get_mut("row_actions").and_then(toml::Value::as_array_mut)) {
			actions.retain(|a| a.get("action").is_some());
		}
	}
	crate::settings::read_sources(&[crate::settings::Source::new("ui-sources.toml", toml::to_string(&t).unwrap())]).unwrap()
}

/// The fixture readings for `config`, with its sources' last reads as 0.8/ui-sources.toml's
/// comments have them.
pub fn sources_values(config: &crate::settings::Config) -> crate::values::Values {
	use crate::settings::FeedFormat;
	let at = |secs: u64| std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
	let backup = "state: ok\nlast run = 1699996400\nfiles: 1204\nverified=true\nChecked 3 repositories.\n";
	let app = r#"{"state": "ok", "version": "2.4.1", "queue": {"length": 3},
		"workers": [{"name": "main", "busy": true}, {"name": "spare", "busy": false}],
		"build info": {"commit": "0123abc"}, "note": null}"#;
	let snapshots = [
		("backup", crate::feeds::read_as("backup", FeedFormat::Lines, backup, Some(0), at(1_699_999_940))),
		("app", crate::feeds::read_as("app", FeedFormat::Json, app, None, at(1_699_999_990))),
		("zone", crate::feeds::read_as("zone", FeedFormat::Text, "45000\n", None, at(1_699_999_998))),
	];
	let mut v = values(config);
	crate::feeds::put(&mut v, config, &snapshots.into_iter().map(|(id, s)| (id.to_string(), std::sync::Arc::new(s))).collect());
	v
}

/// A fixture of 0.8, as JSON.
fn fixture_08(name: &str) -> Value {
	serde_json::from_str(&std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/0.8").join(name)).unwrap()).unwrap()
}

/// The widget `id` of a `ui` document.
pub fn widget_of(doc: &Value, id: &str) -> Option<Value> {
	doc["dashboards"].as_array()?.iter().flat_map(|d| d["widgets"].as_array().into_iter().flatten()).find(|w| w["id"] == id).cloned()
}

/// What the service builds of the fixture's sources is what 0.8/ui-sources.json and
/// 0.8/stream-sources.json say of their widgets; the journal's are checked with it whole.
#[test]
fn the_sources_widgets_ui_and_states() {
	let config = ui_sources_without_journal();
	let values = sources_values(&config);
	let ui = ui_for(&config, &values, Role::Admin, "owner");
	let want = fixture_08("ui-sources.json");
	assert_eq!(ui.tabs, ["sources", "control"]);
	let ids = ["sources/app", "sources/zone", "sources/files", "sources/reads"];
	for id in ids.into_iter().chain(["control/links"]) {
		let mut got = widget_of(&ui.doc, id);
		if id == "control/links" {
			// Its row action that opens the journal is the journal's.
			let mut w = widget_of(&want, id).unwrap();
			w["row_actions"].as_array_mut().unwrap().retain(|a| a.get("action").is_some());
			assert_eq!(got.take(), Some(w));
			continue;
		}
		assert_eq!(got, widget_of(&want, id), "{id}");
	}
	// A source that never failed has never had an error.
	assert!(widget_of(&ui.doc, "sources/app_error").is_none());
	let sorts = std::collections::HashMap::new();
	let s = crate::states::States { view: crate::values::View::new(&values, Role::Admin, ui.units), control: true, sorts: &sorts, read: crate::ui::Sources::ALL };
	let sources = ui.dashboard("sources").unwrap();
	let line = Value::Object(crate::states::data(sample().t, sources, None, &s, None));
	let want = fixture_08("stream-sources.json");
	assert_eq!((&line["t"], &line["dashboard"]), (&want["t"], &want["dashboard"]));
	for id in ids {
		assert_eq!(line["widgets"][id], want["widgets"][id], "{id}");
	}
	// The dashboard on screen needs its three sources read; no graph does, and `zone`'s history is kept.
	assert_eq!(sources.source_heads().iter().collect::<Vec<_>>(), ["app", "backup", "zone"]);
	assert!(crate::ui::tracked(&config, &values).iter().all(|a| !a.contains('.') || crate::values::known_prefix(a)));
	assert_eq!(values.kept.iter().collect::<Vec<_>>(), ["zone.text"]);
}

/// What the service builds of the fixture's settings is 0.8/ui-sources.json.
#[test]
fn ui_sources_whole() {
	let config = crate::settings::read_sources(&[crate::settings::Source::new("ui-sources.toml", UI_SOURCES)]).unwrap();
	let values = sources_values(&config);
	check_compact("0.8/ui-sources", &ui_for(&config, &values, Role::Admin, "owner").doc);
}

/// A tail that has read `lines`, as journalctl prints them, without a journalctl.
fn tail_of(lines: &str) -> std::sync::Arc<crate::journal::Tail> {
	let tail = crate::journal::Tail::idle();
	{
		let mut b = tail.buffer();
		b.start(None);
		for l in lines.lines() {
			b.push(crate::journal::parse(l.as_bytes()).unwrap());
		}
		b.ready();
	}
	std::sync::Arc::new(tail)
}

/// The states of the fixture's settings' `ui`, as an admin sees them.
fn sources_states<'a>(values: &'a crate::values::Values, ui: &crate::ui::Ui, sorts: &'a std::collections::HashMap<String, String>) -> crate::states::States<'a> {
	crate::states::States { view: crate::values::View::new(values, Role::Admin, ui.units), control: true, sorts, read: crate::ui::Sources::ALL }
}

/// The data line of the fixture's `sources` dashboard: the reading's states, then its logs' first
/// ones, as a stream adds them, of the journals the fixture's comments list.
#[test]
fn stream_sources_whole() {
	use crate::journal::{tests, Key, Logs};
	let config = crate::settings::read_sources(&[crate::settings::Source::new("ui-sources.toml", UI_SOURCES)]).unwrap();
	let values = sources_values(&config);
	let ui = ui_for(&config, &values, Role::Admin, "owner");
	let sorts = std::collections::HashMap::new();
	let sources = ui.dashboard("sources").unwrap();
	let mut line = crate::states::data(sample().t, sources, None, &sources_states(&values, &ui, &sorts), None);
	let tails = [(Key::User, tail_of(&tests::user_lines())), (Key::Unit { name: "example.service".into(), user: false }, tail_of(&tests::example_lines()))];
	let mut logs = Logs::default();
	logs.show(crate::journal::on_screen(&ui, sources, &config), |key| tails.iter().find(|(k, _)| k == key).unwrap().1.clone());
	logs.put(&mut line);
	check_compact("0.8/stream-sources", &Value::Object(line));
}

/// A later data line of the fixture's `journal` dashboard (0.8/stream-log-next.json): its stream
/// sent the system journal's lines up to 1700000000000000; then 1012 came, 12 more than the tail
/// keeps, two of them at warning or above.
#[test]
fn stream_log_next() {
	use crate::journal::tests::json_line;
	let config = crate::settings::read_sources(&[crate::settings::Source::new("ui-sources.toml", UI_SOURCES)]).unwrap();
	let values = sources_values(&config);
	let ui = ui_for(&config, &values, Role::Admin, "owner");
	let sorts = std::collections::HashMap::new();
	let s = sources_states(&values, &ui, &sorts);
	let journal = ui.dashboard("journal").unwrap();
	let tail = tail_of(&json_line(1_700_000_000_000_000, 4, Some("example.service"), None, "example", "Started"));
	let mut logs = crate::journal::Logs::default();
	logs.show(crate::journal::on_screen(&ui, journal, &config), |_| tail.clone());
	let mut first = crate::states::data(sample().t, journal, None, &s, None);
	logs.put(&mut first);
	assert_eq!(first["widgets"]["journal/system"]["lines"][0]["c"], 1_700_000_000_000_000_u64);
	{
		let mut b = tail.buffer();
		for i in 1..=1010 {
			b.push(crate::journal::parse(json_line(1_700_000_000_000_000 + i * 2000, 6, Some("example.service"), None, "example", "Request served").as_bytes()).unwrap());
		}
		let sshd = "Disconnecting invalid user admin 192.0.2.30 port 41022: Too many authentication failures [preauth]";
		b.push(crate::journal::parse(json_line(1_700_000_003_123_456, 4, Some("sshd.service"), None, "sshd", sshd).as_bytes()).unwrap());
		let lost = "Lost the connection to the database, retrying in 5 s";
		b.push(crate::journal::parse(json_line(1_700_000_004_234_567, 3, Some("example.service"), None, "example", lost).as_bytes()).unwrap());
	}
	let mut next = crate::states::data(1_700_000_005_123, journal, None, &s, None);
	logs.put(&mut next);
	check_compact("0.8/stream-log-next", &Value::Object(next));
}

/// `GET /api/v1/log` of the fixture's journal widget (0.8/log.json), before the oldest line its
/// client had: the system journal's 2000 lines before it, as the fixture's comments list them.
#[tokio::test]
async fn log_page() {
	use crate::journal::{api, priority_of, tests, Filter, Key};
	let config = crate::settings::read_sources(&[crate::settings::Source::new("ui-sources.toml", UI_SOURCES)]).unwrap();
	let ui = ui_for(&config, &sources_values(&config), Role::Admin, "owner");
	let w = ui.dashboard("journal").unwrap().widgets.iter().find(|w| w.id == "journal/system").unwrap();
	assert_eq!(Key::of(&w.def, &config), Some(Key::System));
	let filter = Filter::new(priority_of(w.def.level.as_deref().unwrap()).unwrap(), None);
	let (lines, before) = api::page(&tests::fake(), &Key::System, Some(1_699_999_995_234_567), &filter).await.unwrap();
	check_compact("0.8/log", &Value::Object(api::answer(&w.id, &lines, before)));
}

/// The change of two logs' filters the app sends (0.8/subscribe-log.json) reads as the service
/// reads it, of two logs of the fixture's `sources`.
#[test]
fn subscribe_log() {
	let change: crate::stream::Change = serde_json::from_value(fixture_08("subscribe-log.json")).unwrap();
	let filter = crate::stream::LogFilter { level: Some("error".into()), search: Some("timeout".into()) };
	assert_eq!(change.log.into_iter().collect::<Vec<_>>(), [("sources/app_log".to_string(), Some(filter)), ("sources/journal".to_string(), None)]);
	let config = crate::settings::read_sources(&[crate::settings::Source::new("ui-sources.toml", UI_SOURCES)]).unwrap();
	let ui = ui_for(&config, &sources_values(&config), Role::Admin, "owner");
	let logs: Vec<&str> = crate::journal::on_screen(&ui, ui.dashboard("sources").unwrap(), &config).into_iter().map(|(w, _)| w.id.as_str()).collect();
	assert_eq!(logs, ["sources/journal", "sources/app_log"]);
}
