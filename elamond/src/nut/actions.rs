//! A station's actions (docs/protocol.md, Station actions): `nut.command.<source>` and
//! `nut.set.<source>.<variable>` as the station offers them and the source allows; the checks a
//! request for one goes through that the other actions haven't; and running it, the source's
//! password read afresh each time, one at a time on a source.

use std::collections::BTreeMap;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;

use serde_json::Value;
use upst_core::nut::{dangerous, CmdInfo, VarInfo};
use upst_core::poll::{Action, Creds, Failure};
use upst_core::fmt as nut_fmt;

use super::{allows, def_of, Runtime, Station, V};
use crate::actions::{self, label_of, Param, ParamKind, Spec};
use crate::errors::{reason, ErrorCode};
use crate::settings::NutSource;

/// What a station's action is about, from its id: the source, and a setting's variable.
#[derive(Debug, PartialEq)]
pub enum Target<'a> {
	Command { source: &'a str },
	Set { source: &'a str, var: &'a str },
}

/// `nut.command.eco` → the commands of `eco`; `nut.set.eco.ups.beeper.status` → its
/// `ups.beeper.status` (a source's id has no dots).
pub fn target(id: &str) -> Option<Target<'_>> {
	if let Some(source) = id.strip_prefix("nut.command.") {
		return (!source.is_empty() && !source.contains('.')).then_some(Target::Command { source });
	}
	let (source, var) = id.strip_prefix("nut.set.")?.split_once('.')?;
	(!source.is_empty() && !var.is_empty()).then_some(Target::Set { source, var })
}

/// The outlet a command is about, and what of it: `outlet.2.load.off` → 2, `load.off`.
fn outlet_of(cmd: &str) -> Option<(u32, &str)> {
	let (n, rest) = cmd.strip_prefix("outlet.")?.split_once('.')?;
	if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
		return None;
	}
	Some((n.parse().ok()?, rest))
}

/// Whether `cmd` on the station of `src` is typed in to run: a command on an outlet the settings
/// call critical but the one that turns it on, and one upst-core counts as dangerous that isn't
/// about one outlet (`load.off`, `shutdown.*`, `calibrate.start`). Another outlet turned off is
/// asked as any action is.
pub fn typed(src: &NutSource, cmd: &str) -> bool {
	match outlet_of(cmd) {
		Some((n, what)) => src.critical(n) && what != "load.on",
		None => dangerous(cmd),
	}
}

/// What upsd said the station offers: its commands, in its order, and its writable variables.
#[derive(Clone, Debug)]
pub struct Meta {
	pub cmds: Vec<CmdInfo>,
	pub rw: Vec<String>,
}

/// What a writable variable takes, as a parameter: its list of values, or its ranges as one from
/// the least to the most on a step of 1, in its unit. `None` for any other kind.
fn takes(var: &str, info: &VarInfo) -> Option<ParamKind> {
	if !info.enum_values.is_empty() {
		return Some(ParamKind::Choice(info.enum_values.iter().map(|v| (v.clone(), label_of(v))).collect()));
	}
	let bounds: Vec<(f64, f64)> = info.ranges.iter().filter_map(|(lo, hi)| Some((nut_fmt::parse_num(lo)?, nut_fmt::parse_num(hi)?))).collect();
	let min = bounds.iter().map(|b| b.0).fold(f64::INFINITY, f64::min);
	let max = bounds.iter().map(|b| b.1).fold(f64::NEG_INFINITY, f64::max);
	if bounds.is_empty() || min > max {
		return None;
	}
	let number = |x: f64| {
		if x.fract() == 0.0 && x.abs() < 9e15 { serde_json::Number::from(x as i64) } else { serde_json::Number::from_f64(x).unwrap_or_else(|| 0.into()) }
	};
	let unit = def_of(var).unit;
	Some(ParamKind::Range { min: number(min), max: number(max), step: 1.into(), unit: (!unit.is_empty()).then(|| unit.to_string()) })
}

