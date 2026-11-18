//! The rules at work: each tick, every enabled rule of the settings is held against the reading's
//! values, what the UPS sources said since the last and the boot, and a rule that turns makes an
//! event. A rule is calm or in trouble, for each thing it looks at; that is kept in a file, so a
//! restart or new settings tell nothing twice. Time comes from outside: nothing here looks at a clock.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::def::{star, Cond, Kind, Rule};
use super::{Draft, Phase};
use crate::access::Role;
use crate::format;
use crate::settings::Config;
use crate::values::{Level, Scalar, Units, Values, View};

/// The rules' states, in the data directory.
pub const STATE_FILE: &str = "events-state.json";

/// What a source says with the events of its first reading since it started (after them): from
/// here on it tells every change, and a trouble it doesn't tell of now isn't there.
pub const WATCHING: &str = "elamon.watching";

/// What a UPS source said: an event of upst-core, by its code and in its words.
#[derive(Clone, Debug, PartialEq)]
pub struct Heard {
	pub source: String,
	pub code: String,
	pub text: String,
	/// Unix ms.
	pub t: u64,
}

/// What a UPS said that began a trouble: its code and its words.
type Said = Option<(String, String)>;

/// A rule's state for one thing: calm, or in trouble since `since`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
struct St {
	fired: bool,
	/// Unix ms the trouble began.
	since: u64,
	/// When its last event of a trouble's start was made, 0 for none: `again` counts from it.
	last: u64,
	/// The trouble began within `again` of the one before: neither it nor its end is an event.
	silent: bool,
	/// Since when the trouble shows, before `for` has passed; and the calm, before `clear_for`.
	#[serde(skip)]
	pending: Option<u64>,
	#[serde(skip)]
	clearing: Option<u64>,
	/// What the UPS said that began it, while it waits out `for`.
	#[serde(skip)]
	said: Said,
}

#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
struct Saved {
	/// The boot the service last saw, while a rule of kind `boot` is on.
	boot: Option<String>,
	/// By the rule's id, then `/` and the thing's key or the source's id where there is one.
	states: BTreeMap<String, St>,
}

/// What a condition comes to with a reading.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Truth {
	Bad,
	Good,
	/// Past the limit no longer, not yet `clear` back from it.
	Between,
	/// No value, or none the test can judge: nothing changes.
	Missing,
}

/// A condition against the value it looks at.
fn test(cond: &Cond, clear: f64, s: Option<&Scalar>) -> Truth {
	let Some(s) = s else { return Truth::Missing };
	let side = |bad: bool, good: bool| if bad { Truth::Bad } else if good { Truth::Good } else { Truth::Between };
	if let Some(want) = cond.when.as_deref().and_then(Level::parse) {
		let level = match (&s.limits, s.v.num()) {
			(Some(l), Some(x)) => Some(l.judge(x).0),
			_ => s.level,
		};
		return level.map_or(Truth::Missing, |l| side(l >= want, true));
	}
	if let Some(a) = cond.above {
		return s.v.num().map_or(Truth::Missing, |x| side(x > a, x <= a - clear));
	}
	if let Some(b) = cond.below {
		return s.v.num().map_or(Truth::Missing, |x| side(x < b, x >= b + clear));
	}
	if let Some(v) = &cond.equals {
		return side(s.v.matches(v), true);
	}
	if let Some(v) = &cond.not {
		return side(!s.v.matches(v), true);
	}
	Truth::Missing
}

/// The addresses a rule's `value` stands for, each with the key in its `*`'s place.
fn addresses(pattern: &str, values: &Values) -> Vec<(Option<String>, String)> {
	let Some((before, after)) = star(pattern) else { return vec![(None, pattern.to_string())] };
	values
		.scalars
		.keys()
		.filter_map(|a| {
			let key = a.strip_prefix(before)?.strip_suffix(after)?;
			(!key.is_empty() && !key.contains('.')).then(|| (Some(key.to_string()), a.clone()))
		})
		.collect()
}

