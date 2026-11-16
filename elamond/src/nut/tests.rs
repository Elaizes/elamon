use std::collections::BTreeSet;
use std::time::{Duration, Instant, UNIX_EPOCH};

use upst_core::demo;

use super::*;
use crate::access::Role;
use crate::settings::{self, Source};
use crate::values::{Units, View};

/// A history of a test's own, taken away when it ends.
pub struct Scratch(Arc<Store>);

impl Scratch {
	pub fn new() -> Scratch {
		Scratch(Arc::new(Store::new(crate::history::disk::tests::scratch("nut"))))
	}

	pub fn store(&self) -> Arc<Store> {
		self.0.clone()
	}
}

impl Drop for Scratch {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(self.0.dir());
	}
}

/// The tests that run pollers, one at a time: a thread left from one is never counted by another.
pub static POLLERS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn source(text: &str) -> Config {
	settings::read_sources(&[Source::new("a.toml", text)]).unwrap_or_else(|m| panic!("{}", settings::join_mistakes(&m)))
}

fn at(secs: u64) -> SystemTime {
	UNIX_EPOCH + Duration::from_secs(secs)
}

fn vars(list: &[(&str, &str)]) -> Vec<(String, String)> {
	list.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

#[test]
fn values_are_told_by_their_names() {
	let def = |a: &str| values::address_def(a).unwrap();
	let shown = |a: &str| {
		let d = def(a);
		(d.unit, d.format, d.role)
	};
	assert_eq!(shown("eco.outlet.2.realpower"), ("W", "watts", Role::Glance));
	assert_eq!(shown("eco.device.serial"), ("", "text", Role::Monitor));
	assert_eq!(shown("eco.ambient.temperature"), ("°C", "temp", Role::Glance));
	assert_eq!(shown("eco.battery.charge"), ("%", "percent", Role::Glance));
	assert_eq!(shown("eco.battery.runtime"), ("s", "duration", Role::Glance));
	assert_eq!(shown("eco.input.voltage"), ("V", "number", Role::Glance));
	assert_eq!(shown("eco.driver.version"), ("", "text", Role::Monitor));
	assert_eq!(shown("eco.ups.firmware"), ("", "text", Role::Glance));
	assert_eq!(shown("eco.status"), ("", "text", Role::Glance));
	assert_eq!(shown("eco.status.since"), ("s", "duration", Role::Glance));
	assert_eq!(shown("eco.link.error"), ("", "text", Role::Monitor));
	assert_eq!(shown("eco.estimate.watts"), ("W", "watts", Role::Glance));
	// The computer's own stay as they are, and a wrong one of them is none.
	assert_eq!(def("cpu.usage").format, "percent");
	assert!(values::address_def("cpu.usgae").is_none());
	// A number without a unit is a number; a temperature in °F for a profile that wants it.
	assert_eq!(values::source_scalar("outlet.count", V::Float(9.0), None).format, "number");
	assert_eq!(values::source_scalar("device.model", V::Text("x".into()), None).format, "text");
	assert_eq!(values::in_units("eco.ambient.temperature", 20.0, Units { fahrenheit: true, bits: false }), 68.0);
	// Patterns as nutflow's allow has them.
	let p = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
	assert!(allows(&p(&["outlet.*.load.*"]), "outlet.12.load.off"));
	assert!(allows(&p(&["beeper.*", "load.off"]), "load.off"));
	assert!(!allows(&p(&["beeper.*"]), "load.off") && !allows(&p(&["load.off"]), "load.off.delay"));
	assert!(allows(&p(&["*"]), "") && !allows(&[], "beeper.mute"));
}

/// An EcoFlow-like station: on battery, discharging, three outlets.
fn station_vars(status: &str) -> Vec<(String, String)> {
	vars(&[
		("battery.charge", "87"),
		("battery.runtime", "5400"),
		("device.mfr", "EcoFlow"),
		("device.model", "DELTA 3 Max Plus"),
		("driver.version", "0.4.0"),
		("outlet.1.desc", "AC outlets, group 1"),
		("outlet.1.realpower", "190"),
		("outlet.1.status", "on"),
		("outlet.1.switchable", "yes"),
		("outlet.1.voltage", "230.1"),
		("outlet.10.desc", "USB-C 1"),
		("outlet.10.status", "off"),
		("outlet.2.desc", "AC outlets, group 2"),
		("outlet.2.status", "off"),
		("ups.load", "6.3"),
		("ups.realpower", "210"),
		("ups.status", status),
	])
}

fn eco() -> Config {
	source("[sources.eco]\nkind = \"nut\"\nups = \"ups\"\noutlets.\"1\".critical = true\n")
}

#[test]
fn a_station_s_values_collections_and_what_is_kept() {
	let config = eco();
	let src = &config.sources["eco"];
	let mut st = Station::new("eco", src);
	st.polled(station_vars("OL"), at(1000));
	st.polled(station_vars("OB DISCHRG RB"), at(1060));
	let snap = st.snapshot(at(1060));
	let get = |a: &str| snap.scalars.get(a).cloned();
	assert_eq!(get("eco.battery.charge").map(|s| (s.v, s.format, s.limits)), Some((V::Float(87.0), "percent", Some(values::Limits::battery()))));
	assert_eq!(get("eco.ups.load").and_then(|s| s.limits), Some(values::Limits::load()));
	assert_eq!(get("eco.status").map(|s| (s.v, s.level)), Some((V::Text("On battery".into()), Some(Level::Warning))));
	assert_eq!(get("eco.status.flags").map(|s| s.v), Some(V::Text("Replace battery".into())));
	assert_eq!(get("eco.status.power").map(|s| s.v), Some(V::Text("battery".into())));
	assert_eq!(get("eco.link").map(|s| s.v), Some(V::Text("up".into())));
	assert!(get("eco.link.error").is_none() && get("eco.status.since").is_none());
	assert_eq!((snap.supply, snap.since), (Supply::Battery, Some(1060)));
	// Numbers the history may keep: not the driver's version, nor a text.
	assert!(snap.kept.contains(&"eco.battery.charge".to_string()) && snap.kept.contains(&"eco.outlet.1.realpower".to_string()));
	assert!(!snap.kept.iter().any(|a| a == "eco.driver.version" || a == "eco.device.model" || a == "eco.status"));

	// Put among the values at the reading's time: how long it has been on battery, the collections.
	let mut v = Values { t: 1_100_000, ..Default::default() };
	put(&mut v, &config, &[("eco".to_string(), Arc::new(snap))].into());
	assert_eq!(v.scalars["eco.status.since"].v, V::Int(40));
	let ups = &v.collections["ups"];
	assert_eq!(ups.len(), 1);
	let item = |f: &str| ups[0].get(f);
	assert_eq!((item("key"), item("name"), item("mfr")), (Some(V::Text("eco".into())), Some(V::Text("DELTA 3 Max Plus".into())), Some(V::Text("EcoFlow".into()))));
	assert_eq!((item("status"), item("power"), item("link"), item("ecoflow")), (Some(V::Text("On battery".into())), Some(V::Text("battery".into())), Some(V::Text("up".into())), Some(V::Bool(true))));
	let outlets = &v.collections["outlets"];
	let keys: Vec<&str> = outlets.iter().map(|o| o.key.as_str()).collect();
	assert_eq!(keys, ["eco.1", "eco.2", "eco.10"]);
	assert_eq!(outlets[0].get("id"), Some(V::Int(1)));
	assert_eq!((outlets[0].get("critical"), outlets[1].get("critical")), (Some(V::Bool(true)), Some(V::Bool(false))));
	assert_eq!((outlets[0].get("realpower"), outlets[0].get("switchable"), outlets[1].get("switchable")), (Some(V::Float(190.0)), Some(V::Bool(true)), None));
	assert!(v.kept.contains("eco.ups.realpower"));
	// Without its history kept, none of it is.
	let off = source("[sources.eco]\nkind = \"nut\"\nups = \"ups\"\nhistory = false\n");
	let mut v = Values::default();
	let mut st = Station::new("eco", &off.sources["eco"]);
	st.polled(station_vars("OL"), at(1000));
	put(&mut v, &off, &[("eco".to_string(), Arc::new(st.snapshot(at(1000))))].into());
	assert!(v.kept.is_empty() && v.scalars.contains_key("eco.battery.charge"));
	// A source with no snapshot yet is connecting, and in the `ups` collection from the start.
	let mut v = Values::default();
	put(&mut v, &config, &BTreeMap::new());
	assert_eq!((v.scalars["eco.status"].v.clone(), v.scalars["eco.status"].level), (V::Text("Connecting".into()), Some(Level::Normal)));
	assert_eq!(v.scalars["eco.link"].v, V::Text("connecting".into()));
	assert_eq!(v.collections["ups"][0].get("name"), Some(V::Text("eco".into())));
	assert!(v.collections["outlets"].is_empty());
}

#[test]
fn a_station_s_actions_are_what_it_offers_as_far_as_its_source_allows() {
	use upst_core::nut::{CmdInfo, VarInfo};
	let config = source(
		"[sources.offers]\nkind = \"nut\"\nups = \"ups\"\nuser = \"elamon\"\npassword_file = \"/etc/elamon/nut.password\"\n\
		 commands = [\"outlet.*.load.*\", \"load.off\"]\nsettings = [\"experimental.*\"]\nrole = \"operate\"\noutlets.\"1\".critical = true\n",
	);
	let mut st = Station::new("offers", &config.sources["offers"]);
	st.polled(station_vars("OL"), at(1000));
	// Nothing until upsd has said what the station offers.
	assert!(st.snapshot(at(1000)).actions.is_empty());
	let cmd = |name: &str, desc: Option<&str>| CmdInfo { name: name.into(), desc: desc.map(String::from) };
	let cmds = vec![
		cmd("outlet.1.load.off", None),
		cmd("outlet.1.load.on", None),
		cmd("outlet.10.load.off", Some(" ")),
		cmd("beeper.enable", Some("Turn the beeper on")),
		cmd("load.off", Some("Turn off the load")),
		cmd("outlet.2.load.cycle", None),
	];
	let rw = ["experimental.output.xboost", "experimental.id", "ups.delay.start", "experimental.battery.charge.max"].map(String::from).to_vec();
	// Of its writable variables, upsd is asked about those the settings allow.
	assert_eq!(st.offers(cmds, rw), ["experimental.output.xboost", "experimental.id", "experimental.battery.charge.max"]);
	let snap = st.snapshot(at(1000));
	assert_eq!(snap.actions.len(), 1);
	let command = &snap.actions[0];
	assert_eq!((command.id.as_str(), command.name.as_str(), command.kind, command.role, command.confirm), ("nut.command.offers", "DELTA 3 Max Plus", "nut", Role::Operate, true));
	let crate::actions::ParamKind::Choice(choices) = &command.params[0].kind else { panic!("{:?}", command.params[0]) };
	let labels: Vec<(&str, &str)> = choices.iter().map(|(v, l)| (v.as_str(), l.as_str())).collect();
	assert_eq!(
		labels,
		[
			("outlet.1.load.off", "AC outlets, group 1: off"),
			("outlet.1.load.on", "AC outlets, group 1: on"),
			("outlet.10.load.off", "USB-C 1: off"),
			("load.off", "Turn off the load"),
			("outlet.2.load.cycle", "outlet.2.load.cycle"),
		]
	);
	assert_eq!(command.params[0].typed, ["outlet.1.load.off", "load.off"]);
	// A setting once upsd has described it: a list of values, or a range; neither isn't offered,
	// which the journal says once.
	let info = |kind: &[&str], values: &[&str], ranges: &[(&str, &str)]| VarInfo {
		desc: None,
		kind: kind.iter().map(|s| s.to_string()).collect(),
		enum_values: values.iter().map(|s| s.to_string()).collect(),
		ranges: ranges.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
	};
	st.described("experimental.battery.charge.max".into(), info(&["RW", "RANGE", "NUMBER"], &[], &[("50", "100")]));
	st.described("experimental.id".into(), info(&["RW", "STRING:32"], &[], &[]));
	let ids = |s: &Snapshot| s.actions.iter().map(|a| a.id.clone()).collect::<Vec<_>>();
	assert_eq!(ids(&st.snapshot(at(1000))), ["nut.command.offers", "nut.set.offers.experimental.battery.charge.max"]);
	st.described("experimental.output.xboost".into(), info(&["RW", "ENUM"], &["enabled", "disabled"], &[]));
	let snap = st.snapshot(at(1000));
	assert_eq!(ids(&snap), ["nut.command.offers", "nut.set.offers.experimental.battery.charge.max", "nut.set.offers.experimental.output.xboost"]);
	assert_eq!(snap.actions[1].name, "experimental.battery.charge.max");
	let said = recorder::tests::said("offers");
	assert_eq!(said.iter().filter(|l| l.contains("experimental.id takes neither a list of values nor a range")).count(), 1, "{said:?}");
	// They stay while upsd can't be reached.
	st.lost(Link::Down("connection refused".into()), at(1010));
	assert_eq!(ids(&st.snapshot(at(1010))).len(), 3);
}

#[test]
fn a_dashboard_keeps_its_shape_while_upsd_is_lost() {
	let config = source(
		r#"
[sources.eco]
kind = "nut"
ups = "ups"

[profiles.default]
dashboards = ["ups"]
header = ""

[dashboards.ups]
title = "{item.name}"
for_each = "ups"

[[dashboards.ups.widgets]]
id = "status"
type = "status"
value = "{item}.status"

[[dashboards.ups.widgets]]
id = "load"
type = "value"
value = "{item}.ups.load"
when = "{item}.ups.load"

[[dashboards.ups.widgets]]
id = "solar"
type = "value"
value = "{item}.experimental.input.dc.1.realpower"
when = "{item}.experimental.input.dc.1.realpower"

[[dashboards.ups.widgets]]
id = "list"
type = "list"
items = [{ label = "Charge", value = "{item}.battery.charge" }, { label = "Serial", value = "{item}.device.serial" }]
"#,
	);
	let mut st = Station::new("eco", &config.sources["eco"]);
	let caps = crate::actions::Caps::default();
	let build = |snap: Snapshot, role: Role| {
		let mut v = Values { t: 2_000_000, ..Default::default() };
		put(&mut v, &config, &[("eco".to_string(), Arc::new(snap))].into());
		let ui = crate::ui::build(&crate::ui::tests::input(&config, &v, &caps, role, "default", None));
		let ids: Vec<String> = ui.dashboards.iter().flat_map(|d| d.all_widgets().into_iter().map(|w| w.id.clone())).collect();
		let rows = ui.dashboard("ups.eco").map(|d| d.widgets.iter().find(|w| w.id == "ups.eco/list").map_or(0, |l| l.def.items.as_ref().map_or(0, Vec::len))).unwrap_or(0);
		(crate::server::shape_of(&v, &caps), ids, rows, v)
	};
	let mut list = station_vars("OL");
	list.push(("device.serial".into(), "0000".into()));
	st.polled(list, at(1000));
	let (shape, ids, rows, _) = build(st.snapshot(at(1000)), Role::Admin);
	assert_eq!(ids, ["ups.eco/status", "ups.eco/load", "ups.eco/list"]);
	assert_eq!(rows, 2);
	// A reading the station leaves out for a while changes nothing.
	let mut fewer = station_vars("OL");
	fewer.retain(|(k, _)| k != "ups.load");
	st.polled(fewer, at(1010));
	let (again, ids_again, rows_again, v) = build(st.snapshot(at(1010)), Role::Admin);
	assert!(!v.scalars.contains_key("eco.ups.load"));
	assert_eq!((again, &ids_again, rows_again), (shape, &ids, rows));
	// Nor does upsd going away: its values are gone, the widgets stay.
	st.lost(Link::Down("connection refused".into()), at(1020));
	let (lost, ids_lost, rows_lost, v) = build(st.snapshot(at(1020)), Role::Admin);
	assert!(!v.scalars.contains_key("eco.battery.charge") && !v.scalars.contains_key("eco.status.since"));
	assert_eq!((v.scalars["eco.status"].v.clone(), v.scalars["eco.status"].level), (V::Text("Unreachable".into()), Some(Level::Critical)));
	assert_eq!(v.scalars["eco.status.power"].v, V::Text("none".into()));
	assert_eq!(v.scalars["eco.link.error"].v, V::Text("connection refused".into()));
	assert_eq!((lost, &ids_lost, rows_lost), (shape, &ids, rows));
	// The outlets stay, without what is read each time.
	let outlet = v.collections["outlets"].iter().find(|o| o.key == "eco.1").unwrap();
	assert_eq!((outlet.get("desc"), outlet.get("status"), outlet.get("realpower")), (Some(V::Text("AC outlets, group 1".into())), None, None));
	// What its role doesn't see doesn't hold a row for it.
	let view = View::new(&v, Role::Glance, Units::default());
	assert!(view.exists("eco.battery.charge") && !view.exists("eco.device.serial") && !view.exists("eco.never"));
	let (_, _, glance_rows, _) = build(st.snapshot(at(1020)), Role::Glance);
	assert_eq!(glance_rows, 1);
}

#[test]
fn the_status_has_a_level_of_its_own_in_the_stream() {
	let config = source(
		"[sources.eco]\nkind = \"nut\"\nups = \"ups\"\n[profiles.default]\ndashboards = [\"ups\"]\nheader = \"\"\n\
		 [dashboards.ups]\ntitle = \"UPS\"\nfor_each = \"ups\"\n\
		 [[dashboards.ups.widgets]]\nid = \"s\"\ntype = \"status\"\nvalue = \"{item}.status\"\n\
		 [[dashboards.ups.widgets]]\nid = \"m\"\ntype = \"status\"\nvalue = \"{item}.status\"\nlevels = { \"On line\" = \"warning\" }\n",
	);
	let mut st = Station::new("eco", &config.sources["eco"]);
	st.polled(station_vars("OB LB"), at(1000));
	let mut v = Values { t: 1_000_000, ..Default::default() };
	put(&mut v, &config, &[("eco".to_string(), Arc::new(st.snapshot(at(1000))))].into());
	let caps = crate::actions::Caps::default();
	let ui = crate::ui::build(&crate::ui::tests::input(&config, &v, &caps, Role::Glance, "default", None));
	let sorts = std::collections::HashMap::new();
	let s = crate::states::States { view: View::new(&v, Role::Glance, ui.units), control: false, sorts: &sorts, read: crate::ui::Sources::ALL };
	let states = crate::states::widgets(ui.dashboard("ups.eco").unwrap(), &s);
	assert_eq!(states["ups.eco/s"], serde_json::json!({ "v": "Low battery", "level": "critical" }));
	// A widget's `levels` say instead.
	assert_eq!(states["ups.eco/m"], serde_json::json!({ "v": "Low battery", "level": "normal" }));
	// What shows a source's value asks for upsd to be read often.
	assert!(ui.dashboard("ups.eco").unwrap().needs().sources);
}

// ---- on the demo upsd -----------------------------------------------------------------------

fn demo_source(id: &str, ups: &str, host: &str, port: u16, extra: &str) -> String {
	format!("[sources.{id}]\nkind = \"nut\"\nups = \"{ups}\"\nhost = \"{host}\"\nport = {port}\ninterval_ms = 200\nbackground_ms = 1000\n{extra}")
}

/// Waits up to `limit` for `ok` of the source `id`'s snapshot.
async fn wait_for(rt: &Runtime, id: &str, limit: Duration, ok: impl Fn(&Snapshot) -> bool) -> Arc<Snapshot> {
	let start = Instant::now();
	loop {
		let s = rt.snapshots().get(id).cloned();
		if let Some(s) = s.filter(|s| ok(s)) {
			return s;
		}
		assert!(start.elapsed() < limit, "{id}: not in {limit:?}: {:?}", rt.snapshots().get(id).map(|s| s.scalars.get(&format!("{id}.status")).map(|x| x.v.clone())));
		tokio::time::sleep(Duration::from_millis(50)).await;
	}
}

fn text(s: &Snapshot, a: &str) -> Option<String> {
	match s.scalars.get(a).map(|s| &s.v) {
		Some(V::Text(t)) => Some(t.clone()),
		_ => None,
	}
}

/// The polls a source takes in over `d`.
async fn polls_over(rt: &Runtime, id: &str, d: Duration) -> u64 {
	let before = rt.snapshots()[id].polls;
	tokio::time::sleep(d).await;
	rt.snapshots()[id].polls - before
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sources_on_the_demo_upsd_are_asked_as_often_as_wanted() {
	let _one = POLLERS.lock().await;
	let server = demo::Server::spawn().unwrap();
	let port = server.addr().port();
	let config = source(&format!("{}{}", demo_source("rack", "rack", "127.0.0.1", port, ""), demo_source("pack", "pack", "127.0.0.1", port, "history = false\n")));
	let history = Scratch::new();
	let rt = Runtime::default();
	rt.start(tokio::runtime::Handle::current(), &config, history.store());
	for id in ["rack", "pack"] {
		let s = wait_for(&rt, id, Duration::from_secs(5), |s| s.scalars.contains_key(&format!("{id}.battery.charge"))).await;
		assert_eq!(text(&s, &format!("{id}.status")).as_deref(), Some("On line"), "{id}");
		assert_eq!(text(&s, &format!("{id}.link")).as_deref(), Some("up"));
		assert_eq!(s.supply, Supply::Line);
	}
	let rack = rt.snapshots()["rack"].clone();
	assert_eq!(rack.scalars["rack.ups.realpower"].unit, "W");
	assert_eq!(text(&rack, "rack.outlet.1.desc").as_deref(), Some("Servers"));
	assert_eq!(rack.item.get("name"), Some(&V::Text("Smart 1500 RM (simulated)".into())));
	// Shown on some client's screen: every 200 ms.
	rt.set_demand(true, &BTreeSet::new(), &BTreeSet::new());
	let live = polls_over(&rt, "rack", Duration::from_millis(2000)).await;
	assert!(live >= 6, "{live} polls in 2 s while shown");
	// Nobody looks: rack's history is kept, every second; pack's isn't, nor is it graphed: not asked.
	rt.set_demand(false, &BTreeSet::new(), &BTreeSet::new());
	tokio::time::sleep(Duration::from_millis(300)).await;
	let (rack, pack) = tokio::join!(polls_over(&rt, "rack", Duration::from_millis(2500)), polls_over(&rt, "pack", Duration::from_millis(2500)));
	assert!((1..=4).contains(&rack), "{rack} polls in 2.5 s in the background");
	assert_eq!(pack, 0, "pack polled while nothing needs it");
	// A graph of pack's values has it asked again.
	rt.set_demand(false, &["pack.battery.charge".to_string()].into(), &BTreeSet::new());
	let pack = polls_over(&rt, "pack", Duration::from_millis(1600)).await;
	assert!(pack >= 1, "{pack} polls of a graphed source");
	drop(rt);
	server.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_station_that_loses_power_and_then_upsd() {
	let _one = POLLERS.lock().await;
	// The demo's rack goes on battery 20 s into its run: 19 s of it have passed.
	let (server, _) = demo::Server::spawn_with_history(19.0, 1.0).unwrap();
	let config = source(&demo_source("rack", "rack", "127.0.0.1", server.addr().port(), ""));
	let history = Scratch::new();
	let rt = Runtime::default();
	rt.start(tokio::runtime::Handle::current(), &config, history.store());
	rt.set_demand(true, &BTreeSet::new(), &BTreeSet::new());
	let s = wait_for(&rt, "rack", Duration::from_secs(30), |s| text(s, "rack.status").as_deref() == Some("On battery")).await;
	assert_eq!(s.supply, Supply::Battery);
	assert_eq!(s.scalars["rack.status"].level, Some(Level::Warning));
	// Its timeline: on line, then on battery since it went.
	let states: Vec<Supply> = s.timeline.iter().map(|c| c.state).collect();
	assert_eq!(states, [Supply::Line, Supply::Battery]);
	assert_eq!(s.since, Some(s.timeline[1].t));
	let now = s.timeline[1].t * 1000 + 5000;
	let segs = crate::history::timeline::segments(&s.timeline, now - 600_000, now);
	assert_eq!(segs.last().map(|g| (g.from, g.state)), Some((s.timeline[1].t * 1000, Supply::Battery)));
	server.stop();
	let s = wait_for(&rt, "rack", Duration::from_secs(10), |s| text(s, "rack.status").as_deref() == Some("Unreachable")).await;
	assert_eq!(text(&s, "rack.link").as_deref(), Some("down"));
	assert!(text(&s, "rack.link.error").is_some_and(|e| !e.is_empty()));
	assert!(!s.scalars.contains_key("rack.battery.charge") && s.known.contains_key("rack.battery.charge"));
	assert_eq!(s.supply, Supply::None);
	assert_eq!(s.timeline.last().map(|c| c.state), Some(Supply::None));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_timeline_is_kept_across_a_restart() {
	let _one = POLLERS.lock().await;
	let server = demo::Server::spawn().unwrap();
	let config = source(&demo_source("rack", "rack", "127.0.0.1", server.addr().port(), ""));
	let history = Scratch::new();
	let rt = Runtime::default();
	rt.start(tokio::runtime::Handle::current(), &config, history.store());
	let first = wait_for(&rt, "rack", Duration::from_secs(5), |s| s.supply == Supply::Line).await;
	let on = first.timeline[0].t;
	// Stopped in a later second than it was first read.
	tokio::time::sleep(Duration::from_millis(1100)).await;
	// Stopped, as the service does: its timeline ends, and when it was last read is kept.
	rt.stop().await;
	assert!(rt.snapshots().is_empty());
	let store = history.store();
	let file = std::fs::read_to_string(store.dir().join("timeline").join("rack")).unwrap();
	assert!(file.starts_with(&format!("{on} line -\n")) && file.ends_with(" none -\n") && file.lines().count() == 2, "{file}");
	store.flush(crate::store::now(), true);
	let last = Store::new(store.dir().to_path_buf()).last_seen("rack").expect("kept");
	assert!(last >= on);
	// Started again a second later at least: changes within one second are one.
	tokio::time::sleep(Duration::from_millis(1100)).await;
	let rt = Runtime::default();
	rt.start(tokio::runtime::Handle::current(), &config, history.store());
	let before: Vec<Supply> = rt.snapshots()["rack"].timeline.iter().map(|c| c.state).collect();
	assert_eq!(before, [Supply::Line, Supply::None]);
	let again = wait_for(&rt, "rack", Duration::from_secs(5), |s| s.supply == Supply::Line).await;
	let after: Vec<(u64, Supply)> = again.timeline.iter().map(|c| (c.t, c.state)).collect();
	assert_eq!((after.len(), after[0], after[2].1), (3, (on, Supply::Line), Supply::Line));
	// On line since it was read again, not since it was first.
	assert_eq!(again.since, Some(after[2].0));
	rt.stop().await;
	server.stop();
}

/// The threads named after a poller of `host`, as the kernel keeps their names (15 bytes).
fn pollers_of(host: &str) -> usize {
	let name: String = format!("upst {host}").chars().take(15).collect();
	let Ok(tasks) = std::fs::read_dir("/proc/self/task") else { return 0 };
	tasks.flatten().filter(|t| std::fs::read_to_string(t.path().join("comm")).is_ok_and(|c| c.trim_end() == name || c.starts_with(&name))).count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_source_taken_out_of_the_settings_stops_its_thread() {
	let _one = POLLERS.lock().await;
	let server = demo::Server::spawn().unwrap();
	// Only this test asks `localhost`: its poller's thread is the only one of that name.
	let config = source(&demo_source("rack", "rack", "localhost", server.addr().port(), ""));
	let history = Scratch::new();
	let rt = Runtime::default();
	rt.start(tokio::runtime::Handle::current(), &config, history.store());
	wait_for(&rt, "rack", Duration::from_secs(5), |s| s.polls > 0).await;
	assert_eq!(pollers_of("localhost"), 1);
	// Changed, it starts again: still one.
	let changed = source(&demo_source("rack", "rack", "localhost", server.addr().port(), "capacity_wh = 500\n"));
	rt.apply(&changed);
	let start = Instant::now();
	wait_for(&rt, "rack", Duration::from_secs(5), |s| s.polls > 0).await;
	while pollers_of("localhost") > 1 {
		assert!(start.elapsed() < Duration::from_secs(7), "the changed source's old thread is still there");
		tokio::time::sleep(Duration::from_millis(50)).await;
	}
	rt.apply(&Config::default());
	assert!(rt.snapshots().is_empty());
	let start = Instant::now();
	while pollers_of("localhost") > 0 {
		assert!(start.elapsed() < Duration::from_secs(7), "a source taken out still has its thread");
		tokio::time::sleep(Duration::from_millis(50)).await;
	}
	server.stop();
}

#[test]
fn elamond_values_asks_upsd_itself() {
	let server = demo::Server::spawn().unwrap();
	let config = source(&format!(
		"{}user = \"admin\"\npassword_file = \"/nonexistent/password\"\ncommands = [\"beeper.*\"]\nsettings = [\"battery.charge.low\"]\n",
		demo_source("rack", "rack", "127.0.0.1", server.addr().port(), "")
	));
	let out = cli::values(&config, "rack").unwrap();
	let line = |start: &str| out.lines().find(|l| l.starts_with(start)).unwrap_or_else(|| panic!("no {start} in\n{out}")).split_whitespace().collect::<Vec<_>>();
	assert_eq!(line("address"), ["address", "value", "unit", "format", "role"]);
	assert_eq!(line("rack.battery.charge.low "), ["rack.battery.charge.low", "40%", "%", "percent", "glance"]);
	assert_eq!(line("rack.device.serial "), ["rack.device.serial", "DEMO-0001", "text", "monitor"]);
	assert_eq!(line("rack.status "), ["rack.status", "On", "line", "text", "glance"]);
	assert_eq!(line("rack.ups.realpower ")[2..], ["W", "W", "watts", "glance"]);
	assert!(out.contains("Commands on this station:\n"), "{out}");
	assert_eq!(line("  beeper.mute").last(), Some(&"allowed"));
	assert_eq!(&line("  load.off")[line("  load.off").len() - 3..], ["not", "in", "commands"]);
	assert!(out.contains("Writable settings:\n"), "{out}");
	assert_eq!(line("  battery.charge.low"), ["battery.charge.low", "RANGE", "10–90", "allowed"]);
	assert_eq!(line("  ups.beeper.status"), ["ups.beeper.status", "ENUM", "enabled,", "disabled,", "muted", "not", "in", "settings"]);
	assert!(out.ends_with("the timeline come from the running service.\n"), "{out}");
	assert!(cli::values(&config, "nope").unwrap_err().starts_with("no source \"nope\" (the settings have rack)"));
	server.stop();
	let gone = cli::values(&config, "rack").unwrap_err();
	assert!(gone.starts_with("can't reach upsd at 127.0.0.1:"), "{gone}");
}

// ---- with a recorder ------------------------------------------------------------------------

/// A port of 127.0.0.1 nothing listens on.
fn closed_port() -> u16 {
	std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn owned(list: &[(&str, String)]) -> Vec<(String, String)> {
	list.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

/// Waits up to 10 s for the source `id` to have said `n` lines of its recorder.
async fn said(id: &str, n: usize) -> Vec<String> {
	let start = Instant::now();
	loop {
		let said = recorder::tests::said(id);
		if said.len() >= n {
			return said;
		}
		assert!(start.elapsed() < Duration::from_secs(10), "{id} said {said:?}");
		tokio::time::sleep(Duration::from_millis(50)).await;
	}
}

/// The size of each file of the long history in `dir`.
fn sizes(dir: &std::path::Path) -> Vec<(String, u64)> {
	let mut out = Vec::new();
	for sub in ["10s", "5m", "1h", "timeline"] {
		for e in std::fs::read_dir(dir.join(sub)).into_iter().flatten().flatten() {
			out.push((format!("{sub}/{}", e.file_name().to_string_lossy()), e.metadata().unwrap().len()));
		}
	}
	out.sort();
	out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_recorder_fills_in_the_history_and_the_timeline() {
	use crate::history::disk::Tier;
	use upst_core::journal::Journal;
	let _one = POLLERS.lock().await;
	// upsd isn't there: what the source has of its past is the recorder's journal.
	let port = closed_port();
	let label = format!("rack@127.0.0.1:{port}");
	let journals = crate::history::disk::tests::scratch("journals");
	let now = crate::store::now();
	let t0 = (now - 5 * 3600) / 3600 * 3600;
	let poll = |status: &str, charge: u64, watts: u64| {
		owned(&[("ups.status", status.into()), ("battery.charge", charge.to_string()), ("ups.realpower", watts.to_string()), ("device.serial", "0000".into())])
	};
	// An hour on line, a poll every 10 s, its power going back and forth.
	let mut j = Journal::open(&journals, &label, t0 as f64).unwrap();
	for k in 0..360 {
		j.poll((t0 + k * 10) as f64, &poll("OL", 100, 200 + k % 2)).unwrap();
		j.tick((t0 + k * 10) as f64).unwrap();
	}
	drop(j);
	// Two hours the recorder didn't run; then half an hour on battery and half an hour on line.
	let t3 = t0 + 3 * 3600;
	let mut j = Journal::open(&journals, &label, t3 as f64).unwrap();
	for k in 0..360 {
		let (status, charge) = if k < 180 { ("OB DISCHRG", 100 - k / 10) } else { ("OL", 82) };
		j.poll((t3 + k * 10) as f64, &poll(status, charge, 300 + k % 2)).unwrap();
		j.tick((t3 + k * 10) as f64).unwrap();
	}
	let path = j.path.clone();
	drop(j);
	let rec = recorder::tests::Recorder::serve(vec![(label.clone(), path)]);
	let config = source(&demo_source("past", "rack", "127.0.0.1", port, &format!("recorder = \"{}\"\n", rec.addr)));
	let history = Scratch::new();
	let store = history.store();
	let rt = Runtime::default();
	rt.start(tokio::runtime::Handle::current(), &config, store.clone());
	let lines = said("past", 1).await;
	assert_eq!(lines, [format!("history filled in from the recorder's {label}: all it had before this start")]);
	// On line, nothing known over the gap, on battery, on line, nothing known since the journal ends.
	let s = rt.snapshots()["past"].clone();
	let changes: Vec<(u64, Supply)> = s.timeline.iter().map(|c| (c.t, c.state)).collect();
	assert_eq!(changes, [(t0, Supply::Line), (t0 + 3590, Supply::None), (t3, Supply::Battery), (t3 + 1800, Supply::Line), (t3 + 3590, Supply::None)]);
	// A point every 10 s, each the poll's; none over the gap; the hours their means.
	let slot = |tier: Tier, t: u64| tier.slot(t);
	let s10 = store.points(Tier::S10, "past.ups.realpower", slot(Tier::S10, t0)..slot(Tier::S10, t0 + 3600));
	assert_eq!((s10.len(), s10[0], s10[1], s10[359].0), (360, (slot(Tier::S10, t0), 200.0), (slot(Tier::S10, t0) + 1, 201.0), slot(Tier::S10, t0 + 3590)));
	assert!(store.points(Tier::S10, "past.ups.realpower", slot(Tier::S10, t0 + 3600)..slot(Tier::S10, t3)).is_empty());
	let h1 = store.points(Tier::H1, "past.ups.realpower", slot(Tier::H1, t0)..slot(Tier::H1, t0) + 5);
	assert_eq!(h1, [(slot(Tier::H1, t0), 200.5), (slot(Tier::H1, t3), 300.5)]);
	// The charge, which the journal writes as it changes, at every poll between too: 100, 99, 98.
	assert_eq!(store.points(Tier::M5, "past.battery.charge", slot(Tier::M5, t3)..slot(Tier::M5, t3) + 1), [(slot(Tier::M5, t3), 99.0)]);
	assert!(store.points(Tier::H1, "past.device.serial", 0..u32::MAX).is_empty());
	// Filled in up to the start: that is where the next start goes on from.
	rt.stop().await;
	store.flush(crate::store::now(), true);
	let until = Store::new(store.dir().to_path_buf()).last_seen("past").unwrap();
	assert!((now..=crate::store::now()).contains(&until), "{until}");
	// Started again, with nothing new in the journal: nothing is added.
	let before = sizes(store.dir());
	let timeline = std::fs::read_to_string(store.dir().join("timeline").join("past")).unwrap();
	assert_eq!(timeline.lines().count(), 5);
	let again = Arc::new(Store::new(store.dir().to_path_buf()));
	let rt = Runtime::default();
	rt.start(tokio::runtime::Handle::current(), &config, again.clone());
	let lines = said("past", 2).await;
	assert!(lines[1].starts_with(&format!("history filled in from the recorder's {label}: the ")), "{lines:?}");
	rt.stop().await;
	again.flush(crate::store::now(), true);
	assert_eq!(sizes(store.dir()), before);
	assert_eq!(std::fs::read_to_string(store.dir().join("timeline").join("past")).unwrap(), timeline);
	drop(rec);
	std::fs::remove_dir_all(&journals).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_recorder_out_of_reach_is_a_line_and_the_source_works_without() {
	use upst_core::journal::Journal;
	let _one = POLLERS.lock().await;
	let server = demo::Server::spawn().unwrap();
	let ups = server.addr().port();
	// One that answers, but knows another UPS.
	let journals = crate::history::disk::tests::scratch("journals-other");
	let mut j = Journal::open(&journals, "pack@nas", 1_700_000_000.0).unwrap();
	j.poll(1_700_000_000.0, &owned(&[("ups.status", "OL".into())])).unwrap();
	let path = j.path.clone();
	drop(j);
	let rec = recorder::tests::Recorder::serve(vec![("pack@nas".into(), path)]);
	let closed = closed_port();
	let config = source(&format!(
		"{}{}",
		demo_source("lone", "rack", "127.0.0.1", ups, &format!("recorder = \"127.0.0.1:{closed}\"\n")),
		demo_source("other", "rack", "127.0.0.1", ups, &format!("recorder = \"{}\"\n", rec.addr)),
	));
	let history = Scratch::new();
	let rt = Runtime::default();
	rt.start(tokio::runtime::Handle::current(), &config, history.store());
	for id in ["lone", "other"] {
		let s = wait_for(&rt, id, Duration::from_secs(5), |s| s.supply == Supply::Line).await;
		assert!(s.scalars.contains_key(&format!("{id}.battery.charge")));
	}
	let lone = said("lone", 1).await;
	assert_eq!(lone.len(), 1, "{lone:?}");
	assert!(lone[0].starts_with(&format!("recorder 127.0.0.1:{closed}: ")) && lone[0].ends_with("; no history from before elamond"), "{lone:?}");
	let other = said("other", 1).await;
	assert_eq!(other, [format!("recorder {}: it has no journal of rack@127.0.0.1:{ups} (it has pack@nas); no history from before elamond", rec.addr)]);
	// Read live, its last reading is kept: the next start fills in from there.
	let store = history.store();
	store.flush(crate::store::now(), true);
	assert!(Store::new(store.dir().to_path_buf()).last_seen("lone").is_some());
	tokio::time::sleep(Duration::from_millis(300)).await;
	assert_eq!(recorder::tests::said("lone").len(), 1);
	rt.stop().await;
	server.stop();
	std::fs::remove_dir_all(&journals).unwrap();
}

#[test]
fn the_last_hour_of_a_journal_primes_the_estimate() {
	use upst_core::journal::Kind;
	let config = eco();
	let until = 1_700_000_000;
	let rec = |t: u64, kind: Kind| Record::new(t as f64, kind);
	let set = |t: u64, k: &str, v: String| rec(t, Kind::Set(k.into(), v));
	let start = until - 3600;
	let mut records = vec![rec(start, Kind::Start), rec(start, Kind::Up), set(start, "ups.status", "OB DISCHRG".into()), set(start, "battery.charge.low", "20".into())];
	// Down a percent every 4 minutes.
	for k in 0..60 {
		records.push(set(start + k * 60, "battery.charge", (95 - k / 4).to_string()));
	}
	let live = |st: &mut Station| {
		st.polled(vars(&[("ups.status", "OB DISCHRG"), ("battery.charge", "80"), ("battery.charge.low", "20")]), at(until));
		st.snapshot(at(until))
	};
	let mut cold = Station::new("eco", &config.sources["eco"]);
	assert!(!live(&mut cold).scalars.contains_key("eco.estimate.to_empty"));
	let mut warm = Station::new("eco", &config.sources["eco"]);
	warm.warm(&records, at(until), None);
	let s = live(&mut warm);
	let to_empty = s.scalars["eco.estimate.to_empty"].v.num().unwrap();
	// 80 % at a percent every 240 s.
	assert!((15_000.0..25_000.0).contains(&to_empty), "{to_empty}");
	assert!(s.scalars.contains_key("eco.estimate.to_low"));
}