fn spec(id: String, name: &str, icon: &str, src: &NutSource, param: Param) -> Spec {
	Spec { id, name: name.to_string(), kind: "nut", icon: icon.into(), role: src.role, confirm: true, unlock: false, params: vec![param] }
}

impl Station {
	/// What upsd says the station offers (each time it is connected to again): kept, and the
	/// writable variables the source's `settings` allow returned, for upsd to describe.
	pub fn offers(&mut self, cmds: Vec<CmdInfo>, rw: Vec<String>) -> Vec<String> {
		let ask = rw.iter().filter(|v| allows(&self.src.settings, v)).cloned().collect();
		self.meta = Some(Meta { cmds, rw });
		ask
	}

	/// What upsd says of a writable variable.
	pub fn described(&mut self, var: String, info: VarInfo) {
		self.infos.insert(var, info);
	}

	/// A command's label: upsd's description of it; else, for an outlet's `load.on` and `load.off`,
	/// the outlet's name and `on` or `off`; else the command.
	fn command_label(&self, c: &CmdInfo) -> String {
		if let Some(d) = c.desc.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
			return d.to_string();
		}
		let outlet = outlet_of(&c.name).and_then(|(n, what)| {
			let word = match what {
				"load.on" => "on",
				"load.off" => "off",
				_ => return None,
			};
			match self.outlets.get(&n)?.get("desc")? {
				V::Text(desc) if !desc.trim().is_empty() => Some(format!("{}: {word}", desc.trim())),
				_ => None,
			}
		});
		outlet.unwrap_or_else(|| c.name.clone())
	}

	/// The station's actions named `name`, as far as upsd has said what it offers: its commands
	/// the source's `commands` allow, then each writable variable its `settings` allow that takes a
	/// list of values or a range. One that takes neither is said in the journal, once.
	pub(super) fn actions(&mut self, name: &str) -> Vec<Spec> {
		let Some(meta) = &self.meta else { return Vec::new() };
		let mut out = Vec::new();
		let cmds: Vec<&CmdInfo> = meta.cmds.iter().filter(|c| allows(&self.src.commands, &c.name)).collect();
		if !cmds.is_empty() {
			let choices = cmds.iter().map(|c| (c.name.clone(), self.command_label(c))).collect();
			let typed = cmds.iter().filter(|c| typed(&self.src, &c.name)).map(|c| c.name.clone()).collect();
			let param = Param { id: "cmd".into(), label: "Command".into(), kind: ParamKind::Choice(choices), optional: false, default: None, typed };
			out.push(spec(format!("nut.command.{}", self.id), name, "power", &self.src, param));
		}
		let mut vars: Vec<&String> = meta.rw.iter().filter(|v| allows(&self.src.settings, v)).collect();
		vars.sort();
		vars.dedup();
		let mut unoffered = Vec::new();
		for var in vars {
			let Some(info) = self.infos.get(var) else { continue };
			let Some(kind) = takes(var, info) else {
				unoffered.push(var.clone());
				continue;
			};
			let named = info.desc.as_deref().map(str::trim).filter(|d| !d.is_empty()).unwrap_or(var);
			let param = Param { id: "value".into(), label: "Value".into(), kind, optional: false, default: None, typed: Vec::new() };
			out.push(spec(format!("nut.set.{}.{var}", self.id), named, "control", &self.src, param));
		}
		for var in unoffered {
			if self.unoffered.insert(var.clone()) {
				super::recorder::say(&self.id, format!("{var} takes neither a list of values nor a range: not offered as a setting"));
			}
		}
		out
	}
}

/// Every station's actions in `snapshots`, by source as the settings have them.
pub fn of_sources(config: &crate::settings::Config, snapshots: &BTreeMap<String, std::sync::Arc<super::Snapshot>>) -> Vec<Spec> {
	config.sources.keys().filter_map(|id| snapshots.get(id)).flat_map(|s| s.actions.iter().cloned()).collect()
}

