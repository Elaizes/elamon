//! `[events]`, `[rules.<id>]` and `[notify.<id>]` of the settings, typed, and what `elamond check`
//! says of them. Nothing here has a value of its own: a key a rule or a target leaves out is
//! `[events.rule]`'s or `[events.target]`'s, and a key `[events]` leaves out is events.toml's, the
//! settings elamond starts with (`fill`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::access::Role;
use crate::settings::{self, Config};
use crate::values;

/// The part of the settings elamond starts with that is about events: `[events]` and the rules.
pub const DEFAULTS: &str = include_str!("../events.toml");

/// How much an event matters.
#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Lvl {
	Info,
	Warning,
	Critical,
}

pub const LEVELS: [&str; 3] = ["info", "warning", "critical"];
/// The levels a value turns to, which `when` waits for.
pub const WHEN: [&str; 2] = ["warning", "critical"];
pub const KINDS: [&str; 3] = ["value", "ups", "boot"];
/// What a target's `priority` and `tags` are keyed by: the levels, and a trouble's end.
pub const MOODS: [&str; 4] = ["info", "warning", "critical", "recovered"];
/// What a text may hold in braces.
pub const PLACEHOLDERS: [&str; 7] = ["value", "name", "duration", "rule", "source", "text", "computer"];
/// Of those, what `plain` and `recovered_plain` may: they go out without names.
pub const PLAIN_PLACEHOLDERS: [&str; 3] = ["value", "duration", "rule"];
/// And a target's `title`, `title_names`, `message` and `more`.
pub const TARGET_PLACEHOLDERS: [&str; 6] = ["text", "rule", "code", "level", "computer", "count"];

impl Lvl {
	pub fn as_str(self) -> &'static str {
		match self {
			Lvl::Info => "info",
			Lvl::Warning => "warning",
			Lvl::Critical => "critical",
		}
	}

	pub fn parse(s: &str) -> Option<Lvl> {
		[Lvl::Info, Lvl::Warning, Lvl::Critical].into_iter().find(|l| l.as_str() == s)
	}
}

/// `[events]`.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct EventsDef {
	pub keep: usize,
	pub keep_days: u64,
	pub role: Role,
	pub background: u64,
	/// The units `{value}` is told in: temperatures in °F, network rates in bit/s.
	pub fahrenheit: bool,
	pub network_bits: bool,
	pub notify: NotifyDef,
	/// `[events.rule]` and `[events.target]`: already in each rule and target.
	#[serde(default)]
	pub rule: Map<String, Value>,
	#[serde(default)]
	pub target: Map<String, Value>,
}

impl EventsDef {
	/// events.toml's `[events]`: what the settings have before a file says anything.
	pub fn starting() -> EventsDef {
		serde_json::from_value(Value::Object(built_in().clone())).unwrap_or_else(|e| panic!("events.toml's [events]: {e}"))
	}
}

/// `[events.notify]`: `elamond notify`.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct NotifyDef {
	pub level: Lvl,
	pub rule: String,
	pub code: String,
	pub length: usize,
	pub burst: u32,
	pub per: u64,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
	Value,
	Ups,
	Boot,
}

/// A condition on a value: the one of a rule, or its `also`.
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Cond {
	pub value: Option<String>,
	pub when: Option<String>,
	pub above: Option<f64>,
	pub below: Option<f64>,
	pub equals: Option<Value>,
	pub not: Option<Value>,
}

impl Cond {
	/// How many of `when`, `above`, `below`, `equals` and `not` it has: one, when sound.
	fn tests(&self) -> usize {
		[self.when.is_some(), self.above.is_some(), self.below.is_some(), self.equals.is_some(), self.not.is_some()].into_iter().filter(|b| *b).count()
	}
}