/// `template` with each `{name}` of `with` filled in, in one pass: what is filled in is never
/// read again, so a value that holds braces of its own brings in nothing.
pub fn fill(template: &str, with: &[(&str, &str)]) -> String {
	let mut out = String::with_capacity(template.len());
	let mut rest = template;
	while let Some(at) = rest.find('{') {
		out.push_str(&rest[..at]);
		let known = rest[at + 1..].find('}').and_then(|end| {
			let name = &rest[at + 1..at + 1 + end];
			with.iter().find(|(n, _)| *n == name).map(|(_, value)| (*value, at + end + 2))
		});
		match known {
			Some((value, after)) => {
				out.push_str(value);
				rest = &rest[after..];
			}
			None => {
				out.push('{');
				rest = &rest[at + 1..];
			}
		}
	}
	out.push_str(rest);
	out
}

fn duration(ms: u64) -> String {
	format::format(format::Shown::Num((ms / 1000) as f64), "duration", "s")
}

/// What a turn of a rule is told with.
struct Told<'a> {
	id: &'a str,
	rule: &'a Rule,
	computer: &'a str,
	name: &'a str,
	source: &'a str,
	value: &'a str,
	/// upst-core's words, and its code.
	text: &'a str,
	code: Option<&'a str>,
}

impl Told<'_> {
	fn draft(&self, t: u64, phase: Phase, lasted: Option<u64>) -> Draft {
		let r = self.rule;
		let lasted = lasted.map(duration).unwrap_or_default();
		let with = [
			("value", self.value),
			("name", self.name),
			("duration", lasted.as_str()),
			("rule", self.id),
			("source", self.source),
			("text", self.text),
			("computer", self.computer),
		];
		let (text, plain, level) = match phase {
			Phase::End => (&r.recovered_text, &r.recovered_plain, r.recovered_level),
			_ => (&r.text, &r.plain, r.level),
		};
		Draft {
			t,
			level,
			of: r.level,
			rule: self.id.to_string(),
			code: self.code.map(str::to_string).or_else(|| r.code.clone()).unwrap_or_else(|| self.id.to_string()),
			phase,
			text: fill(text, &with),
			plain: fill(plain, &with),
			title: None,
			priority: r.priority,
			tags: r.tags.clone(),
		}
	}
}

/// The rules' states, and where they are kept.
pub struct Engine {
	path: PathBuf,
	saved: Saved,
	dirty: bool,
	/// The boot has been looked at since the service started.
	booted: bool,
}