// ---- a request for one ----------------------------------------------------------------------

/// A station's command a client asks for that isn't among those offered: refused as not allowed,
/// before its parameters are looked at further. Another kind of `cmd` is a wrong parameter, which
/// checking the parameters finds.
pub fn offered(spec: &Spec, params: &serde_json::Map<String, Value>) -> Result<(), (ErrorCode, String)> {
	for p in spec.params.iter().filter(|p| p.id == "cmd") {
		if let (ParamKind::Choice(choices), Some(Value::String(cmd))) = (&p.kind, params.get(&p.id))
			&& !choices.iter().any(|(c, _)| c == cmd)
		{
			return Err((ErrorCode::NotAllowed, p.id.clone()));
		}
	}
	Ok(())
}

/// A choice that is typed in, chosen without its text (`confirm_text`, exactly): 403
/// `confirm_required`, the text in `message`. A text where none is needed is ignored.
pub fn typed_in(spec: &Spec, params: &BTreeMap<String, Value>, text: Option<&str>) -> Result<(), (ErrorCode, String)> {
	for p in &spec.params {
		if let Some(Value::String(v)) = params.get(&p.id)
			&& p.typed.contains(v)
			&& text != Some(v.as_str())
		{
			return Err((ErrorCode::ConfirmRequired, v.clone()));
		}
	}
	Ok(())
}

/// The longest password file read: its first line is all that is wanted.
const PASSWORD_FILE_MAX: u64 = 4096;

/// The NUT user of the source `id` and its password, the first line of its `password_file`, read
/// now. A file that is missing, can't be read, may be read by others or is empty fails, saying
/// which and never what is in it; the journal says where.
pub fn creds(id: &str, src: &NutSource) -> Result<Creds, String> {
	let (Some(user), Some(path)) = (&src.user, &src.password_file) else {
		return Err("the source has no user and password_file".into());
	};
	let fail = |what: &str, why: Option<std::io::Error>| {
		let why = why.map(|e| format!(" ({e})")).unwrap_or_default();
		journal(format!("nut {id}: password_file {} {what}{why}: nothing run", path.display()));
		format!("password_file {what}")
	};
	let mut file = match std::fs::File::open(path) {
		Ok(f) => f,
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(fail("is missing", Some(e))),
		Err(e) => return Err(fail("can't be read", Some(e))),
	};
	match file.metadata() {
		Ok(m) if m.permissions().mode() & 0o077 != 0 => return Err(fail("may be read by others: chmod 600 it", None)),
		Ok(_) => {}
		Err(e) => return Err(fail("can't be read", Some(e))),
	}
	let mut text = String::new();
	if let Err(e) = file.by_ref().take(PASSWORD_FILE_MAX).read_to_string(&mut text) {
		return Err(fail("can't be read", Some(e)));
	}
	let password = text.lines().next().unwrap_or_default().to_string();
	if password.is_empty() {
		return Err(fail("has no password on its first line", None));
	}
	Ok(Creds { user: user.clone(), password })
}

/// What a station's answer means for the client: done, or why not.
pub fn outcome(result: Result<String, Failure>) -> Result<(), String> {
	match result {
		Ok(s) if s == "done" => Ok(()),
		Ok(s) if s == "accepted by upsd" => Err(reason::NOT_CONFIRMED.into()),
		Ok(s) if s.starts_with("accepted, still pending") => Err(reason::TIMED_OUT.into()),
		Ok(s) => Err(s),
		Err(f) => Err(f.message),
	}
}

