//! What a client may run (docs/config.md, Actions): the built-in actions and those the settings
//! add, their parameters, and the check of the values a client sends for them.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::access::Role;
use crate::bus::PowerCaps;
use crate::settings::{self, Config};

/// What kind of thing an action does: what a profile's `unlock.for` names, in the order an unlock
/// covers them (an unlock for one covers those after it).
pub const KINDS: [&str; 6] = ["power", "signal", "actions", "units", "profile", "media"];

/// A parameter's kind, and what it takes.
#[derive(Clone, Debug, PartialEq)]
pub enum ParamKind {
	/// One of the values, each with its label.
	Choice(Vec<(String, String)>),
	/// A number from `min` to `max` on a `step` from `min`.
	Range { min: serde_json::Number, max: serde_json::Number, step: serde_json::Number, unit: Option<String> },
	Bool,
	Integer,
	Number { unit: Option<String> },
	Str,
}

impl ParamKind {
	pub fn name(&self) -> &'static str {
		match self {
			ParamKind::Choice(_) => "choice",
			ParamKind::Range { .. } => "range",
			ParamKind::Bool => "bool",
			ParamKind::Integer => "integer",
			ParamKind::Number { .. } => "number",
			ParamKind::Str => "string",
		}
	}
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
	pub id: String,
	pub label: String,
	pub kind: ParamKind,
	/// May be left out, with no default.
	pub optional: bool,
	pub default: Option<Value>,
}

/// An action as a client may be offered it.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
	pub id: String,
	pub name: String,
	pub kind: &'static str,
	pub icon: String,
	/// The least role that may run it.
	pub role: Role,
	/// Asked before running, before the profile has its say.
	pub confirm: bool,
	/// Unlocks the phone first, whatever the profile says.
	pub unlock: bool,
	pub params: Vec<Param>,
}

/// What this computer can do now, for the built-in actions.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Caps {
	pub poweroff: bool,
	pub reboot: bool,
	pub suspend: bool,
	pub lock: bool,
	/// power-profiles-daemon's profiles; none without it.
	pub power_profiles: Vec<String>,
	/// systemd, for the links' units.
	pub units: bool,
	/// A session bus, for MPRIS.
	pub media: bool,
	/// The volume can be read and set.
	pub volume: bool,
}

impl Caps {
	/// What logind says the computer can do, and the rest.
	pub fn from_power(power: &PowerCaps) -> Caps {
		let can = |s: &str| s == "yes" || s == "challenge";
		Caps { poweroff: can(&power.poweroff), reboot: can(&power.reboot), suspend: can(&power.suspend), lock: power.lock, ..Default::default() }
	}

	/// Everything, as the fixtures have it.
	pub fn all(power_profiles: &[String]) -> Caps {
		Caps { poweroff: true, reboot: true, suspend: true, lock: true, power_profiles: power_profiles.to_vec(), units: true, media: true, volume: true }
	}
}

/// The built-in actions in the order they are listed: id, name, kind, icon.
pub const BUILT_IN: [(&str, &str, &str, &str); 14] = [
	("power.poweroff", "Shut down", "power", "power"),
	("power.reboot", "Restart", "power", "restart"),
	("power.suspend", "Sleep", "power", "moon"),
	("power.lock", "Lock", "power", "lock"),
	("profile.set", "Power profile", "profile", "bolt"),
	("process.signal", "Signal", "signal", "processes"),
	("unit.start", "Start", "units", "play"),
	("unit.stop", "Stop", "units", "pause"),
	("unit.restart", "Restart", "units", "restart"),
	("media.play_pause", "Play or pause", "media", "play"),
	("media.next", "Next", "media", "next"),
	("media.previous", "Previous", "media", "previous"),
	("media.seek", "Seek", "media", "play"),
	("volume.set", "Volume", "media", "volume"),
];

/// The least role for an action of `kind`.
pub fn kind_role(kind: &str) -> Role {
	if kind == "power" || kind == "signal" { Role::Admin } else { Role::Operate }
}

