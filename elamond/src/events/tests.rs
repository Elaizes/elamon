//! The rules against readings with a time of their own, the events' file, `elamond notify`'s
//! spool, and the targets against a server on a loopback port: nothing here asks the network.

use std::collections::VecDeque;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::def::Lvl;
use super::rules::{Engine, Heard};
use super::*;
use crate::settings::{self, Config, Source};
use crate::values::{Values, V};

fn scratch(name: &str) -> PathBuf {
	let dir = std::env::temp_dir().join(format!("elamond-events-{name}-{}", data_encoding::HEXLOWER.encode(&crate::store::random_bytes::<6>())));
	std::fs::create_dir_all(&dir).unwrap();
	dir
}

/// The settings elamond starts with, and `more` over them.
fn config(more: &str) -> Config {
	settings::read_sources(&[Source::new("defaults", settings::DEFAULTS), Source::new("more.toml", more)]).unwrap_or_else(|m| panic!("{}", settings::join_mistakes(&m)))
}

/// A reading at `secs` with these values.
fn at(secs: u64, values: &[(&str, V)]) -> Values {
	let mut v = Values { t: secs * 1000, ..Default::default() };
	for (address, value) in values {
		v.put(address.to_string(), Some(value.clone()));
	}
	v
}

fn told(drafts: &[Draft]) -> Vec<(Phase, &str, &str, &str)> {
	drafts.iter().map(|d| (d.phase, d.level.as_str(), d.text.as_str(), d.plain.as_str())).collect()
}