/// Runs a station's action whose request passed every check, for the phone `who`: its password
/// read, given to upsd once any other of its source has ended, the station's answer waited for.
pub async fn run(rt: &Runtime, who: &str, id: &str, src: &NutSource, params: &BTreeMap<String, Value>) -> Result<(), String> {
	let Some(target) = target(id) else { return Err(reason::UNKNOWN_ACTION.into()) };
	let (source, action) = match target {
		Target::Command { source } => {
			let cmd = params.get("cmd").and_then(Value::as_str).unwrap_or_default().to_string();
			(source, Action::Command { cmd, value: None })
		}
		Target::Set { source, var } => {
			let value = params.get("value").map(actions::argument).unwrap_or_default();
			(source, Action::Set { var: var.to_string(), value })
		}
	};
	let what = match &action {
		Action::Command { cmd, .. } => cmd.clone(),
		Action::Set { var, value } => format!("{var} {value}"),
	};
	let creds = creds(source, src)?;
	journal(format!("control: {who} asked {source} for {what}"));
	let result = match rt.act(source, action, creds).await {
		Ok(answer) => outcome(answer),
		Err(why) => Err(why.to_string()),
	};
	match &result {
		Ok(()) => journal(format!("nut {source}: {what}: done")),
		Err(e) => journal(format!("nut {source}: {what} failed: {e}")),
	}
	result
}

/// Writes `line` in the journal.
fn journal(line: String) {
	eprintln!("{line}");
	#[cfg(test)]
	tests::JOURNAL.lock().unwrap_or_else(|p| p.into_inner()).push(line);
}

#[cfg(test)]
pub mod tests {
	use std::sync::Mutex;

	use super::*;

	/// What `journal` wrote.
	pub static JOURNAL: Mutex<Vec<String>> = Mutex::new(Vec::new());

	fn src(text: &str) -> NutSource {
		let config = crate::settings::read_sources(&[crate::settings::Source::new("a.toml", text)]).unwrap();
		config.sources["eco"].clone()
	}

	#[test]
	fn what_is_typed_in() {
		let eco = src("[sources.eco]\nkind = \"nut\"\nups = \"ups\"\noutlets.\"1\".critical = true\n");
		// A critical outlet: all but turning it on.
		assert!(typed(&eco, "outlet.1.load.off") && typed(&eco, "outlet.1.load.cycle") && typed(&eco, "outlet.1.load.off.delay"));
		assert!(!typed(&eco, "outlet.1.load.on"));
		// Another outlet: asked as any action is, though upst-core counts its `load.off` as dangerous.
		assert!(dangerous("outlet.2.load.off") && !typed(&eco, "outlet.2.load.off") && !typed(&eco, "outlet.12.load.off"));
		// What isn't about one outlet: as upst-core counts it.
		for cmd in ["load.off", "load.off.delay", "shutdown.return", "shutdown.stayoff", "calibrate.start", "test.battery.start.deep", "bypass.start"] {
			assert!(typed(&eco, cmd), "{cmd}");
		}
		for cmd in ["load.on", "beeper.enable", "beeper.disable", "calibrate.stop", "test.battery.start.quick", "outlet.load.on"] {
			assert!(!typed(&eco, cmd), "{cmd}");
		}
		// Not an outlet's number: counted as a whole station's.
		assert!(typed(&eco, "outlet.x.load.off") && typed(&eco, "outlet.99999999999.load.off"));
		let plain = src("[sources.eco]\nkind = \"nut\"\nups = \"ups\"\n");
		assert!(!typed(&plain, "outlet.1.load.off") && typed(&plain, "load.off"));
	}