impl Engine {
	/// The states kept in `dir`; none where the file isn't there or doesn't read.
	pub fn load(dir: &Path) -> Engine {
		let path = dir.join(STATE_FILE);
		let saved = match std::fs::read(&path) {
			Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
				eprintln!("events: {} doesn't read ({e}); the rules start calm", path.display());
				Saved::default()
			}),
			Err(_) => Saved::default(),
		};
		Engine { path, saved, dirty: false, booted: false }
	}

	/// Where the states are kept.
	#[cfg(test)]
	pub fn path(&self) -> &Path {
		&self.path
	}

	/// Writes the states, if they changed since they were last written.
	pub fn save(&mut self) {
		if !self.dirty {
			return;
		}
		self.dirty = false;
		let written = serde_json::to_vec(&self.saved).map_err(std::io::Error::other).and_then(|b| crate::store::write_private(&self.path, &b));
		if let Err(e) = written {
			eprintln!("events: can't write {}: {e}", self.path.display());
		}
	}

	/// The trouble of `key` begins at `t`; whether it is an event (not within `again` of the last).
	fn begin(&mut self, key: &str, t: u64, again: u64) -> bool {
		let st = self.saved.states.entry(key.to_string()).or_default();
		st.silent = st.last > 0 && t.saturating_sub(st.last) < again * 1000;
		st.fired = true;
		st.since = t;
		st.pending = None;
		st.clearing = None;
		if !st.silent {
			st.last = t;
		}
		self.dirty = true;
		!st.silent
	}

	/// The trouble of `key` ends; how long it lasted, if its start was an event.
	fn end(&mut self, key: &str, t: u64) -> Option<u64> {
		let st = self.saved.states.get_mut(key)?;
		let told = !st.silent;
		let lasted = t.saturating_sub(st.since);
		st.fired = false;
		st.silent = false;
		st.clearing = None;
		self.dirty = true;
		told.then_some(lasted)
	}

	/// The events the rules of `config` make of the reading `values` (its `t` the time), of what
	/// the UPS sources said since the last tick, and of the boot `boot`, on the computer named `computer`.
	pub fn tick(&mut self, config: &Config, values: &Values, heard: &[Heard], computer: &str, boot: Option<&str>) -> Vec<Draft> {
		let now = values.t;
		let mut out = Vec::new();
		// A rule taken out or turned off is forgotten, its trouble with it.
		let on = |key: &str| config.rules.get(key.split('/').next().unwrap_or(key)).is_some_and(|r| r.enabled);
		let before = self.saved.states.len();
		self.saved.states.retain(|k, _| on(k));
		self.dirty |= self.saved.states.len() != before;
		// Names and all: a target that sends no names sends `plain`, which holds none.
		let view = View::new(values, Role::Admin, Units { fahrenheit: config.events.fahrenheit, bits: config.events.network_bits });
		for (id, rule) in config.rules.iter().filter(|(_, r)| r.enabled) {
			match rule.kind {
				Kind::Value => self.value(id, rule, &view, computer, now, &mut out),
				Kind::Ups => self.ups(id, rule, heard, computer, now, &mut out),
				Kind::Boot => self.boot(id, rule, computer, boot, now, &mut out),
			}
		}
		if self.saved.boot.is_some() && !config.rules.values().any(|r| r.enabled && r.kind == Kind::Boot) {
			self.saved.boot = None;
			self.dirty = true;
		}
		self.booted = true;
		out
	}

	fn value(&mut self, id: &str, rule: &Rule, view: &View, computer: &str, now: u64, out: &mut Vec<Draft>) {
		let Some(pattern) = rule.cond.value.as_deref() else { return };
		let also = rule.also.as_ref().map(|a| test(a, rule.clear, a.value.as_deref().and_then(|v| view.values.scalars.get(v))));
		for (name, address) in addresses(pattern, view.values) {
			let own = test(&rule.cond, rule.clear, view.values.scalars.get(&address));
			let truth = match (own, also) {
				(Truth::Good, _) | (_, Some(Truth::Good)) => Truth::Good,
				(Truth::Missing, _) | (_, Some(Truth::Missing)) => Truth::Missing,
				(t, None | Some(Truth::Bad)) => t,
				(_, Some(Truth::Between)) => Truth::Between,
			};
			let key = name.as_ref().map_or_else(|| id.to_string(), |n| format!("{id}/{n}"));
			// A thing that is calm and has nothing to remember has no state kept.
			let mut st = self.saved.states.get(&key).cloned().unwrap_or_default();
			let turn = match (st.fired, truth) {
				(_, Truth::Missing) => None,
				(false, Truth::Bad) => (now.saturating_sub(*st.pending.get_or_insert(now)) >= rule.hold * 1000).then_some(Phase::Start),
				(false, _) => {
					st.pending = None;
					None
				}
				(true, Truth::Good) => (now.saturating_sub(*st.clearing.get_or_insert(now)) >= rule.clear_for * 1000).then_some(Phase::End),
				(true, _) => {
					st.clearing = None;
					None
				}
			};
			let st_began = st.pending;
			let blank = !st.fired && st.pending.is_none() && (st.last == 0 || now.saturating_sub(st.last) >= rule.again * 1000);
			if !blank {
				self.saved.states.insert(key.clone(), st);
			} else if self.saved.states.remove(&key).is_some_and(|was| was.last > 0) {
				self.dirty = true;
			}
			let Some(phase) = turn else { continue };
			// The trouble is as old as its first sign, `for` included.
			let began = st_began.unwrap_or(now);
			let value = view.text(&address, None).unwrap_or_default();
			let told = Told { id, rule, computer, name: name.as_deref().unwrap_or_default(), source: "", value: &value, text: "", code: None };
			match phase {
				Phase::End => {
					if let Some(lasted) = self.end(&key, now).filter(|_| rule.recovered) {
						out.push(told.draft(now, Phase::End, Some(lasted)));
					}
				}
				_ => {
					if self.begin(&key, began, rule.again) {
						out.push(told.draft(now, Phase::Start, None));
					}
				}
			}
		}
	}

	fn ups(&mut self, id: &str, rule: &Rule, heard: &[Heard], computer: &str, now: u64, out: &mut Vec<Draft>) {
		let told = |source: &'_ str, text: &'_ str, code: &'_ str, t: u64, phase: Phase, lasted: Option<u64>| {
			Told { id, rule, computer, name: source, source, value: "", text, code: Some(code) }.draft(t, phase, lasted)
		};
		for h in heard.iter().filter(|h| rule.source.as_deref().is_none_or(|s| s == h.source)) {
			let key = format!("{id}/{}", h.source);
			// The source started anew (the service did, or its settings changed) and has told what
			// it found: a trouble kept from before that it didn't tell of again is over, how and
			// when unknown, so without an event. Else it would stay, and the next one go untold.
			if h.code == WATCHING {
				let restated = heard.iter().any(|o| o.source == h.source && rule.codes.contains(&o.code));
				if let Some(st) = self.saved.states.get_mut(&key).filter(|st| st.fired && !restated) {
					st.fired = false;
					st.silent = false;
					self.dirty = true;
				}
				continue;
			}
			let begins = rule.codes.contains(&h.code);
			let ends = rule.ends.contains(&h.code);
			// Without `ends`, each of its codes stands alone.
			if begins && rule.ends.is_empty() {
				let st = self.saved.states.entry(key).or_default();
				if st.last == 0 || h.t.saturating_sub(st.last) >= rule.again * 1000 {
					st.last = h.t;
					self.dirty = true;
					out.push(told(&h.source, &h.text, &h.code, h.t, Phase::Once, None));
				}
				continue;
			}
			let (fired, pending) = {
				let st = self.saved.states.entry(key.clone()).or_default();
				(st.fired, st.pending.is_some())
			};
			if begins && !fired && !pending {
				if let Some(st) = self.saved.states.get_mut(&key) {
					st.pending = Some(h.t);
					st.said = Some((h.code.clone(), h.text.clone()));
				}
			} else if ends && fired {
				if let Some(lasted) = self.end(&key, h.t).filter(|_| rule.recovered) {
					out.push(told(&h.source, &h.text, &h.code, h.t, Phase::End, Some(lasted)));
				}
			} else if ends && pending {
				// Over before `for` had passed: nothing to tell.
				if let Some(st) = self.saved.states.get_mut(&key) {
					st.pending = None;
					st.said = None;
				}
			} else if ends && rule.ends_always && rule.recovered {
				out.push(told(&h.source, &h.text, &h.code, h.t, Phase::End, None));
			}
		}
		// A trouble that has lasted `for` begins.
		let due: Vec<(String, u64, Said)> = self
			.saved
			.states
			.iter()
			.filter(|(k, st)| k.strip_prefix(id).is_some_and(|rest| rest.starts_with('/')) && !st.fired)
			.filter_map(|(k, st)| st.pending.filter(|p| now.saturating_sub(*p) >= rule.hold * 1000).map(|p| (k.clone(), p, st.said.clone())))
			.collect();
		for (key, since, said) in due {
			let source = key.split_once('/').map_or("", |(_, s)| s).to_string();
			let (code, text) = said.unwrap_or_default();
			if self.begin(&key, since, rule.again) {
				out.push(told(&source, &text, &code, now, Phase::Start, None));
			}
			if let Some(st) = self.saved.states.get_mut(&key) {
				st.said = None;
			}
		}
	}

	fn boot(&mut self, id: &str, rule: &Rule, computer: &str, boot: Option<&str>, now: u64, out: &mut Vec<Draft>) {
		let Some(boot) = boot.filter(|_| !self.booted) else { return };
		// The first boot seen is where it counts from.
		if self.saved.boot.as_deref().is_some_and(|b| b != boot) {
			out.push(Told { id, rule, computer, name: "", source: "", value: "", text: "", code: None }.draft(now, Phase::Once, None));
		}
		if self.saved.boot.as_deref() != Some(boot) {
			self.saved.boot = Some(boot.to_string());
			self.dirty = true;
		}
	}
}

/// This boot's id, as the kernel gives it.
pub fn boot_id() -> Option<String> {
	std::fs::read_to_string("/proc/sys/kernel/random/boot_id").ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}