/// `[rules.<id>]`, with `[events.rule]` under it.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Rule {
	pub enabled: bool,
	pub kind: Kind,
	pub level: Lvl,
	#[serde(rename = "for")]
	pub hold: u64,
	pub clear_for: u64,
	pub clear: f64,
	pub again: u64,
	pub recovered: bool,
	pub recovered_level: Lvl,
	pub ends_always: bool,
	/// The event's code; the rule's id without one (a UPS's is upst-core's own).
	pub code: Option<String>,
	pub text: String,
	pub plain: String,
	pub recovered_text: String,
	pub recovered_plain: String,
	#[serde(flatten)]
	pub cond: Cond,
	pub also: Option<Cond>,
	/// `ups`: the source, every one without; the codes that begin and end the trouble.
	pub source: Option<String>,
	#[serde(default)]
	pub codes: Vec<String>,
	#[serde(default)]
	pub ends: Vec<String>,
	/// A priority and tags of its own, whatever the target's.
	pub priority: Option<u8>,
	pub tags: Option<Vec<String>>,
}

/// `[notify.<id>]`, with `[events.target]` under it: an ntfy topic.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Target {
	pub enabled: bool,
	pub url: String,
	pub topic_file: PathBuf,
	pub token_file: Option<PathBuf>,
	pub level: Lvl,
	pub names: bool,
	pub rules: Vec<String>,
	pub title: String,
	pub title_names: String,
	pub message: String,
	pub priority: BTreeMap<String, u8>,
	pub tags: BTreeMap<String, Vec<String>>,
	pub timeout: u64,
	pub retries: u32,
	pub retry: u64,
	pub retry_max: u64,
	pub queue: usize,
	pub expire: u64,
	pub burst: u32,
	pub per: u64,
	pub more: String,
}

/// events.toml's `[events]`, read once.
fn built_in() -> &'static Map<String, Value> {
	static TABLE: std::sync::OnceLock<Map<String, Value>> = std::sync::OnceLock::new();
	TABLE.get_or_init(|| {
		let table: toml::Table = DEFAULTS.parse().unwrap_or_else(|e| panic!("events.toml doesn't parse: {e}"));
		match serde_json::to_value(&table).ok().and_then(|v| v.get("events").cloned()) {
			Some(Value::Object(o)) => o,
			_ => panic!("events.toml has no [events]"),
		}
	})
}

/// `over` on top of `under`, tables key by key.
fn over(under: &Map<String, Value>, over: Option<&Value>) -> Value {
	let mut out = under.clone();
	if let Some(Value::Object(o)) = over {
		settings::merge(&mut out, o.clone());
	}
	Value::Object(out)
}

/// Fills in the merged settings what they leave out: `[events]`'s keys from events.toml, then each
/// rule's from `[events.rule]` and each target's from `[events.target]`. Rules and targets
/// themselves come from the files alone.
pub fn fill(settings: &mut Map<String, Value>) {
	let events = over(built_in(), settings.get("events"));
	let template = |name: &str| events.get(name).and_then(Value::as_object).cloned().unwrap_or_default();
	for (table, under) in [("rules", template("rule")), ("notify", template("target"))] {
		if let Some(Value::Object(all)) = settings.get_mut(table) {
			for one in all.values_mut() {
				*one = over(&under, Some(one));
			}
		}
	}
	settings.insert("events".into(), events);
}

/// The names in braces in `text`.
pub fn placeholders(text: &str) -> Vec<&str> {
	let mut out = Vec::new();
	let mut rest = text;
	while let Some(at) = rest.find('{') {
		rest = &rest[at + 1..];
		match rest.find('}') {
			Some(end) => {
				out.push(&rest[..end]);
				rest = &rest[end + 1..];
			}
			None => break,
		}
	}
	out
}

/// A rule's address with `*` for a thing's key: what is before and after it.
pub fn star(address: &str) -> Option<(&str, &str)> {
	address.split_once('*')
}