	#[test]
	fn a_command_not_offered_is_refused_before_its_parameters_and_a_typed_one_without_its_text() {
		let eco = src("[sources.eco]\nkind = \"nut\"\nups = \"ups\"\noutlets.\"1\".critical = true\n");
		let choices = vec![("outlet.1.load.off".to_string(), "Off".to_string()), ("outlet.1.load.on".to_string(), "On".to_string())];
		let param = Param { id: "cmd".into(), label: "Command".into(), kind: ParamKind::Choice(choices), optional: false, default: None, typed: vec!["outlet.1.load.off".into()] };
		let spec = spec("nut.command.eco".into(), "Eco", "power", &eco, param);
		let given = |v: Value| v.as_object().unwrap().clone();
		assert_eq!(offered(&spec, &given(serde_json::json!({ "cmd": "load.off" }))), Err((ErrorCode::NotAllowed, "cmd".into())));
		assert_eq!(offered(&spec, &given(serde_json::json!({ "cmd": "outlet.1.load.on\n" }))), Err((ErrorCode::NotAllowed, "cmd".into())));
		// Not a command at all, or something besides: for the parameters' check to say.
		assert_eq!(offered(&spec, &given(serde_json::json!({ "cmd": 5 }))), Ok(()));
		assert_eq!(offered(&spec, &given(serde_json::json!({ "cmd": "outlet.1.load.on", "extra": 1 }))), Ok(()));
		let params = |cmd: &str| BTreeMap::from([("cmd".to_string(), Value::from(cmd))]);
		let required = Err((ErrorCode::ConfirmRequired, "outlet.1.load.off".to_string()));
		assert_eq!(typed_in(&spec, &params("outlet.1.load.off"), None), required);
		assert_eq!(typed_in(&spec, &params("outlet.1.load.off"), Some("outlet.1.load.of")), required);
		assert_eq!(typed_in(&spec, &params("outlet.1.load.off"), Some("outlet.1.load.off")), Ok(()));
		assert_eq!(typed_in(&spec, &params("outlet.1.load.on"), Some("anything")), Ok(()));
	}

	#[test]
	fn ids_name_the_source_and_the_variable() {
		assert_eq!(target("nut.command.eco"), Some(Target::Command { source: "eco" }));
		assert_eq!(target("nut.set.eco.ups.beeper.status"), Some(Target::Set { source: "eco", var: "ups.beeper.status" }));
		assert_eq!(target("nut.command.eco.x"), None);
		assert_eq!(target("nut.set.eco"), None);
		assert_eq!(target("power.reboot"), None);
	}

	#[test]
	fn what_a_station_answered() {
		assert_eq!(outcome(Ok("done".into())), Ok(()));
		assert_eq!(outcome(Ok("accepted by upsd".into())), Err("accepted by upsd but not confirmed by the station".into()));
		assert_eq!(outcome(Ok("accepted, still pending after 15s".into())), Err("timed_out".into()));
		assert_eq!(outcome(Err(Failure { message: "invalid value".into(), auth: false })), Err("invalid value".into()));
	}

	#[test]
	fn a_variable_takes_its_list_or_its_ranges() {
		let info = |kind: &[&str], values: &[&str], ranges: &[(&str, &str)]| VarInfo {
			desc: None,
			kind: kind.iter().map(|s| s.to_string()).collect(),
			enum_values: values.iter().map(|s| s.to_string()).collect(),
			ranges: ranges.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
		};
		let choice = takes("ups.beeper.status", &info(&["RW", "ENUM"], &["enabled", "disabled"], &[]));
		assert_eq!(choice, Some(ParamKind::Choice(vec![("enabled".into(), "Enabled".into()), ("disabled".into(), "Disabled".into())])));
		let range = takes("battery.charge.low", &info(&["RW", "RANGE", "NUMBER"], &[], &[("10", "20"), ("50", "90")]));
		assert_eq!(range, Some(ParamKind::Range { min: 10.into(), max: 90.into(), step: 1.into(), unit: Some("%".into()) }));
		let unitless = takes("input.sensitivity.level", &info(&["RW", "RANGE"], &[], &[("0.5", "3")]));
		assert_eq!(unitless, Some(ParamKind::Range { min: serde_json::Number::from_f64(0.5).unwrap(), max: 3.into(), step: 1.into(), unit: None }));
		assert_eq!(takes("ups.id", &info(&["RW", "STRING:32"], &[], &[])), None);
		assert_eq!(takes("ups.delay", &info(&["RW", "RANGE"], &[], &[("x", "y")])), None);
	}
}