fn disk(percent: f64) -> [(&'static str, V); 1] {
	[("disk.data.percent", V::Float(percent))]
}

#[test]
fn a_rule_turns_once_after_its_time_and_back_past_its_margin() {
	let dir = scratch("disk");
	let c = config("");
	let mut e = Engine::load(&dir);
	let tick = |e: &mut Engine, secs: u64, p: f64| e.tick(&c, &at(secs, &disk(p)), &[], "desk", None);
	assert!(tick(&mut e, 0, 50.0).is_empty());
	// Over the limit, and for less than `for`: nothing yet; back under meanwhile starts it anew.
	assert!(tick(&mut e, 10, 95.0).is_empty());
	assert!(tick(&mut e, 40, 89.0).is_empty());
	assert!(tick(&mut e, 50, 95.0).is_empty());
	assert!(tick(&mut e, 109, 95.0).is_empty());
	let began = tick(&mut e, 110, 95.4);
	assert_eq!(told(&began), [(Phase::Start, "warning", "Disk data is 95% full", "A disk is 95% full")]);
	assert_eq!((began[0].code.as_str(), began[0].rule.as_str(), began[0].of), ("disk.full", "disk", Lvl::Warning));
	// Every tick after tells nothing, however long; nor does a little room (`clear` is 2).
	for secs in 111..200 {
		assert!(tick(&mut e, secs, 96.0).is_empty());
	}
	assert!(tick(&mut e, 200, 89.0).is_empty());
	assert!(tick(&mut e, 201, 91.0).is_empty());
	let ended = tick(&mut e, 300, 80.0);
	assert_eq!(told(&ended), [(Phase::End, "info", "Disk data has room again: 80% full", "The disk has room again")]);
	assert_eq!(ended[0].of, Lvl::Warning);
	assert!(tick(&mut e, 301, 80.0).is_empty());
	std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_restart_and_new_settings_tell_nothing_twice() {
	let dir = scratch("restart");
	let c = config("[rules.disk]\nfor = 0\n");
	let mut e = Engine::load(&dir);
	// The first reading finds the trouble there already: one event.
	assert_eq!(e.tick(&c, &at(0, &disk(99.0)), &[], "desk", None).len(), 1);
	e.save();
	// The service starts again, the disk as full: nothing.
	let mut e = Engine::load(&dir);
	assert!(e.tick(&c, &at(5, &disk(99.0)), &[], "desk", None).is_empty());
	// The settings are read again, the rule's words changed: nothing; its limit raised past the
	// disk: the trouble ends.
	let reworded = config("[rules.disk]\nfor = 0\ntext = \"{name} full\"\n");
	assert!(e.tick(&reworded, &at(6, &disk(99.0)), &[], "desk", None).is_empty());
	let raised = config("[rules.disk]\nfor = 0\nabove = 99.5\nclear = 0\n");
	assert_eq!(told(&e.tick(&raised, &at(7, &disk(99.0)), &[], "desk", None))[0].0, Phase::End);
	e.save();
	// A state file that doesn't read starts the rules calm, and is written anew.
	std::fs::write(dir.join(super::rules::STATE_FILE), "{not json").unwrap();
	let mut e = Engine::load(&dir);
	assert_eq!(e.tick(&c, &at(8, &disk(99.0)), &[], "desk", None).len(), 1);
	std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_value_gone_is_no_end_and_a_rule_gone_is_forgotten() {
	let dir = scratch("gone");
	let c = config("[rules.disk]\nfor = 0\n");
	let mut e = Engine::load(&dir);
	assert_eq!(e.tick(&c, &at(0, &disk(99.0)), &[], "desk", None).len(), 1);
	// Unmounted, or its source down: neither an end nor, once back and as full, a second start.
	assert!(e.tick(&c, &at(1, &[]), &[], "desk", None).is_empty());
	assert!(e.tick(&c, &at(2, &disk(99.0)), &[], "desk", None).is_empty());
	// Turned off, it tells nothing and forgets; on again, it finds the trouble anew.
	let off = config("[rules.disk]\nfor = 0\nenabled = false\n");
	assert!(e.tick(&off, &at(3, &disk(99.0)), &[], "desk", None).is_empty());
	assert!(e.tick(&off, &at(4, &disk(50.0)), &[], "desk", None).is_empty());
	assert_eq!(e.tick(&c, &at(5, &disk(99.0)), &[], "desk", None).len(), 1);
	// The settings without the rules elamond starts with make no events at all: none is built in.
	let bare = settings::read_sources(&[Source::new("bare.toml", "schema = 3\n")]).unwrap();
	assert!(bare.rules.is_empty());
	let mut e = Engine::load(&scratch("bare"));
	let trouble = [("disk.data.percent", V::Float(100.0)), ("cpu.temp", V::Float(120.0)), ("battery.percent", V::Float(1.0)), ("power.on_ac", V::Bool(false))];
	assert!(e.tick(&bare, &at(0, &trouble), &[], "desk", None).is_empty());
	assert!(e.tick(&bare, &at(1000, &trouble), &[], "desk", None).is_empty());
	std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn levels_other_values_and_things_by_their_keys() {
	let dir = scratch("kinds");
	let c = config("[rules.cpu-temp]\nfor = 0\nclear_for = 0\n[rules.power-saver]\nenabled = true\n[links.web]\nname = \"Web\"\nurl = \"http://{host}:8080\"\nunit = \"web.service\"\n");
	let mut e = Engine::load(&dir);
	let tick = |e: &mut Engine, secs: u64, v: &[(&str, V)]| e.tick(&c, &at(secs, v), &[], "desk", None);
	// `when`: the level the service judges the value by itself (a temperature is critical at 95).
	assert!(tick(&mut e, 0, &[("cpu.temp", V::Float(80.0))]).is_empty());
	assert_eq!(told(&tick(&mut e, 1, &[("cpu.temp", V::Float(96.0))])), [(Phase::Start, "warning", "The CPU is at 96°C", "The CPU is at 96°C")]);
	assert_eq!(told(&tick(&mut e, 2, &[("cpu.temp", V::Float(70.0))]))[0].0, Phase::End);
	// The battery low only counts on battery (`also`).
	let battery = |percent: f64, ac: bool| [("battery.percent", V::Float(percent)), ("power.on_ac", V::Bool(ac))];
	assert!(tick(&mut e, 3, &battery(10.0, true)).is_empty());
	assert_eq!(told(&tick(&mut e, 4, &battery(10.0, false)))[0].2, "The battery is at 10%");
	assert_eq!(told(&tick(&mut e, 5, &battery(10.0, true)))[0].0, Phase::End);
	// A unit that failed, each link by its key; the name is in `text` alone.
	let units = |a: &str, b: &str| [("link.web.unit_state", V::Text(a.into())), ("link.sync.unit_state", V::Text(b.into()))];
	assert!(tick(&mut e, 6, &units("active", "inactive")).is_empty());
	assert_eq!(told(&tick(&mut e, 7, &units("failed", "active"))), [(Phase::Start, "warning", "The unit of web failed", "A unit failed")]);
	assert_eq!(told(&tick(&mut e, 8, &units("failed", "failed"))), [(Phase::Start, "warning", "The unit of sync failed", "A unit failed")]);
	assert_eq!(told(&tick(&mut e, 9, &units("active", "failed"))), [(Phase::End, "info", "The unit of web is active again", "The unit runs again")]);
	// A text that equals, at the level its rule gives it.
	assert_eq!(told(&tick(&mut e, 10, &[("power.profile", V::Text("power-saver".into()))]))[0].1, "info");
	std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn again_keeps_a_flapping_thing_quiet() {
	let dir = scratch("again");
	let c = config("[rules.disk]\nfor = 0\nclear = 0\nagain = 600\n");
	let mut e = Engine::load(&dir);
	let tick = |e: &mut Engine, secs: u64, p: f64| e.tick(&c, &at(secs, &disk(p)), &[], "desk", None).len();
	assert_eq!([tick(&mut e, 1, 95.0), tick(&mut e, 10, 50.0)], [1, 1]);
	// Within ten minutes of its last, neither the trouble nor its end is told.
	assert_eq!([tick(&mut e, 20, 95.0), tick(&mut e, 30, 50.0), tick(&mut e, 40, 95.0), tick(&mut e, 50, 50.0)], [0; 4]);
	assert_eq!([tick(&mut e, 700, 95.0), tick(&mut e, 710, 50.0)], [1, 1]);
	std::fs::remove_dir_all(dir).unwrap();
}

fn heard(code: &str, secs: u64) -> Heard {
	Heard { source: "eco".into(), code: code.into(), text: format!("upst says {code}"), t: secs * 1000 }
}

const STATION: &str = "[sources.eco]\nkind = \"nut\"\nups = \"eco\"\n";

#[test]
fn a_ups_goes_by_its_codes() {
	let dir = scratch("ups");
	let c = config(STATION);
	let mut e = Engine::load(&dir);
	let tick = |e: &mut Engine, secs: u64, heard: &[Heard]| e.tick(&c, &at(secs, &[]), heard, "desk", None);
	// What no rule names is no event.
	assert!(tick(&mut e, 0, &[heard("input.boost", 0), heard("status.flag.set", 0)]).is_empty());
	let lost = tick(&mut e, 10, &[heard("power.lost", 10)]);
	assert_eq!(told(&lost), [(Phase::Start, "warning", "eco is on battery", "The UPS is on battery")]);
	assert_eq!(lost[0].code, "power.lost");
	// Said again (the source started anew, the service too): the same trouble.
	e.save();
	let mut e = Engine::load(&dir);
	assert!(tick(&mut e, 20, &[heard("power.started_on_battery", 20)]).is_empty());
	let low = tick(&mut e, 600, &[heard("battery.low", 600)]);
	assert_eq!(told(&low), [(Phase::Start, "critical", "eco: the battery is low", "The UPS's battery is low")]);
	let back = tick(&mut e, 3610, &[heard("power.restored", 3610), heard("battery.low.cleared", 3610)]);
	assert_eq!(told(&back)[0], (Phase::End, "info", "eco: the power is back after 1 h 0 min", "The power is back after 1 h 0 min"));
	assert_eq!((back[0].of, back[1].of, back[1].code.as_str()), (Lvl::Warning, Lvl::Critical, "battery.low.cleared"));
	// Another source's is its own; a rule of one source hears only that one.
	let only = config(&format!("{STATION}[rules.ups-battery]\nsource = \"eco\"\n"));
	let other = Heard { source: "rack".into(), ..heard("power.lost", 3700) };
	assert!(e.tick(&only, &at(3700, &[]), std::slice::from_ref(&other), "desk", None).is_empty());
	assert_eq!(told(&tick(&mut e, 3701, &[other]))[0].2, "rack is on battery");
	std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_trouble_kept_from_before_a_start_ends_when_the_source_no_longer_tells_of_it() {
	use super::rules::WATCHING;
	let dir = scratch("watching");
	let c = config(STATION);
	let tick = |e: &mut Engine, secs: u64, heard: &[Heard]| e.tick(&c, &at(secs, &[]), heard, "desk", None);
	let mut e = Engine::load(&dir);
	assert_eq!(tick(&mut e, 10, &[heard("power.lost", 10), heard("battery.low", 10)]).len(), 2);
	e.save();
	// The computer went down with the power and is back: the station, read for the first time,
	// is on line and has nothing to tell. What was kept ends there, in silence, and the next cut
	// is told as any.
	let mut e = Engine::load(&dir);
	assert!(tick(&mut e, 5000, &[heard(WATCHING, 5000)]).is_empty());
	assert_eq!(told(&tick(&mut e, 9000, &[heard("power.lost", 9000)]))[0].2, "eco is on battery");
	assert_eq!(told(&tick(&mut e, 9060, &[heard("power.restored", 9060)]))[0].2, "eco: the power is back after 1 min");
	// Started anew while still on battery, the station says so first: the same trouble, no event.
	assert_eq!(tick(&mut e, 9100, &[heard("power.lost", 9100)]).len(), 1);
	e.save();
	let mut e = Engine::load(&dir);
	assert!(tick(&mut e, 9200, &[heard("power.started_on_battery", 9200), heard(WATCHING, 9200)]).is_empty());
	assert_eq!(told(&tick(&mut e, 9300, &[heard("power.restored", 9300)]))[0].0, Phase::End);
	// Nothing is kept of what is calm and has nothing to remember.
	let c = config("[rules.disk]\nfor = 0\n");
	let mut e = Engine::load(&scratch("calm"));
	let many: Vec<(String, V)> = (0..50).map(|i| (format!("disk.d{i}.percent"), V::Float(if i == 7 { 99.0 } else { 10.0 }))).collect();
	let many: Vec<(&str, V)> = many.iter().map(|(a, v)| (a.as_str(), v.clone())).collect();
	assert_eq!(e.tick(&c, &at(1, &many), &[], "desk", None).len(), 1);
	assert_eq!(e.tick(&c, &at(2, &many), &[], "desk", None).len(), 0);
	e.save();
	let kept: serde_json::Value = serde_json::from_slice(&std::fs::read(e_path(&e)).unwrap()).unwrap();
	assert_eq!(kept["states"].as_object().unwrap().keys().collect::<Vec<_>>(), ["disk/d7"]);
	// What a value holds is never read as a placeholder.
	assert_eq!(super::rules::fill("{value} on {computer}{", &[("value", "{computer} {name}"), ("computer", "desk")]), "{computer} {name} on desk{");
	// A UPS a rule hears, or looks at, is one to ask on with nobody looking.
	let needs = def::Needs::of(&config(&format!("{STATION}[sources.rack]\nkind = \"nut\"\nups = \"rack\"\n[rules.ups-battery]\nsource = \"eco\"\n[rules.ups-low]\nenabled = false\n[rules.ups-link]\nenabled = false\n")));
	assert_eq!(needs.ups.iter().collect::<Vec<_>>(), ["eco"]);
	assert_eq!(def::Needs::of(&config(STATION)).ups.len(), 1);
	std::fs::remove_dir_all(dir).unwrap();
}

fn e_path(e: &Engine) -> PathBuf {
	e.path().to_path_buf()
}

#[test]
fn upsd_back_is_told_only_after_its_loss_was() {
	let dir = scratch("upsd");
	let c = config(STATION);
	let mut e = Engine::load(&dir);
	let tick = |e: &mut Engine, c: &Config, secs: u64, heard: &[Heard]| e.tick(c, &at(secs, &[]), heard, "desk", None);
	// The first contact after a start upsd wasn't there for, within `for`: nothing, either way.
	assert!(tick(&mut e, &c, 0, &[heard("upsd.unreachable", 0)]).is_empty());
	assert!(tick(&mut e, &c, 30, &[heard("upsd.back", 30)]).is_empty());
	assert!(tick(&mut e, &c, 100, &[]).is_empty());
	// Never lost, as far as the rule saw: its return alone is nothing.
	assert!(tick(&mut e, &c, 110, &[heard("upsd.back", 110)]).is_empty());
	// Gone for longer than `for`: told, and then its return is.
	assert!(tick(&mut e, &c, 200, &[heard("upsd.lost", 200)]).is_empty());
	assert!(tick(&mut e, &c, 259, &[]).is_empty());
	assert_eq!(told(&tick(&mut e, &c, 260, &[]))[0].2, "eco: upsd doesn't answer");
	assert_eq!(told(&tick(&mut e, &c, 500, &[heard("upsd.back", 500)])), [(Phase::End, "info", "eco: upsd answers again after 5 min", "The UPS's server answers again after 5 min")]);
	// `ends_always` tells a return whenever it comes; a rule without `ends` tells each code alone.
	let always = config(&format!("{STATION}[rules.ups-link]\nends_always = true\n[rules.boost]\nkind = \"ups\"\ncodes = [\"input.boost\"]\nlevel = \"info\"\ntext = \"{{source}}: {{text}}\"\nplain = \"The UPS boosts its input\"\n"));
	assert_eq!(told(&tick(&mut e, &always, 600, &[heard("upsd.back", 600)]))[0].0, Phase::End);
	let boosts = tick(&mut e, &always, 700, &[heard("input.boost", 700), heard("input.boost", 701)]);
	assert_eq!(told(&boosts), [(Phase::Once, "info", "eco: upst says input.boost", "The UPS boosts its input"); 2]);
	std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_boot_is_told_once_and_not_a_restart() {
	let dir = scratch("boot");
	let c = config("[rules.boot]\nenabled = true\n");
	let tick = |e: &mut Engine, boot: &str| e.tick(&c, &at(0, &[]), &[], "desk", Some(boot)).len();
	// The first boot seen is where it counts from.
	let mut e = Engine::load(&dir);
	assert_eq!([tick(&mut e, "boot-1"), tick(&mut e, "boot-1")], [0, 0]);
	e.save();
	// The service started again in the same boot: nothing. In another: once.
	let mut e = Engine::load(&dir);
	assert_eq!(tick(&mut e, "boot-1"), 0);
	e.save();
	let mut e = Engine::load(&dir);
	let started = e.tick(&c, &at(0, &[]), &[], "desk", Some("boot-2"));
	assert_eq!(told(&started), [(Phase::Once, "info", "desk started", "The computer started")]);
	assert_eq!(tick(&mut e, "boot-2"), 0);
	std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn what_the_settings_say_is_what_the_rules_do() {
	let dir = scratch("settings");
	// A rule of one's own, on a source's value; a limit, a level, words and a code changed.
	let c = config(
		"[sources.queue]\nkind = \"command\"\ncommand = [\"true\"]\n\
		 [rules.backlog]\nvalue = \"queue.length\"\nabove = 100\nlevel = \"critical\"\ncode = \"queue.long\"\ntext = \"{rule} on {computer}: {value}\"\nplain = \"{rule}: {value}\"\nrecovered = false\npriority = 5\ntags = [\"inbox\"]\n\
		 [rules.disk]\nabove = 50\nfor = 0\nlevel = \"critical\"\ntext = \"{name}!\"\n",
	);
	let mut e = Engine::load(&dir);
	let mut values = at(0, &disk(60.0));
	values.scalars.insert("queue.length".into(), crate::feeds::scalar(V::Int(250)));
	let made = e.tick(&c, &values, &[], "desk", None);
	assert_eq!(told(&made), [(Phase::Start, "critical", "backlog on desk: 250", "backlog: 250"), (Phase::Start, "critical", "data!", "A disk is 60% full")]);
	assert_eq!((made[0].code.as_str(), made[0].priority, made[0].tags.clone()), ("queue.long", Some(5), Some(vec!["inbox".to_string()])));
	// `recovered = false`: the end is no event.
	values.scalars.insert("queue.length".into(), crate::feeds::scalar(V::Int(3)));
	values.t += 1000;
	assert!(e.tick(&c, &values, &[], "desk", None).is_empty());
	// What only the rules need read: the source, by its id; a link's unit and a GPU, when a rule looks.
	let needs = def::Needs::of(&c);
	assert!(needs.heads.contains("queue") && needs.links && !needs.gpus);
	assert!(def::Needs::of(&config("[rules.gpu-temp]\nenabled = true\n")).gpus);
	std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_settings_of_events_are_checked() {
	let bad = |more: &str| settings::join_mistakes(&settings::read_sources(&[Source::new("defaults", settings::DEFAULTS), Source::new("more.toml", more)]).unwrap_err());
	for (more, says) in [
		("[rules.x]\nvalue = \"cpu.tmp\"\nabove = 1\n", "no value \"cpu.tmp\""),
		("[rules.x]\nabove = 1\n", "needs value"),
		("[rules.x]\nvalue = \"cpu.temp\"\n", "needs one of when, above, below, equals and not"),
		("[rules.x]\nvalue = \"cpu.temp\"\nabove = 1\nbelow = 0\n", "takes one of"),
		("[rules.x]\nvalue = \"cpu.temp\"\nabove = \"hot\"\n", "should be a number"),
		("[rules.x]\nvalue = \"cpu.temp\"\nwhen = \"hot\"\n", "no level \"hot\""),
		("[rules.x]\nvalue = \"cpu.temp\"\nabove = 1\nlevel = \"normal\"\n", "no level of an event \"normal\""),
		("[rules.x]\nvalue = \"cpu.temp\"\nabove = 1\ntext = \"{vlaue}\"\n", "no {vlaue} here"),
		("[rules.x]\nvalue = \"disk.*.percent\"\nabove = 1\nplain = \"{name} is full\"\n", "no {name} here"),
		("[rules.x]\nkind = \"ups\"\n", "needs codes"),
		("[rules.x]\nkind = \"ups\"\ncodes = [\"power.lost\"]\nsource = \"eco\"\n", "no source \"eco\" of kind nut"),
		("[rules.x]\nkind = \"ups\"\ncodes = [\"power.lost\"]\nvalue = \"cpu.temp\"\n", "goes by codes, not by a value"),
		("[rules.x]\nkind = \"boot\"\ncodes = [\"power.lost\"]\n", "takes no value and no codes"),
		("[rules.x]\nvalue = \"cpu.temp\"\nabove = 1\nfro = 3\n", "no key \"fro\""),
		("[notify.phone]\nurl = \"https://ntfy.example.net\"\n", "topic_file"),
		("[notify.phone]\ntopic_file = \"/x\"\nurl = \"ftp://ntfy.example.net\"\n", "notify.phone.url"),
		("[notify.phone]\ntopic_file = \"/x\"\nrules = [\"dsik\"]\n", "no rule \"dsik\"; did you mean disk?"),
		("[notify.phone]\ntopic_file = \"/x\"\npriority = { warning = 9 }\n", "notify.phone.priority"),
		("[notify.phone]\ntopic_file = \"/x\"\ntitle = \"{name}\"\n", "no {name} here"),
		("[events]\nkeep = 1\n", "events.keep"),
		("[events]\nrole = \"owner\"\n", "no role \"owner\""),
		("[events]\nrole = \"glance\"\n", "no role \"glance\""),
		("[notify.phone]\ntopic_file = \"/x\"\npriority = { warnign = 3 }\n", "no \"warnign\" here"),
		("[notify.phone]\ntopic_file = \"/x\"\ntags = { recoverd = [\"x\"] }\n", "did you mean recovered?"),
		("[events.notify]\nlength = 3000\n", "events.notify.length"),
	] {
		let said = bad(more);
		assert!(said.contains(says), "{more:?} → {said}");
	}
	// A code upst-core doesn't give, and a topic file that isn't there, are warnings.
	let c = config(&format!("{STATION}[rules.x]\nkind = \"ups\"\ncodes = [\"power.los\"]\n[notify.phone]\ntopic_file = \"/nowhere/ntfy.topic\"\n"));
	let warned = settings::warnings(&c).join("\n");
	assert!(warned.contains("gives no \"power.los\"; did you mean power.lost?") && warned.contains("notify.phone.topic_file /nowhere/ntfy.topic can't be read"), "{warned}");
	assert_eq!(def::summary(&c), format!("Events: {} rules on; sent on to phone.", c.rules.values().filter(|r| r.enabled).count()));
	assert!(def::summary(&config("")).ends_with("sent nowhere: no [notify.<id>] names an ntfy topic."));
	// What `[events]` leaves out is events.toml's, key by key; a rule's and a target's, the
	// tables' that say so; settings of 0.8, without any of this, read as they did.
	let c = config("[events]\nkeep = 50\n[events.rule]\nlevel = \"critical\"\n[events.target]\nnames = true\n[rules.mine]\nvalue = \"cpu.usage\"\nabove = 99\n[notify.phone]\ntopic_file = \"/x\"\n");
	assert_eq!((c.events.keep, c.events.keep_days, c.events.notify.length), (50, 30, 500));
	assert_eq!((c.rules["mine"].level, c.rules["mine"].hold, c.rules["disk"].level), (Lvl::Critical, 0, Lvl::Critical));
	assert!(c.notify["phone"].names && c.notify["phone"].url == "https://ntfy.sh");
	let old = settings::read_sources(&[Source::new("old.toml", "schema = 3\ncontrol = true\n")]).unwrap();
	assert!(old.rules.is_empty() && old.notify.is_empty() && old.events == def::EventsDef::starting());
}

fn draft(t: u64, level: Lvl, text: &str) -> Draft {
	Draft { t, level, of: level, rule: "disk".into(), code: "disk.full".into(), phase: Phase::Start, text: text.into(), plain: "A disk is full".into(), title: None, priority: None, tags: None }
}

#[test]
fn events_are_kept_within_their_count_and_days_and_read_back() {
	let dir = scratch("log");
	let c = Arc::new(config("[events]\nkeep = 10\nkeep_days = 2\n"));
	let day = 86_400_000;
	// A little before now: what is kept is held against the clock as the service starts.
	let then = crate::activity::now_ms() - 60_000;
	let kept = Events::load(&dir, c.clone());
	for i in 0..25u64 {
		kept.record(draft(then + i, Lvl::Warning, &format!("Disk {i}\u{1b}[2J is full")), &c);
	}
	kept.delivered(25, "phone", "sent");
	kept.delivered(25, "tablet", "failed: HTTP 500");
	kept.delivered(3, "phone", "sent");
	let newest = kept.newest(100);
	assert_eq!(newest.len(), 10);
	assert_eq!((newest[0].0.n, newest[0].0.text.as_str(), newest[9].0.n), (25, "Disk 24 [2J is full", 16));
	assert_eq!(newest[0].1.iter().map(|d| (d.target.as_str(), d.result.as_str())).collect::<Vec<_>>(), [("phone", "sent"), ("tablet", "failed: HTTP 500")]);
	// The file is the owner's alone and stays within a few times what is kept.
	let path = dir.join(FILE);
	assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
	assert!(std::fs::read_to_string(&path).unwrap().lines().count() <= 30);
	// Read back, a damaged line skipped; the count goes on.
	std::fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(b"{not json\n").unwrap();
	let again = Events::load(&dir, c.clone());
	assert_eq!(again.newest(100), newest);
	assert_eq!(again.record(draft(then + 30, Lvl::Critical, "More"), &c).n, 26);
	// Older than its days, an event goes, as the next is kept or as the service starts.
	let aged = scratch("aged");
	let later = Events::load(&aged, c.clone());
	later.record(draft(then - 3 * day, Lvl::Info, "Long ago"), &c);
	assert_eq!(later.newest(100).len(), 1);
	later.record(draft(then, Lvl::Info, "Later"), &c);
	assert_eq!(later.newest(100).iter().map(|(e, _)| e.n).collect::<Vec<_>>(), [2]);
	// One from a clock that ran ahead takes none of the others with it.
	later.record(draft(then + 300 * day, Lvl::Info, "From the future"), &c);
	assert_eq!(later.newest(100).len(), 2);
	// As a `log` has them: the rule in the program's place, the level a journal's.
	let (lines, more) = again.page(None, &crate::journal::Filter::new(4, None), 3);
	assert_eq!(lines.iter().map(|l| (l.id.as_deref(), l.priority, l.text.as_str())).collect::<Vec<_>>(), [(Some("disk"), 4, "Disk 23 [2J is full"), (Some("disk"), 4, "Disk 24 [2J is full"), (Some("disk"), 2, "More")]);
	let (older, _) = again.page(more, &crate::journal::Filter::new(4, Some("disk 2")), 100);
	assert_eq!(older.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["Disk 20 [2J is full", "Disk 21 [2J is full", "Disk 22 [2J is full"]);
	let mut sent = None;
	let state = again.tail().buffer().state(&mut sent, &crate::journal::Filter::new(2, None), 200).unwrap();
	assert_eq!(state["lines"].as_array().unwrap().len(), 1);
	assert_eq!(state["lines"][0]["level"], "critical");
	std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn elamond_notify_leaves_events_for_the_service() {
	let dir = scratch("spool");
	let c = Arc::new(config("[events.notify]\nburst = 3\nper = 60\nlength = 20\nlevel = \"warning\"\nrule = \"script\"\n"));
	let kept = Events::load(&dir, c.clone());
	let mut window = VecDeque::new();
	kept.take_notified(&c, &mut window);
	assert!(kept.newest(10).is_empty());
	spool(&dir, &Notified { t: 5, level: None, title: Some("Backup".into()), text: "The backup is done, all of it, and well".into() }).unwrap();
	spool(&dir, &Notified { t: 6, level: Some(Lvl::Critical), title: None, text: "It failed".into() }).unwrap();
	std::fs::OpenOptions::new().append(true).open(dir.join(SPOOL_FILE)).unwrap().write_all(b"not ours\n").unwrap();
	assert_eq!(std::fs::metadata(dir.join(SPOOL_FILE)).unwrap().permissions().mode() & 0o777, 0o600);
	kept.take_notified(&c, &mut window);
	let made = kept.newest(10);
	assert_eq!(made.len(), 2);
	assert_eq!((made[1].0.level, made[1].0.rule.as_str(), made[1].0.text.as_str(), made[1].0.title.as_deref(), made[1].0.phase), (Lvl::Warning, "script", "The backup is done,", Some("Backup"), Phase::Once));
	assert_eq!((made[0].0.level, made[0].0.plain.as_str()), (Lvl::Critical, "It failed"));
	// Taken once; and a script in a loop makes no more than `burst` in `per`.
	kept.take_notified(&c, &mut window);
	assert_eq!(kept.newest(10).len(), 2);
	for i in 0..10 {
		spool(&dir, &Notified { t: 10 + i, level: None, title: None, text: format!("again {i}") }).unwrap();
	}
	kept.take_notified(&c, &mut window);
	assert_eq!(kept.newest(100).len(), 3);
	std::fs::remove_dir_all(dir).unwrap();
}

// ---- the targets ----------------------------------------------------------------------------

/// A server on a loopback port that answers each request with the next of `codes` (the last of
/// them from then on), after `delay`, and keeps what it was asked.
struct Ntfy {
	url: String,
	seen: Arc<Mutex<Vec<String>>>,
	task: tokio::task::JoinHandle<()>,
}

impl Ntfy {
	async fn start(codes: &[u16], delay: Duration) -> Ntfy {
		use tokio::io::{AsyncReadExt, AsyncWriteExt};
		let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
		let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
		let seen = Arc::new(Mutex::new(Vec::new()));
		let (kept, codes) = (seen.clone(), codes.to_vec());
		let task = tokio::spawn(async move {
			while let Ok((mut conn, _)) = listener.accept().await {
				let mut request = Vec::new();
				let mut buf = [0u8; 4096];
				// The headers, then as much body as they say.
				while let Ok(n) = conn.read(&mut buf).await {
					request.extend_from_slice(&buf[..n]);
					let text = String::from_utf8_lossy(&request).to_string();
					let Some((head, body)) = text.split_once("\r\n\r\n") else { if n == 0 { break } else { continue } };
					let length = head.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok())).unwrap_or(0);
					if body.len() >= length || n == 0 {
						break;
					}
				}
				let nth = {
					let mut seen = kept.lock().unwrap();
					seen.push(String::from_utf8_lossy(&request).to_string());
					seen.len() - 1
				};
				let code = codes.get(nth).or(codes.last()).copied().unwrap_or(200);
				tokio::time::sleep(delay).await;
				let extra = if code == 302 { "Location: http://127.0.0.1:1/elsewhere\r\n" } else { "" };
				let _ = conn.write_all(format!("HTTP/1.1 {code} X\r\n{extra}Content-Length: 2\r\nConnection: close\r\n\r\n{{}}").as_bytes()).await;
			}
		});
		Ntfy { url, seen, task }
	}

	fn seen(&self) -> Vec<String> {
		self.seen.lock().unwrap().clone()
	}

	/// The JSON bodies it was sent, in order.
	fn bodies(&self) -> Vec<serde_json::Value> {
		self.seen().iter().map(|r| serde_json::from_str(r.split_once("\r\n\r\n").map_or("", |(_, b)| b)).unwrap_or_default()).collect()
	}
}

impl Drop for Ntfy {
	fn drop(&mut self) {
		self.task.abort();
	}
}

const TOPIC: &str = "topic-marker-7f3a";
const TOKEN: &str = "tk_token-marker-91c2";

fn secret(dir: &Path, name: &str, text: &str) -> PathBuf {
	let path = dir.join(name);
	std::fs::write(&path, format!("{text}\n")).unwrap();
	std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
	path
}

/// The service's events in `dir` with their targets started, as `more` of the settings has them.
fn started(dir: &Path, more: &str) -> (Arc<Events>, Arc<Config>) {
	let c = Arc::new(config(more));
	let kept = Arc::new(Events::load(dir, c.clone()));
	kept.start(&tokio::runtime::Handle::current(), "desk");
	(kept, c)
}

/// Waits until `done`, a few seconds at most.
async fn until(what: &str, done: impl Fn() -> bool) {
	for _ in 0..800 {
		if done() {
			return;
		}
		tokio::time::sleep(Duration::from_millis(10)).await;
	}
	panic!("never: {what}");
}

fn results(kept: &Events) -> Vec<(u64, String, String)> {
	let mut all: Vec<_> = kept.newest(100).into_iter().flat_map(|(e, sent)| sent.into_iter().map(move |d| (e.n, d.target, d.result))).collect();
	all.sort();
	all
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn events_reach_their_targets_as_the_settings_shape_them() {
	let dir = scratch("ntfy");
	let server = Ntfy::start(&[200], Duration::ZERO).await;
	let topic = secret(&dir, "ntfy.topic", TOPIC);
	let token = secret(&dir, "ntfy.token", TOKEN);
	let (kept, c) = started(
		&dir,
		&format!(
			"[notify.plain]\nurl = \"{0}\"\ntopic_file = \"{1}\"\n\
			 [notify.named]\nurl = \"{0}\"\ntopic_file = \"{1}\"\ntoken_file = \"{2}\"\nnames = true\nlevel = \"critical\"\nrules = [\"disk\"]\ntitle_names = \"{{computer}} says\"\nmessage = \"[{{level}}] {{text}}\"\npriority = {{ critical = 3 }}\ntags = {{ critical = [\"fire\"] }}\n",
			server.url,
			topic.display(),
			token.display()
		),
	);
	let warn = kept.record(draft(1, Lvl::Warning, "Disk \"data\" is 95% full\\"), &c);
	let crit = kept.record(Draft { priority: Some(1), tags: Some(vec!["own".into()]), ..draft(2, Lvl::Critical, "Disk data is 99% full") }, &c);
	let other = kept.record(Draft { rule: "unit".into(), ..draft(3, Lvl::Critical, "The unit of web failed") }, &c);
	let end = kept.record(Draft { phase: Phase::End, level: Lvl::Info, of: Lvl::Critical, priority: Some(1), ..draft(4, Lvl::Critical, "Disk data has room again") }, &c);
	let low = kept.record(draft(5, Lvl::Info, "Nothing much"), &c);
	until("six requests", || server.seen().len() == 6 && results(&kept).len() == 9).await;
	assert_eq!(
		results(&kept),
		[
			(warn.n, "named".to_string(), "not sent: below critical".to_string()),
			(warn.n, "plain".into(), "sent".into()),
			(crit.n, "named".into(), "sent".into()),
			(crit.n, "plain".into(), "sent".into()),
			// `rules`: the other target takes the disk's alone, and says nothing of the rest.
			(other.n, "plain".into(), "sent".into()),
			// An end goes by the level of its start.
			(end.n, "named".into(), "sent".into()),
			(end.n, "plain".into(), "sent".into()),
			(low.n, "named".into(), "not sent: below critical".into()),
			(low.n, "plain".into(), "not sent: below warning".into()),
		]
	);
	let mut bodies = server.bodies();
	bodies.sort_by_key(|b| (b["title"].as_str().map(str::to_string), b["message"].as_str().map(str::to_string), b["priority"].as_u64()));
	let json = |title: &str, message: &str, priority: u8, tags: &[&str]| {
		let mut o = serde_json::json!({ "topic": TOPIC, "title": title, "message": message, "priority": priority });
		if !tags.is_empty() {
			o["tags"] = serde_json::json!(tags);
		}
		o
	};
	assert_eq!(
		bodies,
		[
			// Without names: the plain text, the plain title; the rule's own priority and tags over
			// the target's for a start, the target's `recovered` for an end.
			json("Elamon", "A disk is full", 1, &["own"]),
			json("Elamon", "A disk is full", 2, &["white_check_mark"]),
			json("Elamon", "A disk is full", 4, &["warning"]),
			json("Elamon", "A disk is full", 5, &["rotating_light"]),
			// With names, and the target's own shape.
			json("desk says", "[critical] Disk data is 99% full", 1, &["own"]),
			json("desk says", "[info] Disk data has room again", 2, &["white_check_mark"]),
		]
	);
	// The topic is in the body alone, the token in its header alone; asked of the server's root.
	for request in server.seen() {
		let (head, _) = request.split_once("\r\n\r\n").unwrap();
		assert!(head.starts_with("POST / HTTP/1.1\r\n") && !head.contains(TOPIC), "{head}");
		assert!(head.to_ascii_lowercase().contains("content-type: application/json"));
	}
	assert_eq!(server.seen().iter().filter(|r| r.contains(&format!("Authorization: Bearer {TOKEN}"))).count(), 2);
	// Neither is anywhere in what is kept.
	let file = std::fs::read_to_string(dir.join(FILE)).unwrap();
	assert!(!file.contains(TOPIC) && !file.contains(TOKEN));
	std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_that_fails_is_tried_again_and_one_that_refuses_is_not() {
	let dir = scratch("retry");
	let topic = secret(&dir, "ntfy.topic", TOPIC);
	let target = |id: &str, url: &str, more: &str| format!("[notify.{id}]\nurl = \"{url}\"\ntopic_file = \"{}\"\nretry = 1\nretry_max = 1\ntimeout = 1\n{more}", topic.display());
	let busy = Ntfy::start(&[500, 429, 200], Duration::ZERO).await;
	let refuses = Ntfy::start(&[401], Duration::ZERO).await;
	let elsewhere = Ntfy::start(&[302], Duration::ZERO).await;
	let slow = Ntfy::start(&[200], Duration::from_secs(5)).await;
	let down = Ntfy::start(&[503], Duration::ZERO).await;
	let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
	let nowhere = format!("http://127.0.0.1:{}", closed.local_addr().unwrap().port());
	drop(closed);
	let unread = dir.join("others.topic");
	std::fs::write(&unread, TOPIC).unwrap();
	std::fs::set_permissions(&unread, std::fs::Permissions::from_mode(0o644)).unwrap();
	let (kept, c) = started(
		&dir,
		&[
			target("busy", &busy.url, ""),
			target("refuses", &refuses.url, ""),
			target("elsewhere", &elsewhere.url, ""),
			target("slow", &slow.url, "retries = 0\n"),
			target("down", &down.url, "retries = 1\n"),
			target("nowhere", &nowhere, "retries = 0\n"),
			target("old", &down.url, "expire = 1\n"),
			format!("[notify.open]\nurl = \"{}\"\ntopic_file = \"{}\"\n", busy.url, unread.display()),
			"[notify.off]\nenabled = false\ntopic_file = \"/nowhere\"\n".to_string(),
		]
		.concat(),
	);
	let e = kept.record(draft(1, Lvl::Warning, "Disk data is full"), &c);
	until("every target done with it", || results(&kept).len() == 8).await;
	let said: std::collections::BTreeMap<String, String> = results(&kept).into_iter().map(|(n, target, result)| {
		assert_eq!(n, e.n);
		(target, result)
	}).collect();
	assert_eq!(said["busy"], "sent");
	assert_eq!(busy.seen().len(), 3);
	assert_eq!(said["refuses"], "failed: HTTP 401");
	assert_eq!(refuses.seen().len(), 1);
	assert_eq!(said["elsewhere"], "failed: redirect to elsewhere (302), not followed");
	assert_eq!(said["slow"], "failed: timed out");
	assert_eq!(said["down"], "failed: HTTP 503");
	assert!(said["nowhere"].starts_with("failed: ") && !said["nowhere"].contains("curl:"), "{}", said["nowhere"]);
	assert_eq!(said["old"], "not sent: too old by the time it could be");
	assert_eq!(said["open"], "failed: topic_file may be read by others: chmod 600 it");
	assert_eq!(down.seen().len(), 3);
	assert!(!said.values().any(|r| r.contains(TOPIC)));
	std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn more_than_a_targets_share_is_told_as_one() {
	let dir = scratch("burst");
	let server = Ntfy::start(&[200], Duration::ZERO).await;
	let topic = secret(&dir, "ntfy.topic", TOPIC);
	let (kept, c) = started(&dir, &format!("[notify.phone]\nurl = \"{}\"\ntopic_file = \"{}\"\nburst = 2\nper = 1\nmore = \"and {{count}} more\"\n", server.url, topic.display()));
	for i in 0..7 {
		kept.record(draft(i, Lvl::Warning, "Disk data is full"), &c);
	}
	until("the first two, and then how many more", || server.seen().len() == 3).await;
	let held = results(&kept).iter().filter(|(_, _, r)| r == "held back: more than 2 in 1 s").count();
	assert_eq!((held, results(&kept).iter().filter(|(_, _, r)| r == "sent").count()), (5, 2));
	let last = server.bodies().pop().unwrap();
	assert_eq!((last["message"].as_str(), last["title"].as_str(), last["priority"].as_u64()), (Some("and 5 more"), Some("Elamon"), Some(2)));
	// A queue no longer than the settings let it: the oldest waiting goes.
	let stuck = Ntfy::start(&[500], Duration::ZERO).await;
	let (kept, c) = started(&scratch("queue"), &format!("[notify.phone]\nurl = \"{}\"\ntopic_file = \"{}\"\nqueue = 2\nretry = 60\n", stuck.url, topic.display()));
	for i in 0..5 {
		kept.record(draft(i, Lvl::Warning, "Disk data is full"), &c);
	}
	until("three dropped", || results(&kept).len() == 3).await;
	assert!(results(&kept).iter().all(|(_, _, r)| r == "dropped: too many waiting"));
	std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_request_is_made_of_no_secret() {
	// What curl is run with: the server's address, and nothing of the topic or the token, which
	// it reads on its stdin with the body.
	let c = config("[notify.phone]\nurl = \"https://ntfy.example.net\"\ntopic_file = \"/x\"\n");
	let args = ntfy::curl_args(&c.notify["phone"]);
	assert_eq!(args[..3], ["-q", "--config", "-"]);
	assert!(args.contains(&"https://ntfy.example.net".to_string()) && args.windows(2).any(|w| w == ["--max-redirs", "0"]) && args.windows(2).any(|w| w == ["--proto", "=http,https"]));
	assert_eq!(ntfy::body("t", "Elamon", "A \"disk\"\nis full", 4, &["warning".into()]), r#"{"topic":"t","title":"Elamon","message":"A \"disk\"\nis full","priority":4,"tags":["warning"]}"#);
}

use std::io::Write;