/// "power-saver" → "Power saver".
pub fn label_of(value: &str) -> String {
	let words = value.replace(['-', '_'], " ");
	let mut c = words.chars();
	c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

fn param(id: &str, label: &str, kind: ParamKind, optional: bool) -> Param {
	Param { id: id.into(), label: label.into(), kind, optional, default: None }
}

/// A built-in action's parameters, with the choices the computer and settings give.
fn built_in_params(id: &str, config: &Config, caps: &Caps) -> Vec<Param> {
	let player = || param("player", "Player", ParamKind::Str, false);
	match id {
		"profile.set" => {
			let choices = caps.power_profiles.iter().map(|p| (p.clone(), label_of(p))).collect();
			vec![param("profile", "Profile", ParamKind::Choice(choices), false)]
		}
		"process.signal" => vec![
			param("pid", "Process", ParamKind::Integer, false),
			param("signal", "Signal", ParamKind::Choice(vec![("term".into(), "Quit".into()), ("kill".into(), "Force kill".into())]), false),
			param("start", "Started", ParamKind::Integer, true),
		],
		"unit.start" | "unit.stop" | "unit.restart" => {
			let links = config.links.iter().filter(|(_, l)| l.unit.is_some()).map(|(id, l)| (id.clone(), l.name.clone())).collect();
			vec![param("link", "Link", ParamKind::Choice(links), false)]
		}
		"media.play_pause" | "media.next" | "media.previous" => vec![player()],
		"media.seek" => vec![player(), param("position", "Position", ParamKind::Number { unit: Some("s".into()) }, false)],
		"volume.set" => vec![
			param("level", "Level", ParamKind::Range { min: 0.into(), max: 100.into(), step: 1.into(), unit: Some("%".into()) }, true),
			param("muted", "Muted", ParamKind::Bool, true),
		],
		_ => Vec::new(),
	}
}

/// The kinds of a built-in action's parameters, by id, whatever the computer: what the settings
/// are checked against.
pub fn built_in_param_kinds(id: &str) -> Option<Vec<(&'static str, &'static str)>> {
	BUILT_IN.iter().find(|b| b.0 == id)?;
	Some(match id {
		"profile.set" => vec![("profile", "choice")],
		"process.signal" => vec![("pid", "integer"), ("signal", "choice"), ("start", "integer")],
		"unit.start" | "unit.stop" | "unit.restart" => vec![("link", "choice")],
		"media.play_pause" | "media.next" | "media.previous" => vec![("player", "string")],
		"media.seek" => vec![("player", "string"), ("position", "number")],
		"volume.set" => vec![("level", "range"), ("muted", "bool")],
		_ => Vec::new(),
	})
}

/// Whether the computer can do a built-in action.
fn can(id: &str, caps: &Caps, config: &Config) -> bool {
	match id {
		"power.poweroff" => caps.poweroff,
		"power.reboot" => caps.reboot,
		"power.suspend" => caps.suspend,
		"power.lock" => caps.lock,
		"profile.set" => !caps.power_profiles.is_empty(),
		"unit.start" | "unit.stop" | "unit.restart" => caps.units && config.links.values().any(|l| l.unit.is_some()),
		"volume.set" => caps.volume,
		id if id.starts_with("media.") => caps.media,
		_ => true,
	}
}

/// An action the settings add, as offered.
pub fn settings_spec(id: &str, a: &settings::Action) -> Spec {
	let params = a
		.params
		.iter()
		.map(|(pid, p)| {
			let kind = if let Some(choices) = &p.choices {
				ParamKind::Choice(
					choices.iter().map(|c| (c.clone(), p.labels.as_ref().and_then(|l| l.get(c)).cloned().unwrap_or_else(|| c.clone()))).collect(),
				)
			} else if let Some(range) = &p.range {
				let step = p.step.clone().unwrap_or_else(|| 1.into());
				ParamKind::Range { min: range[0].clone(), max: range[1].clone(), step, unit: p.unit.clone() }
			} else {
				ParamKind::Bool
			};
			Param { id: pid.clone(), label: p.label.clone().unwrap_or_else(|| pid.clone()), kind, optional: false, default: p.default.clone() }
		})
		.collect();
	Spec {
		id: id.to_string(),
		name: a.name.clone(),
		kind: "actions",
		icon: a.icon.clone().unwrap_or_else(|| "terminal".into()),
		role: a.role,
		confirm: a.confirm,
		unlock: a.unlock,
		params,
	}
}

/// Every action the computer can do: the built-in ones in their order, then the settings' by id.
pub fn available(config: &Config, caps: &Caps) -> Vec<Spec> {
	let built_in = BUILT_IN.iter().filter(|b| can(b.0, caps, config)).map(|&(id, name, kind, icon)| Spec {
		id: id.into(),
		name: name.into(),
		kind,
		icon: icon.into(),
		role: kind_role(kind),
		confirm: kind == "power" || kind == "signal",
		unlock: false,
		params: built_in_params(id, config, caps),
	});
	built_in.chain(config.actions.iter().map(|(id, a)| settings_spec(id, a))).collect()
}

/// A parameter as `ui` describes it.
pub fn param_json(p: &Param) -> Value {
	let mut o = serde_json::Map::new();
	o.insert("id".into(), p.id.clone().into());
	o.insert("kind".into(), p.kind.name().into());
	o.insert("label".into(), p.label.clone().into());
	match &p.kind {
		ParamKind::Choice(c) => {
			o.insert("choices".into(), c.iter().map(|(v, l)| json!({ "value": v, "label": l })).collect());
		}
		ParamKind::Range { min, max, step, unit } => {
			o.insert("min".into(), Value::Number(min.clone()));
			o.insert("max".into(), Value::Number(max.clone()));
			o.insert("step".into(), Value::Number(step.clone()));
			if let Some(u) = unit {
				o.insert("unit".into(), u.clone().into());
			}
		}
		ParamKind::Number { unit: Some(u) } => {
			o.insert("unit".into(), u.clone().into());
		}
		_ => {}
	}
	if p.optional {
		o.insert("optional".into(), true.into());
	}
	if let Some(d) = &p.default {
		o.insert("default".into(), d.clone());
	}
	Value::Object(o)
}

/// Whether `x` is on a step of `step` from `min` (to a millionth of a step).
fn on_step(x: f64, min: f64, step: f64) -> bool {
	if step <= 0.0 {
		return true;
	}
	let n = (x - min) / step;
	(n - n.round()).abs() < 1e-6
}

/// Whether `v` is a value `p` takes.
pub fn param_takes(p: &Param, v: &Value) -> bool {
	match (&p.kind, v) {
		(ParamKind::Choice(c), Value::String(s)) => c.iter().any(|(value, _)| value == s),
		(ParamKind::Range { min, max, step, .. }, Value::Number(n)) => {
			let (lo, hi, st, x) = (min.as_f64().unwrap_or(0.0), max.as_f64().unwrap_or(0.0), step.as_f64().unwrap_or(1.0), n.as_f64().unwrap_or(f64::NAN));
			let whole = min.is_i64() && step.is_i64();
			(lo..=hi).contains(&x) && on_step(x, lo, st) && (!whole || n.is_i64() || x.fract() == 0.0)
		}
		(ParamKind::Bool, Value::Bool(_)) => true,
		(ParamKind::Integer, Value::Number(n)) => n.is_i64() || n.is_u64(),
		(ParamKind::Number { .. }, Value::Number(_)) => true,
		(ParamKind::Str, Value::String(_)) => true,
		_ => false,
	}
}

/// The values to run `spec` with, from those a client sent: each checked against its parameter,
/// defaults filled in. `Err` names the first parameter that is wrong, missing or unknown
/// (400 `bad_param`).
pub fn check_params(spec: &Spec, given: &serde_json::Map<String, Value>) -> Result<BTreeMap<String, Value>, String> {
	if let Some(unknown) = given.keys().find(|k| !spec.params.iter().any(|p| &p.id == *k)) {
		return Err(unknown.clone());
	}
	let mut out = BTreeMap::new();
	for p in &spec.params {
		match given.get(&p.id) {
			Some(v) if param_takes(p, v) => {
				out.insert(p.id.clone(), v.clone());
			}
			Some(_) => return Err(p.id.clone()),
			None => match &p.default {
				Some(d) => {
					out.insert(p.id.clone(), d.clone());
				}
				None if p.optional => {}
				None => return Err(p.id.clone()),
			},
		}
	}
	Ok(out)
}

/// A value as a command argument: `server`, `50`, `true`.
fn argument(v: &Value) -> String {
	match v {
		Value::String(s) => s.clone(),
		Value::Number(n) if n.as_f64().is_some_and(|x| x.fract() == 0.0 && !n.is_f64()) => n.to_string(),
		other => other.to_string(),
	}
}

/// The command of an action the settings add, an argument that is exactly `{param}` replaced by
/// that parameter's value; any other argument as it is.
pub fn command(action: &settings::Action, params: &BTreeMap<String, Value>) -> Vec<String> {
	action
		.command
		.iter()
		.map(|arg| match whole_param(arg) {
			Some(name) => params.get(name).map(argument).unwrap_or_else(|| arg.clone()),
			None => arg.clone(),
		})
		.collect()
}

/// The parameter an argument is, when it is exactly `{name}`.
pub fn whole_param(arg: &str) -> Option<&str> {
	let name = arg.strip_prefix('{')?.strip_suffix('}')?;
	(!name.is_empty() && !name.contains(['{', '}'])).then_some(name)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn config() -> Config {
		settings::read_sources(&[settings::Source::new("test.toml", include_str!("../fixtures/ui-source.toml"))]).unwrap()
	}

	#[test]
	fn what_the_computer_can_do() {
		let config = config();
		let all = available(&config, &Caps::all(&["balanced".into()]));
		let ids: Vec<&str> = all.iter().map(|s| s.id.as_str()).collect();
		assert_eq!(ids.len(), 17);
		assert_eq!(&ids[14..], ["backup", "fan", "mode"]);
		let none = available(&config, &Caps::default());
		let ids: Vec<&str> = none.iter().map(|s| s.id.as_str()).collect();
		assert_eq!(ids, ["process.signal", "backup", "fan", "mode"]);
		let caps = Caps::from_power(&PowerCaps { poweroff: "challenge".into(), reboot: "no".into(), suspend: "na".into(), lock: true });
		assert_eq!((caps.poweroff, caps.reboot, caps.suspend, caps.lock), (true, false, false, true));
		assert_eq!(label_of("power-saver"), "Power saver");
	}

	#[test]
	fn values_are_checked_against_their_parameters() {
		let config = config();
		let all = available(&config, &Caps::all(&[]));
		let spec = |id: &str| all.iter().find(|s| s.id == id).unwrap().clone();
		let given = |v: Value| v.as_object().unwrap().clone();
		let mode = spec("mode");
		assert_eq!(check_params(&mode, &given(json!({}))).unwrap()["mode"], "desktop");
		assert_eq!(check_params(&mode, &given(json!({ "mode": "server" }))).unwrap()["mode"], "server");
		assert_eq!(check_params(&mode, &given(json!({ "mode": "other" }))), Err("mode".into()));
		assert_eq!(check_params(&mode, &given(json!({ "extra": 1 }))), Err("extra".into()));
		let fan = spec("fan");
		assert!(check_params(&fan, &given(json!({ "speed": 55 }))).is_ok());
		assert_eq!(check_params(&fan, &given(json!({ "speed": 52 }))), Err("speed".into()));
		assert_eq!(check_params(&fan, &given(json!({ "speed": 105 }))), Err("speed".into()));
		assert_eq!(check_params(&fan, &given(json!({ "speed": 55.5 }))), Err("speed".into()));
		let signal = spec("process.signal");
		assert_eq!(check_params(&signal, &given(json!({ "pid": 4242 }))), Err("signal".into()));
		assert_eq!(check_params(&signal, &given(json!({ "pid": 4242, "signal": "term" }))).unwrap().len(), 2);
		assert_eq!(check_params(&signal, &given(json!({ "pid": "4242", "signal": "term" }))), Err("pid".into()));
		let volume = spec("volume.set");
		assert!(check_params(&volume, &given(json!({}))).unwrap().is_empty());
		assert!(check_params(&volume, &given(json!({ "level": 40, "muted": true }))).is_ok());
		let backup = &config.actions["backup"];
		let params = check_params(&spec("backup"), &given(json!({}))).unwrap();
		assert_eq!(command(backup, &params), ["backup", "--verify", "true"]);
		let params = check_params(&fan, &given(json!({ "speed": 55 }))).unwrap();
		assert_eq!(command(&config.actions["fan"], &params), ["fanctl", "--speed", "55"]);
		assert_eq!((whole_param("{mode}"), whole_param("--x={mode}"), whole_param("{}")), (Some("mode"), None, None));
	}
}