/// What the settings can tell is wrong with the rules and the targets, without reading anything.
pub fn check(c: &Config, push: &mut impl FnMut(String, String)) {
	let known = |text: &str, path: String, allowed: &[&str], push: &mut dyn FnMut(String, String)| {
		for p in placeholders(text).into_iter().filter(|p| !allowed.contains(p)) {
			let hint = settings::suggest(p, allowed.iter().copied()).map(|s| format!("; did you mean {{{s}}}?")).unwrap_or_default();
			push(path.clone(), format!("no {{{p}}} here: {}{hint}", allowed.iter().map(|a| format!("{{{a}}}")).collect::<Vec<_>>().join(", ")));
		}
	};
	for (id, r) in &c.rules {
		let path = settings::join("rules", id);
		let at = |key: &str| settings::join(&path, key);
		for (key, text) in [("text", &r.text), ("recovered_text", &r.recovered_text)] {
			known(text, at(key), &PLACEHOLDERS, push);
		}
		// What is sent where names aren't: nothing that is one.
		for (key, text) in [("plain", &r.plain), ("recovered_plain", &r.recovered_plain)] {
			known(text, at(key), &PLAIN_PLACEHOLDERS, push);
		}
		if let Some(code) = &r.code
			&& (code.is_empty() || code.chars().any(|ch| !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-'))))
		{
			push(at("code"), format!("\"{code}\" isn't a code: lower-case letters, digits, dots, - and _"));
		}
		if r.priority.is_some_and(|p| !(1..=5).contains(&p)) {
			push(at("priority"), "ntfy's priorities are 1 to 5".into());
		}
		let of_value = r.cond != Cond::default() || r.also.is_some();
		let of_ups = r.source.is_some() || !r.codes.is_empty() || !r.ends.is_empty();
		match r.kind {
			Kind::Value => {
				if of_ups {
					push(path.clone(), "source, codes and ends are a rule's of kind = \"ups\"".into());
				}
				check_cond(c, &r.cond, &path, true, push);
				if let Some(also) = &r.also {
					check_cond(c, also, &at("also"), false, push);
				}
			}
			Kind::Ups => {
				if of_value {
					push(path.clone(), "a rule of kind = \"ups\" goes by codes, not by a value".into());
				}
				if r.codes.is_empty() {
					push(at("codes"), "a rule of kind = \"ups\" needs codes: what upst-core calls what begins it (power.lost, battery.low…)".into());
				}
				if let Some(s) = r.source.as_deref().filter(|s| !c.sources.contains_key(*s)) {
					let hint = settings::suggest(s, c.sources.keys().map(String::as_str)).map(|h| format!("; did you mean {h}?")).unwrap_or_default();
					push(at("source"), format!("no source \"{s}\" of kind nut{hint}"));
				}
			}
			Kind::Boot => {
				if of_value || of_ups {
					push(path.clone(), "a rule of kind = \"boot\" takes no value and no codes".into());
				}
			}
		}
	}
	for (id, t) in &c.notify {
		let path = settings::join("notify", id);
		let at = |key: &str| settings::join(&path, key);
		if let Err(why) = settings::url_shape(&t.url) {
			push(at("url"), why.to_string());
		}
		for r in t.rules.iter().filter(|r| !c.rules.contains_key(*r) && **r != c.events.notify.rule) {
			let hint = settings::suggest(r, c.rules.keys().map(String::as_str)).map(|h| format!("; did you mean {h}?")).unwrap_or_default();
			push(at("rules"), format!("no rule \"{r}\"{hint}"));
		}
		for (key, text) in [("title", &t.title), ("title_names", &t.title_names), ("message", &t.message), ("more", &t.more)] {
			known(text, at(key), &TARGET_PLACEHOLDERS, push);
		}
		for (mood, p) in &t.priority {
			if !(1..=5).contains(p) {
				push(at("priority"), format!("{mood} = {p}: ntfy's priorities are 1 to 5"));
			}
		}
		for mood in MOODS {
			if !t.priority.contains_key(mood) {
				push(at("priority"), format!("has no {mood}"));
			}
		}
		for (key, mood) in t.priority.keys().map(|m| ("priority", m)).chain(t.tags.keys().map(|m| ("tags", m))) {
			if !MOODS.contains(&mood.as_str()) {
				let hint = settings::suggest(mood, MOODS).map(|h| format!("; did you mean {h}?")).unwrap_or_default();
				push(at(key), format!("no \"{mood}\" here: {}{hint}", MOODS.join(", ")));
			}
		}
		if t.retry_max < t.retry {
			push(at("retry_max"), "is less than retry".into());
		}
	}
}

/// A condition as the settings can tell it is sound: a value this computer or a source has, and
/// one test of it.
fn check_cond(c: &Config, cond: &Cond, path: &str, starred: bool, push: &mut impl FnMut(String, String)) {
	let Some(address) = cond.value.as_deref() else {
		push(path.to_string(), "needs value: the address it looks at (`elamond values` lists them)".into());
		return;
	};
	let at = settings::join(path, "value");
	let stars = address.matches('*').count();
	if stars > 1 || (stars == 1 && !starred) {
		push(at.clone(), if starred { "one * at most, in place of a thing's key".into() } else { "takes no *: one value".into() });
	}
	// A source's values are known only once it is read; the computer's own are listed.
	let head = address.split('.').next().unwrap_or_default();
	let of_source = c.sources.contains_key(head) || c.feeds.contains_key(head);
	let plain = address.replace('*', "x");
	if !of_source && values::address_def(&plain).is_none() {
		let hint = values::suggest_address(&plain).map(|s| format!("; did you mean {s}?")).unwrap_or_default();
		push(at, format!("no value \"{address}\" (`elamond values` lists them; a source's begin with its id){hint}"));
	}
	match cond.tests() {
		1 => {}
		0 => push(path.to_string(), "needs one of when, above, below, equals and not".into()),
		_ => push(path.to_string(), "takes one of when, above, below, equals and not".into()),
	}
}

/// What `elamond check` warns of: a rule on a code upst-core doesn't give, a topic or token
/// file that isn't private.
pub fn warnings(c: &Config, out: &mut Vec<String>) {
	for (id, r) in c.rules.iter().filter(|(_, r)| r.kind == Kind::Ups) {
		for (key, code) in r.codes.iter().map(|c| ("codes", c)).chain(r.ends.iter().map(|c| ("ends", c))) {
			if !crate::nut::CODES.contains(&code.as_str()) {
				let hint = settings::suggest(code, crate::nut::CODES.iter().copied()).map(|h| format!("; did you mean {h}?")).unwrap_or_default();
				out.push(format!("rules.{id}.{key}: upst-core {} gives no \"{code}\"{hint}", crate::nut::CODES_OF));
			}
		}
	}
	for (id, t) in &c.notify {
		out.extend(settings::secret_file_warning(&format!("notify.{id}.topic_file"), &t.topic_file, "its events aren't sent until it is"));
		if let Some(p) = &t.token_file {
			out.extend(settings::secret_file_warning(&format!("notify.{id}.token_file"), p, "its events aren't sent until it is"));
		}
	}
}

/// What `elamond check` says of the events of sound settings: how many rules are on, and where
/// their events go.
pub fn summary(c: &Config) -> String {
	let on = c.rules.values().filter(|r| r.enabled).count();
	let rules = format!("{on} rule{} on", if on == 1 { "" } else { "s" });
	let targets: Vec<&str> = c.notify.iter().filter(|(_, t)| t.enabled).map(|(id, _)| id.as_str()).collect();
	if targets.is_empty() {
		format!("Events: {rules}; kept and shown, sent nowhere: no [notify.<id>] names an ntfy topic.")
	} else {
		format!("Events: {rules}; sent on to {}.", targets.join(", "))
	}
}

/// What only the rules need read: the heads of their values' addresses (a `command`, `http` or
/// `file` source's id among them), whether a link's, whether a GPU's, and the UPS sources their
/// rules hear or look at, which are asked on for them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Needs {
	pub heads: std::collections::BTreeSet<String>,
	pub links: bool,
	pub gpus: bool,
	pub ups: std::collections::BTreeSet<String>,
}

impl Needs {
	pub fn of(c: &Config) -> Needs {
		let mut n = Needs::default();
		for r in c.rules.values().filter(|r| r.enabled && r.kind == Kind::Value) {
			for a in [r.cond.value.as_deref(), r.also.as_ref().and_then(|a| a.value.as_deref())].into_iter().flatten() {
				let head = a.split('.').next().unwrap_or_default();
				n.links |= head == "link";
				n.gpus |= head == "gpu";
				n.heads.insert(head.to_string());
				if c.sources.contains_key(head) {
					n.ups.insert(head.to_string());
				}
			}
		}
		for r in c.rules.values().filter(|r| r.enabled && r.kind == Kind::Ups) {
			match &r.source {
				Some(s) => drop(n.ups.insert(s.clone())),
				None => n.ups.extend(c.sources.keys().cloned()),
			}
		}
		n
	}
}
