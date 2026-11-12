//! The settings, schema 2 (docs/config.md): what is built in (defaults.toml), then elamond.toml,
//! then elamond.d/*.toml, a table with an id merging key by key with the one before and a list
//! replacing it whole. Each file is checked on its own first, so a mistake names the file and the
//! line it is in; then the settings as merged, for what refers to what. The same definitions give
//! the JSON Schema `elamond schema` prints.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::access::Role;
use crate::actions;
use crate::format;
use crate::listen::Target;
use crate::values;

/// The standard dashboards and the default profile, read before any file.
pub const DEFAULTS: &str = include_str!("defaults.toml");
/// What mistakes in them are said to be in.
pub const DEFAULTS_NAME: &str = "built-in defaults";

pub const WIDGET_TYPES: [&str; 15] =
	["value", "gauge", "meter", "status", "graph", "list", "bars", "table", "button", "toggle", "select", "slider", "text", "group", "media"];
pub const ICONS: [&str; 32] = [
	"battery", "bolt", "chip", "clock", "control", "eye", "fan", "gauge", "globe", "gpu", "help", "laptop", "leaf", "lock", "memory", "monitor",
	"moon", "music", "network", "next", "overview", "pause", "play", "power", "previous", "processes", "restart", "rocket", "storage", "terminal",
	"thermometer", "volume",
];
pub const COLORS: [&str; 15] =
	["cpu", "memory", "swap", "gpu", "rx", "tx", "read", "write", "battery", "good", "warn", "hot", "primary", "secondary", "tertiary"];
pub const ROLES: [&str; 4] = ["glance", "monitor", "operate", "admin"];
pub const LEVELS: [&str; 3] = ["normal", "warning", "critical"];
pub const THEMES: [&str; 3] = ["system", "light", "dark"];
pub const GRACES: [&str; 3] = ["every_time", "1m", "5m"];
pub const HEIGHTS: [&str; 3] = ["small", "medium", "large"];
pub const SCOPES: [&str; 2] = ["user", "system"];
/// The roles an action may ask for.
pub const ACTION_ROLES: [&str; 2] = ["operate", "admin"];
/// The widgets that are 4 columns wide unless they say otherwise.
pub const WIDE: [&str; 6] = ["graph", "table", "list", "group", "media", "text"];

// ---- the settings, typed ------------------------------------------------------------------

pub type WidgetDef = Map<String, Value>;

#[derive(Deserialize, Debug, Clone)]
#[serde(default = "Config::bare")]
pub struct Config {
	/// Addresses and interfaces to listen on; "::", the default, takes every address, IPv4 too.
	#[serde(deserialize_with = "crate::listen::deserialize")]
	pub listen: Vec<Target>,
	pub port: u16,
	/// Host names that reach this computer, for `elamond pair` to offer after its addresses.
	#[serde(deserialize_with = "crate::names::deserialize")]
	pub names: Vec<String>,
	/// Name shown in the app; the hostname when empty.
	pub name: String,
	/// Announce the service on the local network so the app finds it.
	pub mdns: bool,
	pub interval_ms: u64,
	pub history_minutes: u64,
	/// Let paired clients act on this computer, as far as their roles allow. Off unless turned on.
	pub control: bool,
	/// The role a client pairs with when `elamond pair` isn't given one.
	pub default_role: Role,
	/// The profile of a client paired without one, and of one whose profile is gone.
	pub default_profile: String,
	pub links: BTreeMap<String, Link>,
	pub actions: BTreeMap<String, Action>,
	pub profiles: BTreeMap<String, ProfileDef>,
	pub dashboards: BTreeMap<String, Dashboard>,
	pub templates: BTreeMap<String, WidgetDef>,
}

/// A web page this computer hosts.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Link {
	pub name: String,
	/// What a client opens; "{host}" becomes the address it reaches this computer at.
	pub url: String,
	/// A systemd service unit clients may start, stop and restart.
	#[serde(default)]
	pub unit: Option<String>,
	#[serde(default)]
	pub scope: Option<String>,
	#[serde(default)]
	pub icon: Option<String>,
}

impl Link {
	/// "user" or "system".
	pub fn scope(&self) -> &str {
		self.scope.as_deref().unwrap_or("user")
	}
}

/// A command a client may run, with the values it may choose among.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Action {
	pub name: String,
	/// Program and arguments, run without a shell; an argument that is exactly "{param}" is
	/// replaced by that parameter's value.
	pub command: Vec<String>,
	#[serde(default)]
	pub params: BTreeMap<String, ParamDef>,
	#[serde(default = "operate")]
	pub role: Role,
	#[serde(default = "yes")]
	pub confirm: bool,
	#[serde(default)]
	pub unlock: bool,
	/// Seconds before the command is stopped.
	#[serde(default = "minute")]
	pub timeout: u64,
	#[serde(default)]
	pub icon: Option<String>,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(default)]
pub struct ParamDef {
	pub choices: Option<Vec<String>>,
	pub labels: Option<BTreeMap<String, String>>,
	pub range: Option<Vec<serde_json::Number>>,
	pub step: Option<serde_json::Number>,
	pub unit: Option<String>,
	#[serde(rename = "type")]
	pub kind: Option<String>,
	pub default: Option<Value>,
	pub label: Option<String>,
}

fn operate() -> Role {
	Role::Operate
}

fn yes() -> bool {
	true
}

fn minute() -> u64 {
	crate::consts::timeout::ACTION_DEFAULT_SECS
}

/// A profile as the settings give it; `Config::profile` resolves its `use` chain.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct ProfileDef {
	#[serde(rename = "use")]
	pub base: Option<String>,
	pub name: Option<String>,
	pub theme: Option<String>,
	pub pure_black: Option<bool>,
	pub animations: Option<bool>,
	pub interval_ms: Option<u64>,
	pub fahrenheit: Option<bool>,
	pub network_bits: Option<bool>,
	pub confirm_actions: Option<bool>,
	pub unlock: Option<UnlockDef>,
	pub dashboards: Option<Vec<String>>,
	pub header: Option<String>,
	pub android: Option<BTreeMap<String, bool>>,
	pub web: Option<Map<String, Value>>,
}

#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct UnlockDef {
	#[serde(rename = "for")]
	pub kinds: Option<Vec<String>>,
	pub grace: Option<String>,
}

/// A profile, its `use` chain followed.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
	pub id: String,
	pub name: String,
	pub theme: String,
	pub pure_black: bool,
	pub animations: bool,
	pub interval_ms: u64,
	pub fahrenheit: bool,
	pub network_bits: bool,
	pub confirm_actions: bool,
	pub unlock_for: Vec<String>,
	pub unlock_grace: String,
	pub dashboards: Vec<String>,
	/// The dashboard shown as chips under the computer's name; none when empty.
	pub header: String,
	pub android: BTreeMap<String, bool>,
	pub web: Map<String, Value>,
}

/// The keys of `[profiles.<id>.android]` and their defaults.
pub const ANDROID: [(&str, bool); 4] = [("dynamic_color", true), ("blur", true), ("haptics", true), ("keep_screen_on", false)];

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Dashboard {
	pub title: String,
	#[serde(default)]
	pub icon: Option<String>,
	#[serde(default)]
	pub role: Option<Role>,
	#[serde(default)]
	pub for_each: Option<String>,
	#[serde(default)]
	pub widgets: Vec<WidgetDef>,
}

/// A widget, its template applied: every key any type takes.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Widget {
	#[serde(rename = "type")]
	pub kind: Option<String>,
	pub id: Option<String>,
	pub span: Option<u8>,
	pub label: Option<String>,
	pub icon: Option<String>,
	pub color: Option<String>,
	pub role: Option<Role>,
	pub when: Option<String>,
	pub open: Option<String>,
	pub caption: Option<String>,
	pub collapsed: Option<bool>,
	#[serde(rename = "use")]
	pub template: Option<String>,
	pub with: Option<Map<String, Value>>,
	pub for_each: Option<ForEach>,
	pub value: Option<String>,
	pub unit: Option<String>,
	pub format: Option<String>,
	pub map: Option<BTreeMap<String, String>>,
	pub empty: Option<String>,
	pub warning_above: Option<Bound>,
	pub critical_above: Option<Bound>,
	pub warning_below: Option<Bound>,
	pub critical_below: Option<Bound>,
	pub soft: Option<f64>,
	pub levels: Option<BTreeMap<String, String>>,
	pub spark: Option<bool>,
	pub min: Option<Bound>,
	pub max: Option<Bound>,
	pub values: Option<Vec<String>>,
	pub labels: Option<Labels>,
	pub colors: Option<Vec<String>>,
	pub range: Option<u64>,
	pub height: Option<String>,
	pub floor: Option<serde_json::Number>,
	pub items: Option<Vec<ListItem>>,
	pub source: Option<String>,
	pub field: Option<String>,
	pub columns: Option<Vec<Column>>,
	pub sort: Option<Sort>,
	pub sorts: Option<Vec<String>>,
	pub search: Option<bool>,
	pub limit: Option<u64>,
	pub filter: Option<Map<String, Value>>,
	pub hide_empty: Option<bool>,
	pub row_open: Option<String>,
	pub row_actions: Option<Vec<RowAction>>,
	pub expand: Option<Vec<Column>>,
	pub action: Option<String>,
	pub params: Option<Map<String, Value>>,
	pub param: Option<String>,
	pub on: Option<Value>,
	pub off: Option<Value>,
	pub text: Option<String>,
	pub level: Option<String>,
	pub widgets: Option<Vec<WidgetDef>>,
}

/// A number, or an address (or, for a column's bar, a field or `top`).
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum Bound {
	Num(serde_json::Number),
	Name(String),
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum ForEach {
	Collection(String),
	List(Vec<String>),
}

/// A graph's labels, one per value; a bar chart's, the field under each bar.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum Labels {
	Field(String),
	List(Vec<String>),
}

#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct ListItem {
	pub label: Option<String>,
	pub value: Option<String>,
	pub text: Option<String>,
	pub format: Option<String>,
	pub unit: Option<String>,
	pub map: Option<BTreeMap<String, String>>,
	pub empty: Option<String>,
	pub when: Option<String>,
	pub for_each: Option<ForEach>,
}

#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Column {
	pub field: String,
	pub label: Option<String>,
	pub format: Option<String>,
	pub unit: Option<String>,
	pub bar: Option<Bar>,
	pub grow: Option<bool>,
	pub map: Option<BTreeMap<String, String>>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Bar {
	pub max: Bound,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Sort {
	pub field: String,
	#[serde(default)]
	pub desc: bool,
}

#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct RowAction {
	pub action: String,
	pub label: Option<String>,
	pub params: Option<Map<String, Value>>,
}

impl Default for Config {
	/// The built-in settings alone, as with no files at all.
	fn default() -> Self {
		static BUILT_IN: std::sync::OnceLock<Config> = std::sync::OnceLock::new();
		BUILT_IN
			.get_or_init(|| read_sources(&[]).unwrap_or_else(|m| panic!("the built-in defaults have mistakes: {}", join_mistakes(&m))))
			.clone()
	}
}

impl Config {
	/// What a key the files leave out is, before anything is read.
	fn bare() -> Config {
		Config {
			listen: vec![Target::ANY],
			port: crate::consts::protocol::DEFAULT_PORT,
			names: Vec::new(),
			name: String::new(),
			mdns: true,
			interval_ms: 1000,
			history_minutes: 10,
			control: false,
			default_role: Role::Admin,
			default_profile: "default".into(),
			links: BTreeMap::new(),
			actions: BTreeMap::new(),
			profiles: BTreeMap::new(),
			dashboards: BTreeMap::new(),
			templates: BTreeMap::new(),
		}
	}

	/// The profile `id` with its `use` chain followed; `default_profile` for one that isn't there.
	pub fn profile(&self, id: &str) -> Profile {
		let id = if self.profiles.contains_key(id) { id } else { self.default_profile.as_str() };
		let mut chain: Vec<&ProfileDef> = Vec::new();
		let mut at = Some(id);
		let mut seen = HashSet::new();
		while let Some(p) = at.filter(|p| seen.insert(*p)).and_then(|p| self.profiles.get(p)) {
			chain.push(p);
			at = p.base.as_deref();
		}
		// From the end of the chain to the profile itself, each overriding the one it uses.
		let mut out = Profile {
			id: id.to_string(),
			name: id.to_string(),
			theme: "system".into(),
			pure_black: false,
			animations: true,
			interval_ms: 1000,
			fahrenheit: false,
			network_bits: false,
			confirm_actions: true,
			unlock_for: ["power", "signal", "actions", "units"].map(String::from).to_vec(),
			unlock_grace: "every_time".into(),
			dashboards: ["overview", "processes", "sensors", "control"].map(String::from).to_vec(),
			header: "header".into(),
			android: ANDROID.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
			web: Map::new(),
		};
		for p in chain.iter().rev() {
			if let Some(v) = &p.theme {
				out.theme = v.clone();
			}
			out.pure_black = p.pure_black.unwrap_or(out.pure_black);
			out.animations = p.animations.unwrap_or(out.animations);
			out.interval_ms = p.interval_ms.unwrap_or(out.interval_ms);
			out.fahrenheit = p.fahrenheit.unwrap_or(out.fahrenheit);
			out.network_bits = p.network_bits.unwrap_or(out.network_bits);
			out.confirm_actions = p.confirm_actions.unwrap_or(out.confirm_actions);
			if let Some(u) = &p.unlock {
				if let Some(k) = &u.kinds {
					out.unlock_for = k.clone();
				}
				if let Some(g) = &u.grace {
					out.unlock_grace = g.clone();
				}
			}
			if let Some(d) = &p.dashboards {
				out.dashboards = d.clone();
			}
			if let Some(h) = &p.header {
				out.header = h.clone();
			}
			for (k, v) in p.android.iter().flatten() {
				out.android.insert(k.clone(), *v);
			}
			for (k, v) in p.web.iter().flatten() {
				out.web.insert(k.clone(), v.clone());
			}
		}
		// A profile's name is its own, not one it uses: without one, its id.
		if let Some(name) = chain.first().and_then(|p| p.name.clone()) {
			out.name = name;
		}
		out
	}

	/// Whether `id` is an action: a built-in one or one the settings add.
	pub fn has_action(&self, id: &str) -> bool {
		self.actions.contains_key(id) || actions::BUILT_IN.iter().any(|b| b.0 == id)
	}

	/// The kinds of an action's parameters by id: `choice`, `range`, `bool`, `integer`, `number`
	/// or `string`.
	pub fn param_kinds(&self, action: &str) -> Option<Vec<(String, &'static str)>> {
		if let Some(a) = self.actions.get(action) {
			return Some(actions::settings_spec(action, a).params.iter().map(|p| (p.id.clone(), p.kind.name())).collect());
		}
		Some(actions::built_in_param_kinds(action)?.into_iter().map(|(p, k)| (p.to_string(), k)).collect())
	}

	/// The dashboard `id` names: one of the settings', or one `for_each` makes (`gpu.nvidia`).
	pub fn dashboard_of(&self, id: &str) -> Option<(&str, &Dashboard)> {
		if let Some((k, d)) = self.dashboards.get_key_value(id) {
			return Some((k.as_str(), d));
		}
		let (base, _) = id.split_once('.')?;
		self.dashboards.get_key_value(base).filter(|(_, d)| d.for_each.is_some()).map(|(k, d)| (k.as_str(), d))
	}
}

// ---- mistakes -------------------------------------------------------------------------------

/// A mistake in the settings: where, and what.
#[derive(Debug, Clone, PartialEq)]
pub struct Mistake {
	pub file: Option<String>,
	pub line: Option<usize>,
	/// Where in the settings: `dashboards.power.widgets[2]`.
	pub path: String,
	pub message: String,
}

impl fmt::Display for Mistake {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match (&self.file, self.line) {
			(Some(file), Some(line)) => write!(f, "{file}:{line}: ")?,
			(Some(file), None) => write!(f, "{file}: ")?,
			_ => {}
		}
		if !self.path.is_empty() {
			write!(f, "{}: ", self.path)?;
		}
		write!(f, "{}", self.message)
	}
}

/// The mistakes, one to a line.
pub fn join_mistakes(m: &[Mistake]) -> String {
	m.iter().map(Mistake::to_string).collect::<Vec<_>>().join("\n")
}

/// A settings file as read: what to call it, and its text.
#[derive(Clone, Debug)]
pub struct Source {
	pub name: String,
	pub text: String,
}

impl Source {
	pub fn new(name: impl Into<String>, text: impl Into<String>) -> Source {
		Source { name: name.into(), text: text.into() }
	}
}

/// The files `files` names that are there, read; one that is there and can't be read is a mistake.
pub fn read_files(files: &[PathBuf]) -> Result<Vec<Source>, Vec<Mistake>> {
	let mut out = Vec::new();
	for path in files {
		match std::fs::read_to_string(path) {
			Ok(text) => out.push(Source::new(path.display().to_string(), text)),
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
			Err(e) => return Err(vec![Mistake { file: Some(path.display().to_string()), line: None, path: String::new(), message: e.to_string() }]),
		}
	}
	Ok(out)
}

/// The settings from the built-in defaults and `files`, in that order; those not there are left out.
pub fn load(files: &[PathBuf]) -> Result<Config, Vec<Mistake>> {
	read_sources(&read_files(files)?)
}

/// The settings from the built-in defaults and `sources`, in that order, or every mistake found.
pub fn read_sources(sources: &[Source]) -> Result<Config, Vec<Mistake>> {
	let all: Vec<Source> = std::iter::once(Source::new(DEFAULTS_NAME, DEFAULTS)).chain(sources.iter().cloned()).collect();
	let mut mistakes = Vec::new();
	let mut docs = Vec::new();
	for (i, s) in all.iter().enumerate() {
		let table: toml::Table = match s.text.parse() {
			Ok(t) => t,
			Err(e) => {
				let e: toml::de::Error = e;
				let line = e.span().map(|r| line_at(&s.text, r.start));
				mistakes.push(Mistake { file: Some(s.name.clone()), line, path: String::new(), message: e.message().trim().to_string() });
				continue;
			}
		};
		let value = serde_json::to_value(&table).unwrap_or_default();
		let lines = lines_of(&s.text);
		let mut bad = Vec::new();
		check_top(&value, true, &mut bad);
		for (path, message) in bad {
			mistakes.push(Mistake { file: Some(s.name.clone()), line: find_line(&lines, &path), path, message });
		}
		docs.push((i, value, lines));
	}
	if !mistakes.is_empty() {
		return Err(mistakes);
	}
	// Where each part of the merged settings came from: the last file that set it.
	let mut merged = Map::new();
	let mut whence: HashMap<String, (usize, usize)> = HashMap::new();
	for (i, value, lines) in docs {
		if let Value::Object(o) = value {
			merge(&mut merged, o);
		}
		for (path, line) in lines {
			whence.insert(path, (i, line));
		}
	}
	let at = |path: &str, message: String| {
		let mut p = path;
		loop {
			if let Some((i, line)) = whence.get(p) {
				return Mistake { file: Some(all[*i].name.clone()), line: Some(*line), path: path.to_string(), message };
			}
			match parent(p) {
				Some(up) => p = up,
				None => return Mistake { file: None, line: None, path: path.to_string(), message },
			}
		}
	};
	let merged = Value::Object(merged);
	let mut bad = Vec::new();
	check_top(&merged, false, &mut bad);
	if !bad.is_empty() {
		return Err(bad.into_iter().map(|(p, m)| at(&p, m)).collect());
	}
	let config: Config = match serde_json::from_value(merged) {
		Ok(c) => c,
		Err(e) => return Err(vec![Mistake { file: None, line: None, path: String::new(), message: e.to_string() }]),
	};
	let bad = semantics(&config);
	if !bad.is_empty() {
		return Err(bad.into_iter().map(|(p, m)| at(&p, m)).collect());
	}
	Ok(config)
}

/// Tables merge key by key; anything else, lists included, replaces what was there.
pub fn merge(into: &mut Map<String, Value>, from: Map<String, Value>) {
	for (key, value) in from {
		match (into.get_mut(&key), value) {
			(Some(Value::Object(a)), Value::Object(b)) => merge(a, b),
			(_, v) => {
				into.insert(key, v);
			}
		}
	}
}

/// The line, from 1, that byte `at` of `text` is on.
fn line_at(text: &str, at: usize) -> usize {
	text.as_bytes()[..at.min(text.len())].iter().filter(|b| **b == b'\n').count() + 1
}

/// The line each part of a file is on, by its path.
fn lines_of(text: &str) -> HashMap<String, usize> {
	use toml::de::{DeTable, DeValue};
	fn value(path: String, v: &toml::Spanned<DeValue>, text: &str, out: &mut HashMap<String, usize>) {
		out.entry(path.clone()).or_insert_with(|| line_at(text, v.span().start));
		match v.get_ref() {
			DeValue::Table(t) => table(&path, t, text, out),
			DeValue::Array(a) => {
				for (i, e) in a.iter().enumerate() {
					value(format!("{path}[{i}]"), e, text, out);
				}
			}
			_ => {}
		}
	}
	fn table(path: &str, t: &DeTable, text: &str, out: &mut HashMap<String, usize>) {
		for (k, v) in t.iter() {
			let p = join(path, k.get_ref());
			out.insert(p.clone(), line_at(text, k.span().start));
			value(p, v, text, out);
		}
	}
	let mut out = HashMap::new();
	if let Ok(doc) = DeTable::parse(text) {
		table("", doc.get_ref(), text, &mut out);
	}
	out
}

/// The line of `path`, or of the nearest part around it that has one.
fn find_line(lines: &HashMap<String, usize>, path: &str) -> Option<usize> {
	let mut p = path;
	loop {
		if let Some(l) = lines.get(p) {
			return Some(*l);
		}
		p = parent(p)?;
	}
}

/// `path` without its last part: `a.b[2].c` → `a.b[2]` → `a.b` → `a`.
fn parent(path: &str) -> Option<&str> {
	if path.is_empty() {
		return None;
	}
	if path.ends_with(']') {
		return path.rfind('[').map(|i| &path[..i]);
	}
	let mut depth = 0;
	for (i, c) in path.char_indices().rev() {
		match c {
			'"' => depth ^= 1,
			'.' if depth == 0 => return Some(&path[..i]),
			_ => {}
		}
	}
	Some("")
}

/// `path.key`, the key quoted when it isn't a bare one.
pub fn join(path: &str, key: &str) -> String {
	let bare = !key.is_empty() && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
	let key = if bare { key.to_string() } else { format!("\"{key}\"") };
	if path.is_empty() { key } else { format!("{path}.{key}") }
}

// ---- what the settings may hold -------------------------------------------------------------

/// What a key holds.
#[derive(Clone, Debug)]
pub enum Ty {
	Str,
	/// A string that may hold values.
	Text,
	Id,
	Address,
	Bool,
	Int(i64, i64),
	Num,
	/// A string, a number or a bool.
	Scalar,
	/// One of the words, called the first.
	Enum(&'static str, &'static [&'static str]),
	Format,
	List(Box<Ty>),
	/// A table of any keys.
	Map(Box<Ty>),
	/// A table keyed by ids.
	IdMap(Box<Ty>),
	/// A table of the keys of a record.
	Rec(&'static str),
	Any(Vec<Ty>),
	Widget,
}

#[derive(Clone, Debug)]
pub struct Key {
	pub name: &'static str,
	pub ty: Ty,
	pub required: bool,
}

fn k(name: &'static str, ty: Ty) -> Key {
	Key { name, ty, required: false }
}

fn req(name: &'static str, ty: Ty) -> Key {
	Key { name, ty, required: true }
}

fn list(t: Ty) -> Ty {
	Ty::List(Box::new(t))
}

fn map(t: Ty) -> Ty {
	Ty::Map(Box::new(t))
}

fn id_map(t: Ty) -> Ty {
	Ty::IdMap(Box::new(t))
}

fn num_or_address() -> Ty {
	Ty::Any(vec![Ty::Num, Ty::Address])
}

fn icon() -> Ty {
	Ty::Enum("icon", &ICONS)
}

fn color() -> Ty {
	Ty::Enum("colour", &COLORS)
}

fn role() -> Ty {
	Ty::Enum("role", &ROLES)
}

/// The records the settings are made of, by name.
pub const RECORDS: [&str; 15] =
	["settings", "link", "action", "param", "profile", "unlock", "android", "web", "dashboard", "item", "column", "expand", "bar", "sort", "row_action"];

/// The keys of a record.
pub fn record(name: &str) -> Vec<Key> {
	match name {
		"settings" => vec![
			k("listen", Ty::Any(vec![Ty::Str, list(Ty::Str)])),
			k("port", Ty::Int(1, 65535)),
			k("names", list(Ty::Str)),
			k("name", Ty::Str),
			k("mdns", Ty::Bool),
			k("interval_ms", Ty::Int(250, 10_000)),
			k("history_minutes", Ty::Int(1, 1440)),
			k("control", Ty::Bool),
			k("default_role", role()),
			k("default_profile", Ty::Id),
			k("links", id_map(Ty::Rec("link"))),
			k("actions", id_map(Ty::Rec("action"))),
			k("profiles", id_map(Ty::Rec("profile"))),
			k("dashboards", id_map(Ty::Rec("dashboard"))),
			k("templates", id_map(Ty::Widget)),
		],
		"link" => vec![req("name", Ty::Str), req("url", Ty::Str), k("unit", Ty::Str), k("scope", Ty::Enum("scope", &SCOPES)), k("icon", icon())],
		"action" => vec![
			req("name", Ty::Str),
			req("command", list(Ty::Str)),
			k("params", id_map(Ty::Rec("param"))),
			k("role", Ty::Enum("role", &ACTION_ROLES)),
			k("confirm", Ty::Bool),
			k("unlock", Ty::Bool),
			k("timeout", Ty::Int(1, 3600)),
			k("icon", icon()),
		],
		"param" => vec![
			k("choices", list(Ty::Str)),
			k("labels", map(Ty::Str)),
			k("range", list(Ty::Num)),
			k("step", Ty::Num),
			k("unit", Ty::Str),
			k("type", Ty::Enum("parameter type", &["bool"])),
			k("default", Ty::Scalar),
			k("label", Ty::Str),
		],
		"profile" => vec![
			k("use", Ty::Id),
			k("name", Ty::Str),
			k("theme", Ty::Enum("theme", &THEMES)),
			k("pure_black", Ty::Bool),
			k("animations", Ty::Bool),
			k("interval_ms", Ty::Int(250, 60_000)),
			k("fahrenheit", Ty::Bool),
			k("network_bits", Ty::Bool),
			k("confirm_actions", Ty::Bool),
			k("unlock", Ty::Rec("unlock")),
			k("dashboards", list(Ty::Str)),
			k("header", Ty::Str),
			k("android", Ty::Rec("android")),
			k("web", Ty::Rec("web")),
		],
		"unlock" => vec![k("for", list(Ty::Enum("action kind", &actions::KINDS))), k("grace", Ty::Enum("grace", &GRACES))],
		"android" => ANDROID.iter().map(|(n, _)| k(n, Ty::Bool)).collect(),
		"web" => vec![],
		"dashboard" => {
			vec![req("title", Ty::Text), k("icon", icon()), k("role", role()), k("for_each", Ty::Str), k("widgets", list(Ty::Widget))]
		}
		"item" => vec![
			k("label", Ty::Text),
			k("value", Ty::Address),
			k("text", Ty::Text),
			k("format", Ty::Format),
			k("unit", Ty::Str),
			k("map", map(Ty::Str)),
			k("empty", Ty::Text),
			k("when", Ty::Address),
			k("for_each", Ty::Any(vec![Ty::Str, list(Ty::Str)])),
		],
		"column" => vec![
			req("field", Ty::Str),
			k("label", Ty::Str),
			k("format", Ty::Format),
			k("unit", Ty::Str),
			k("bar", Ty::Rec("bar")),
			k("grow", Ty::Bool),
			k("map", map(Ty::Str)),
		],
		"expand" => vec![req("field", Ty::Str), k("label", Ty::Str), k("format", Ty::Format), k("unit", Ty::Str), k("map", map(Ty::Str))],
		"bar" => vec![req("max", Ty::Any(vec![Ty::Num, Ty::Str]))],
		"sort" => vec![req("field", Ty::Str), k("desc", Ty::Bool)],
		"row_action" => vec![req("action", Ty::Str), k("label", Ty::Str), k("params", map(Ty::Scalar))],
		_ => vec![],
	}
}

/// The keys every widget takes.
pub fn common_keys() -> Vec<Key> {
	vec![
		k("type", Ty::Enum("widget type", &WIDGET_TYPES)),
		k("id", Ty::Id),
		k("span", Ty::Int(1, 4)),
		k("label", Ty::Text),
		k("icon", icon()),
		k("color", color()),
		k("role", role()),
		k("when", Ty::Address),
		k("open", Ty::Str),
		k("caption", Ty::Text),
		k("use", Ty::Id),
		k("with", map(Ty::Scalar)),
		k("for_each", Ty::Any(vec![Ty::Str, list(Ty::Str)])),
	]
}

/// The keys of widgets that show a value.
fn value_keys() -> Vec<Key> {
	vec![
		k("value", Ty::Address),
		k("unit", Ty::Str),
		k("format", Ty::Format),
		k("map", map(Ty::Str)),
		k("empty", Ty::Text),
		k("warning_above", num_or_address()),
		k("critical_above", num_or_address()),
		k("warning_below", num_or_address()),
		k("critical_below", num_or_address()),
		k("soft", Ty::Num),
		k("levels", map(Ty::Enum("level", &LEVELS))),
	]
}

/// A widget type's own keys.
pub fn type_keys(kind: &str) -> Vec<Key> {
	let action = || vec![k("value", Ty::Address), k("action", Ty::Str), k("param", Ty::Str)];
	let scale = || vec![k("min", num_or_address()), k("max", num_or_address())];
	match kind {
		"value" => value_keys().into_iter().chain([k("spark", Ty::Bool)]).collect(),
		"gauge" | "meter" => value_keys().into_iter().chain(scale()).collect(),
		"status" => value_keys(),
		"graph" => vec![
			k("values", list(Ty::Address)),
			k("labels", list(Ty::Text)),
			k("colors", list(color())),
			k("range", Ty::Int(1, 86_400)),
			k("height", Ty::Enum("height", &HEIGHTS)),
			k("min", num_or_address()),
			k("max", num_or_address()),
			k("floor", Ty::Num),
			k("empty", Ty::Text),
			k("unit", Ty::Str),
			k("format", Ty::Format),
		],
		"list" => vec![k("items", list(Ty::Rec("item")))],
		"bars" => vec![
			k("source", Ty::Str),
			k("field", Ty::Str),
			k("max", num_or_address()),
			k("labels", Ty::Str),
			k("unit", Ty::Str),
			k("format", Ty::Format),
		],
		"table" => vec![
			k("source", Ty::Str),
			k("columns", list(Ty::Rec("column"))),
			k("sort", Ty::Rec("sort")),
			k("sorts", list(Ty::Str)),
			k("search", Ty::Bool),
			k("limit", Ty::Int(1, 10_000)),
			k("filter", map(Ty::Scalar)),
			k("hide_empty", Ty::Bool),
			k("row_open", Ty::Str),
			k("row_actions", list(Ty::Rec("row_action"))),
			k("expand", list(Ty::Rec("expand"))),
			k("collapsed", Ty::Bool),
		],
		"button" => vec![k("action", Ty::Str), k("params", map(Ty::Scalar))],
		"toggle" => action().into_iter().chain([k("on", Ty::Scalar), k("off", Ty::Scalar)]).collect(),
		"select" | "slider" => action(),
		"text" => vec![k("text", Ty::Text), k("level", Ty::Enum("level", &LEVELS))],
		"group" => vec![k("widgets", list(Ty::Widget)), k("collapsed", Ty::Bool)],
		_ => vec![],
	}
}

/// The keys a widget of `kind` takes, or, without a type, any type's.
pub fn widget_keys(kind: Option<&str>) -> Vec<Key> {
	let mut keys = common_keys();
	let kinds: Vec<&str> = match kind {
		Some(k) if WIDGET_TYPES.contains(&k) => vec![k],
		_ => WIDGET_TYPES.to_vec(),
	};
	for t in kinds {
		for key in type_keys(t) {
			match keys.iter_mut().find(|k| k.name == key.name) {
				Some(have) if format!("{:?}", have.ty) != format!("{:?}", key.ty) => {
					have.ty = Ty::Any(vec![have.ty.clone(), key.ty]);
				}
				Some(_) => {}
				None => keys.push(key),
			}
		}
	}
	keys
}

/// What a value of `ty` is, to say what was expected.
fn describe(ty: &Ty) -> String {
	match ty {
		Ty::Str | Ty::Text => "a string".into(),
		Ty::Id => "an id (1 to 32 of a-z, 0-9, _ and -, starting with a letter or a digit)".into(),
		Ty::Address => "an address, like \"cpu.usage\"".into(),
		Ty::Bool => "true or false".into(),
		Ty::Int(lo, hi) => format!("an integer from {lo} to {hi}"),
		Ty::Num => "a number".into(),
		Ty::Scalar => "a string, a number or true or false".into(),
		Ty::Enum(what, words) => format!("a {what}: {}", words.join(", ")),
		Ty::Format => format!("a format: {}, or a pattern like \"{{:.1}}\"", format::FORMATS.join(", ")),
		Ty::List(t) => format!("a list of {}", plural_of(t)),
		Ty::Map(_) | Ty::IdMap(_) | Ty::Rec(_) => "a table".into(),
		Ty::Any(ts) => ts.iter().map(describe).collect::<Vec<_>>().join(" or "),
		Ty::Widget => "a widget (a table with a type)".into(),
	}
}

fn plural_of(ty: &Ty) -> String {
	match ty {
		Ty::Str | Ty::Text => "strings".into(),
		Ty::Address => "addresses".into(),
		Ty::Num => "numbers".into(),
		Ty::Widget => "widgets".into(),
		Ty::Enum(what, _) => format!("{what}s"),
		Ty::Rec(_) => "tables".into(),
		other => describe(other),
	}
}

/// What was found, to say so.
fn found(v: &Value) -> String {
	match v {
		Value::String(s) if s.chars().count() > 40 => format!("\"{}…\"", s.chars().take(40).collect::<String>()),
		Value::String(s) => format!("\"{s}\""),
		Value::Array(_) => "a list".into(),
		Value::Object(_) => "a table".into(),
		other => other.to_string(),
	}
}

/// An id: 1 to 32 of `a-z`, `0-9`, `_` and `-`, starting with a letter or a digit.
pub fn is_id(s: &str) -> bool {
	(1..=32).contains(&s.len())
		&& s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
		&& s.bytes().next().is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// The id pattern, for the JSON Schema.
pub const ID_PATTERN: &str = "^[a-z0-9][a-z0-9_-]{0,31}$";

/// The one of `choices` closest to `word`, when it is close enough to be what was meant.
pub fn suggest<'a>(word: &str, choices: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
	let limit = (word.chars().count() / 3).clamp(1, 3);
	choices.into_iter().map(|c| (distance(word, c), c)).filter(|(d, _)| *d <= limit).min_by_key(|(d, _)| *d).map(|(_, c)| c)
}

/// The edits from one word to another, a swap of neighbours counting as one.
fn distance(a: &str, b: &str) -> usize {
	let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
	let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
	for (i, row) in d.iter_mut().enumerate() {
		row[0] = i;
	}
	for (j, cell) in d[0].iter_mut().enumerate() {
		*cell = j;
	}
	for i in 1..=a.len() {
		for j in 1..=b.len() {
			let cost = usize::from(a[i - 1] != b[j - 1]);
			d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
			if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
				d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
			}
		}
	}
	d[a.len()][b.len()]
}

fn did_you_mean<'a>(word: &str, choices: impl IntoIterator<Item = &'a str>) -> String {
	suggest(word, choices).map(|s| format!("; did you mean {s}?")).unwrap_or_default()
}

// ---- checking each file, and the merged settings ---------------------------------------------

type Found = Vec<(String, String)>;

/// The settings' shape: every key known and of its kind. `partial` for a file on its own, which
/// may leave out what another file gives.
fn check_top(v: &Value, partial: bool, out: &mut Found) {
	let Value::Object(top) = v else { return };
	let mut top = top.clone();
	if top.remove("units").is_some() {
		out.push(("units".into(), "settings schema 1's units are gone: name a link's unit on the link, in [links.<id>] (unit = \"…\")".into()));
	}
	for (key, form) in [("links", "[links.<id>]"), ("actions", "[actions.<id>]")] {
		if top.get(key).is_some_and(Value::is_array) {
			top.remove(key);
			out.push((
				key.into(),
				format!("settings schema 1's [[{key}]] is gone: give each a table of its own, {form}, its id the key (see `elamond defaults` and docs/config.md)"),
			));
		}
	}
	// Where to listen and the names offered, each as the service takes it.
	match top.get("listen") {
		Some(Value::String(s)) => {
			if let Err(e) = Target::parse(s) {
				out.push(("listen".into(), e));
			}
		}
		Some(Value::Array(a)) if a.is_empty() => out.push(("listen".into(), "is empty; leave it out to listen on every address".into())),
		Some(Value::Array(a)) => {
			for (i, s) in a.iter().enumerate().filter_map(|(i, v)| Some((i, v.as_str()?))) {
				if let Err(e) = Target::parse(s) {
					out.push((format!("listen[{i}]"), e));
				}
			}
		}
		_ => {}
	}
	if let Some(Value::Array(a)) = top.get("names") {
		for (i, s) in a.iter().enumerate().filter_map(|(i, v)| Some((i, v.as_str()?))) {
			if let Err(e) = crate::names::check(s) {
				out.push((format!("names[{i}]"), e));
			}
		}
	}
	check(&Value::Object(top), &Ty::Rec("settings"), "", partial, out);
}

fn check(v: &Value, ty: &Ty, path: &str, partial: bool, out: &mut Found) {
	let wrong = |out: &mut Found| out.push((path.to_string(), format!("should be {}, not {}", describe(ty), found(v))));
	match ty {
		Ty::Str | Ty::Text => {
			if !v.is_string() {
				wrong(out);
			}
		}
		Ty::Id => match v.as_str() {
			Some(s) if is_id(s) => {}
			Some(s) => out.push((path.into(), format!("\"{s}\" isn't an id: 1 to 32 of a-z, 0-9, _ and -, starting with a letter or a digit"))),
			None => wrong(out),
		},
		Ty::Address => match v.as_str() {
			Some(s) => check_address(s, path, out),
			None => wrong(out),
		},
		Ty::Bool => {
			if !v.is_boolean() {
				wrong(out);
			}
		}
		Ty::Int(lo, hi) => match v.as_i64() {
			Some(n) if (*lo..=*hi).contains(&n) => {}
			_ => wrong(out),
		},
		Ty::Num => {
			if !v.is_number() {
				wrong(out);
			}
		}
		Ty::Scalar => {
			if !(v.is_string() || v.is_number() || v.is_boolean()) {
				wrong(out);
			}
		}
		Ty::Enum(what, words) => match v.as_str() {
			Some(s) if words.contains(&s) || s.contains('{') => {}
			Some(s) => out.push((path.into(), format!("no {what} \"{s}\"{}", did_you_mean(s, words.iter().copied())))),
			None => wrong(out),
		},
		Ty::Format => match v.as_str() {
			Some(s) if format::is_format(s) => {}
			Some(s) => out.push((path.into(), format!("no format \"{s}\"{}", did_you_mean(s, format::FORMATS)))),
			None => wrong(out),
		},
		Ty::List(t) => match v.as_array() {
			Some(a) => {
				for (i, e) in a.iter().enumerate() {
					check(e, t, &format!("{path}[{i}]"), partial, out);
				}
			}
			None => wrong(out),
		},
		Ty::Map(t) => match v.as_object() {
			Some(o) => {
				for (key, e) in o {
					check(e, t, &join(path, key), partial, out);
				}
			}
			None => wrong(out),
		},
		Ty::IdMap(t) => match v.as_object() {
			Some(o) => {
				for (key, e) in o {
					if !is_id(key) {
						out.push((join(path, key), format!("\"{key}\" isn't an id: 1 to 32 of a-z, 0-9, _ and -, starting with a letter or a digit")));
					}
					check(e, t, &join(path, key), partial, out);
				}
			}
			None => wrong(out),
		},
		Ty::Rec(name) => match v.as_object() {
			Some(o) => check_keys(o, &record(name), path, partial, "here", out),
			None => wrong(out),
		},
		Ty::Any(ts) => {
			let fits = ts.iter().any(|t| {
				let mut none = Vec::new();
				check(v, t, path, partial, &mut none);
				none.is_empty()
			});
			if !fits {
				// The one alternative of the same kind says best what is wrong.
				let same = ts.iter().find(|t| matches!((t, v), (Ty::Str | Ty::Text | Ty::Address | Ty::Enum(..), Value::String(_)) | (Ty::List(_), Value::Array(_))));
				match same {
					Some(t) => check(v, t, path, partial, out),
					None => wrong(out),
				}
			}
		}
		Ty::Widget => match v.as_object() {
			Some(o) => check_widget(o, path, partial, out),
			None => wrong(out),
		},
	}
}

fn check_keys(o: &Map<String, Value>, keys: &[Key], path: &str, partial: bool, what: &str, out: &mut Found) {
	for (name, v) in o {
		match keys.iter().find(|k| k.name == name) {
			Some(key) => check(v, &key.ty, &join(path, name), partial, out),
			None => out.push((join(path, name), format!("no key \"{name}\" {what}{}", did_you_mean(name, keys.iter().map(|k| k.name))))),
		}
	}
	if !partial {
		for key in keys.iter().filter(|k| k.required && !o.contains_key(k.name)) {
			out.push((path.into(), format!("{} is missing", key.name)));
		}
	}
}

/// A widget's keys, against its type's when it has one.
fn check_widget(o: &Map<String, Value>, path: &str, partial: bool, out: &mut Found) {
	let kind = o.get("type").and_then(Value::as_str);
	if let Some(t) = kind
		&& !WIDGET_TYPES.contains(&t)
	{
		let line = format!("no widget type \"{t}\"{}", did_you_mean(t, WIDGET_TYPES));
		out.push((path.into(), line));
		let mut rest = o.clone();
		rest.remove("type");
		check_keys(&rest, &widget_keys(None), path, partial, "in a widget", out);
		return;
	}
	let what = match kind {
		Some(t) => format!("in a {t} widget"),
		None => "in a widget".into(),
	};
	check_keys(o, &widget_keys(kind), path, partial, &what, out);
}

/// An address of the service's own must be one there is.
fn check_address(a: &str, path: &str, out: &mut Found) {
	if values::known_prefix(a) && values::address_def(a).is_none() {
		let hint = values::suggest_address(a).map(|s| format!("; did you mean {s}?")).unwrap_or_default();
		out.push((path.into(), format!("no value \"{a}\"{hint}")));
	}
}

// ---- what refers to what --------------------------------------------------------------------

/// What `{item}` stands for where a string is: nothing, an item of a collection, or a string of a
/// list.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Scope<'a> {
	None,
	Collection(&'a str),
	List,
}

fn semantics(c: &Config) -> Found {
	let mut out = Vec::new();
	let mut push = |p: String, m: String| out.push((p, m));
	if !c.profiles.contains_key(&c.default_profile) {
		push("default_profile".into(), format!("no profile \"{}\"{}", c.default_profile, did_you_mean(&c.default_profile, c.profiles.keys().map(String::as_str))));
	}
	for (id, l) in &c.links {
		let path = join("links", id);
		if !crate::links::web_url(&l.url) {
			push(join(&path, "url"), format!("\"{}\" isn't a web page's address: http://, https:// or a bare host:port", l.url));
		}
		if let Some(u) = &l.unit
			&& let Err(why) = crate::links::unit_shape(u)
		{
			push(join(&path, "unit"), format!("{u} {why}"));
		}
	}
	for (id, a) in &c.actions {
		check_action(id, a, &mut push);
	}
	for (id, p) in &c.profiles {
		let path = join("profiles", id);
		if let Some(base) = &p.base {
			if !c.profiles.contains_key(base) {
				push(join(&path, "use"), format!("no profile \"{base}\"{}", did_you_mean(base, c.profiles.keys().map(String::as_str))));
			} else {
				let mut seen = vec![id.as_str()];
				let mut at = Some(base.as_str());
				while let Some(next) = at {
					if seen.contains(&next) {
						push(join(&path, "use"), format!("goes round in a loop: {} → {next}", seen.join(" → ")));
						break;
					}
					seen.push(next);
					at = c.profiles.get(next).and_then(|p| p.base.as_deref());
				}
			}
		}
		for (i, d) in p.dashboards.iter().flatten().enumerate() {
			if c.dashboard_of(d).is_none() {
				push(format!("{}[{i}]", join(&path, "dashboards")), format!("no dashboard \"{d}\"{}", did_you_mean(d, c.dashboards.keys().map(String::as_str))));
			}
		}
		if let Some(h) = p.header.as_deref().filter(|h| !h.is_empty())
			&& !c.dashboards.contains_key(h)
		{
			push(join(&path, "header"), format!("no dashboard \"{h}\"{}", did_you_mean(h, c.dashboards.keys().map(String::as_str))));
		}
	}
	for id in c.templates.keys() {
		if let Err(e) = template_chain(id, &c.templates) {
			push(join("templates", id), e);
		}
	}
	for (id, d) in &c.dashboards {
		let path = join("dashboards", id);
		let scope = match &d.for_each {
			Some(name) if values::collection_def(name).is_none() => {
				push(join(&path, "for_each"), no_collection(name));
				Scope::None
			}
			Some(name) => Scope::Collection(name.as_str()),
			None => Scope::None,
		};
		check_text(&d.title, &join(&path, "title"), scope, &mut push);
		check_widgets(c, &d.widgets, &join(&path, "widgets"), scope, &mut push);
	}
	out
}

fn no_collection(name: &str) -> String {
	format!("no collection \"{name}\"{}", did_you_mean(name, values::COLLECTIONS.iter().map(|c| c.name)))
}

fn check_action(id: &str, a: &Action, push: &mut impl FnMut(String, String)) {
	let path = join("actions", id);
	if a.command.first().is_none_or(|p| p.trim().is_empty()) {
		push(join(&path, "command"), "is empty: give the program and its arguments".into());
	}
	for (i, arg) in a.command.iter().enumerate() {
		if let Some(name) = actions::whole_param(arg)
			&& !a.params.contains_key(name)
		{
			push(format!("{}[{i}]", join(&path, "command")), format!("{{{name}}} names no parameter{}", did_you_mean(name, a.params.keys().map(String::as_str))));
		}
	}
	for (pid, p) in &a.params {
		let ppath = join(&join(&path, "params"), pid);
		let forms = [p.choices.is_some(), p.range.is_some(), p.kind.is_some()].iter().filter(|x| **x).count();
		if forms != 1 {
			push(ppath.clone(), "give exactly one of choices = […], range = [min, max] or type = \"bool\"".into());
			continue;
		}
		if let Some(choices) = &p.choices {
			if choices.is_empty() {
				push(join(&ppath, "choices"), "is empty".into());
			}
			let unique: BTreeSet<&String> = choices.iter().collect();
			if unique.len() != choices.len() {
				push(join(&ppath, "choices"), "has a choice twice".into());
			}
			for key in p.labels.iter().flat_map(|l| l.keys()) {
				if !choices.contains(key) {
					push(join(&join(&ppath, "labels"), key), format!("\"{key}\" isn't one of the choices"));
				}
			}
		}
		if let Some(range) = &p.range {
			let nums: Vec<f64> = range.iter().filter_map(serde_json::Number::as_f64).collect();
			if nums.len() != 2 || nums[0] >= nums[1] {
				push(join(&ppath, "range"), "should be [min, max], min below max".into());
			}
		}
		if p.step.as_ref().and_then(serde_json::Number::as_f64).is_some_and(|s| s <= 0.0) {
			push(join(&ppath, "step"), "should be above 0".into());
		}
		if let Some(d) = &p.default {
			let spec = actions::settings_spec(id, a);
			if let Some(param) = spec.params.iter().find(|x| &x.id == pid)
				&& !actions::param_takes(param, d)
			{
				push(join(&ppath, "default"), format!("{} isn't a value the parameter takes", found(d)));
			}
		}
		if p.choices.is_none() && p.labels.is_some() {
			push(join(&ppath, "labels"), "are for choices".into());
		}
		if p.range.is_none() && (p.step.is_some() || p.unit.is_some()) {
			push(ppath.clone(), "step and unit are for a range".into());
		}
	}
}

/// A template's `use` chain, followed to its end: its ids, or why it can't be.
fn template_chain(id: &str, templates: &BTreeMap<String, WidgetDef>) -> Result<Vec<String>, String> {
	let mut chain = vec![id.to_string()];
	let mut at = templates.get(id).ok_or_else(|| format!("no template \"{id}\"{}", did_you_mean(id, templates.keys().map(String::as_str))))?;
	while let Some(next) = at.get("use").and_then(Value::as_str) {
		if chain.iter().any(|c| c == next) {
			return Err(format!("use goes round in a loop: {} → {next}", chain.join(" → ")));
		}
		chain.push(next.to_string());
		at = templates.get(next).ok_or_else(|| format!("no template \"{next}\"{}", did_you_mean(next, templates.keys().map(String::as_str))))?;
	}
	Ok(chain)
}

/// Every string in `v`, `with`'s `{key}`s filled in.
fn fill_with(v: &mut Value, with: &Map<String, Value>) {
	match v {
		Value::String(s) => {
			for (key, value) in with {
				let text = match value {
					Value::String(t) => t.clone(),
					other => other.to_string(),
				};
				*s = s.replace(&format!("{{{key}}}"), &text);
			}
		}
		Value::Array(a) => a.iter_mut().for_each(|e| fill_with(e, with)),
		Value::Object(o) => o.values_mut().for_each(|e| fill_with(e, with)),
		_ => {}
	}
}

/// A widget with its template applied: the template's keys, `with` filled into its strings, then
/// the widget's own, which win. A template may itself start from another.
pub fn apply_template(w: &WidgetDef, templates: &BTreeMap<String, WidgetDef>) -> Result<WidgetDef, String> {
	let Some(id) = w.get("use").and_then(Value::as_str) else {
		return Ok(w.clone());
	};
	let chain = template_chain(id, templates)?;
	let mut out = Map::new();
	for t in chain.iter().rev() {
		for (k, v) in &templates[t] {
			if k != "use" && k != "with" {
				out.insert(k.clone(), v.clone());
			}
		}
	}
	if let Some(Value::Object(with)) = w.get("with") {
		let mut filled = Value::Object(out);
		fill_with(&mut filled, with);
		out = match filled {
			Value::Object(o) => o,
			_ => Map::new(),
		};
	}
	for (k, v) in w {
		if k != "use" && k != "with" {
			out.insert(k.clone(), v.clone());
		}
	}
	Ok(out)
}

/// Placeholders in a string: `{item}` and `{item.<field>}` only where `for_each` gives an item, and
/// addresses that are the service's own must be some it has.
fn check_text(text: &str, path: &str, scope: Scope, push: &mut impl FnMut(String, String)) {
	for p in format::placeholders(text) {
		let (address, fmt) = format::split_placeholder(p.inner);
		if let Some(f) = fmt
			&& !format::is_format(f)
		{
			push(path.into(), format!("no format \"{f}\" in {{{}}}{}", p.inner, did_you_mean(f, format::FORMATS)));
		}
		if address == "item" || address.starts_with("item.") {
			check_item(address, path, scope, push);
			continue;
		}
		// `{gpu.{item}.summary}`: the item inside, then the address around it.
		for inner in format::placeholders(address) {
			if inner.inner == "item" || inner.inner.starts_with("item.") {
				check_item(inner.inner, path, scope, push);
			}
		}
		if !address.contains('.') && !address.contains('{') {
			push(path.into(), format!("{{{address}}} is no value: an address has a dot (cpu.usage); in a template, with = {{ {address} = … }} fills it in"));
			continue;
		}
		let mut bad = Vec::new();
		check_address(address, path, &mut bad);
		for (p, m) in bad {
			push(p, m);
		}
	}
}

fn check_item(placeholder: &str, path: &str, scope: Scope, push: &mut impl FnMut(String, String)) {
	match (scope, placeholder.strip_prefix("item.")) {
		(Scope::None, _) => push(path.into(), format!("{{{placeholder}}} with no for_each to give an item")),
		(Scope::List, Some(f)) => push(path.into(), format!("{{item.{f}}}: the items of a list are strings, without fields")),
		(Scope::Collection(c), Some(f)) if values::field_def(c, f).is_none() => {
			let fields = values::collection_def(c).map(|d| d.fields.iter().map(|f| f.0).collect::<Vec<_>>()).unwrap_or_default();
			push(path.into(), format!("no field \"{f}\" in {c}{}", did_you_mean(f, fields)));
		}
		_ => {}
	}
}

/// Every string of a widget that may hold values, by its path: all but a table's row actions,
/// whose `{field}`s are the row's.
fn texts(v: &Value, path: &str, out: &mut Vec<(String, String)>) {
	match v {
		Value::String(s) => out.push((path.to_string(), s.clone())),
		Value::Array(a) => {
			for (i, e) in a.iter().enumerate() {
				texts(e, &format!("{path}[{i}]"), out);
			}
		}
		Value::Object(o) => {
			for (k, e) in o {
				if !matches!(k.as_str(), "row_actions" | "widgets" | "items" | "filter" | "map" | "levels" | "format" | "with") {
					texts(e, &join(path, k), out);
				}
			}
		}
		_ => {}
	}
}

fn check_widgets(c: &Config, widgets: &[WidgetDef], path: &str, scope: Scope, push: &mut impl FnMut(String, String)) {
	let mut ids: HashMap<String, usize> = HashMap::new();
	for (i, raw) in widgets.iter().enumerate() {
		let wpath = format!("{path}[{i}]");
		let id = raw.get("id").and_then(Value::as_str).map_or_else(|| i.to_string(), str::to_string);
		if let Some(before) = ids.insert(id.clone(), i) {
			push(wpath.clone(), format!("id \"{id}\" is the id of widgets[{before}] too"));
		}
		check_widget_semantics(c, raw, &wpath, scope, push);
	}
}

fn check_widget_semantics(c: &Config, raw: &WidgetDef, path: &str, scope: Scope, push: &mut impl FnMut(String, String)) {
	let merged = match apply_template(raw, &c.templates) {
		Ok(m) => m,
		Err(e) => {
			push(join(path, "use"), e);
			return;
		}
	};
	let Some(kind) = merged.get("type").and_then(Value::as_str).map(str::to_string) else {
		push(path.into(), "no type: give type = \"…\", or use = a template that has one".into());
		return;
	};
	// Keys a template brought must suit the widget's type too.
	let mut bad = Vec::new();
	check_widget(&merged, path, false, &mut bad);
	if !bad.is_empty() {
		let from = raw.get("use").and_then(Value::as_str).map(|t| format!(" (with template {t})")).unwrap_or_default();
		for (p, m) in bad {
			push(p, format!("{m}{from}"));
		}
		return;
	}
	let w: Widget = match serde_json::from_value(Value::Object(merged.clone())) {
		Ok(w) => w,
		Err(e) => {
			push(path.into(), e.to_string());
			return;
		}
	};
	// What `{item}` is here.
	let scope = match &w.for_each {
		Some(ForEach::Collection(name)) if values::collection_def(name).is_none() => {
			push(join(path, "for_each"), no_collection(name));
			return;
		}
		Some(ForEach::Collection(name)) => Scope::Collection(values::collection_def(name).map_or("", |d| d.name)),
		Some(ForEach::List(_)) => Scope::List,
		None => scope,
	};
	let mut strings = Vec::new();
	texts(&Value::Object(merged.clone()), path, &mut strings);
	for (p, s) in &strings {
		check_text(s, p, scope, push);
	}
	let need = |key: &str, have: bool, push: &mut dyn FnMut(String, String)| {
		if !have {
			push(path.into(), format!("a {kind} widget needs {key}"));
		}
	};
	match kind.as_str() {
		"value" | "gauge" | "meter" | "status" => need("value", w.value.is_some(), push),
		"graph" => need("values", w.values.as_ref().is_some_and(|v| !v.is_empty()), push),
		"list" => need("items", w.items.is_some(), push),
		"bars" => need("source and field", w.source.is_some() && w.field.is_some(), push),
		"table" => need("source and columns", w.source.is_some() && w.columns.is_some(), push),
		"button" => need("action or open", w.action.is_some() || w.open.is_some(), push),
		"toggle" | "select" | "slider" => need("action and param", w.action.is_some() && w.param.is_some(), push),
		"text" => need("text", w.text.is_some(), push),
		"group" => need("widgets", w.widgets.is_some(), push),
		_ => {}
	}
	if let Some(open) = &w.open {
		let known = if let Some((base, _)) = open.split_once(".{") {
			c.dashboards.get(base).is_some_and(|d| d.for_each.is_some())
		} else {
			c.dashboard_of(open).is_some()
		};
		if !known {
			push(join(path, "open"), format!("no dashboard \"{open}\"{}", did_you_mean(open, c.dashboards.keys().map(String::as_str))));
		}
	}
	if let Some(action) = w.action.as_deref().filter(|a| !a.contains('{')) {
		check_action_use(c, action, &w, &kind, path, push);
	}
	if let Some(Labels::List(l)) = &w.labels
		&& l.len() > w.values.as_ref().map_or(0, Vec::len)
	{
		push(join(path, "labels"), "has more labels than the graph has values".into());
	}
	if w.colors.as_ref().map_or(0, Vec::len) > w.values.as_ref().map_or(0, Vec::len) {
		push(join(path, "colors"), "has more colours than the graph has values".into());
	}
	if let Some(source) = &w.source {
		check_source(c, source, &w, &kind, path, push);
	}
	for (i, item) in w.items.iter().flatten().enumerate() {
		let ipath = format!("{}[{i}]", join(path, "items"));
		if item.value.is_some() == item.text.is_some() {
			push(ipath.clone(), "give either value or text".into());
		}
		let iscope = match &item.for_each {
			Some(ForEach::Collection(name)) if values::collection_def(name).is_none() => {
				push(join(&ipath, "for_each"), no_collection(name));
				continue;
			}
			Some(ForEach::Collection(name)) => Scope::Collection(values::collection_def(name).map_or("", |d| d.name)),
			Some(ForEach::List(_)) => Scope::List,
			None => scope,
		};
		for (key, s) in [("label", &item.label), ("value", &item.value), ("text", &item.text), ("empty", &item.empty), ("when", &item.when)] {
			if let Some(s) = s {
				check_text(s, &join(&ipath, key), iscope, push);
			}
		}
	}
	if let Some(widgets) = &w.widgets {
		check_widgets(c, widgets, &join(path, "widgets"), scope, push);
	}
}

/// A widget's action, and what it sets of it.
fn check_action_use(c: &Config, action: &str, w: &Widget, kind: &str, path: &str, push: &mut impl FnMut(String, String)) {
	if !c.has_action(action) {
		let names: Vec<&str> = c.actions.keys().map(String::as_str).chain(actions::BUILT_IN.iter().map(|b| b.0)).collect();
		push(join(path, "action"), format!("no action \"{action}\"{}", did_you_mean(action, names)));
		return;
	}
	let kinds = c.param_kinds(action).unwrap_or_default();
	let spec = c.actions.get(action).map(|a| actions::settings_spec(action, a));
	if let Some(param) = &w.param {
		match kinds.iter().find(|(p, _)| p == param) {
			None => push(join(path, "param"), format!("no parameter \"{param}\" in {action}{}", did_you_mean(param, kinds.iter().map(|(p, _)| p.as_str())))),
			Some((_, k)) => {
				let fits = match kind {
					"select" => *k == "choice",
					"slider" => *k == "range",
					"toggle" => *k == "bool" || *k == "choice",
					_ => true,
				};
				if !fits {
					push(join(path, "param"), format!("a {kind} sets a {} parameter; {param} of {action} is a {k}", match kind {
						"select" => "choice",
						"slider" => "range",
						_ => "bool or choice",
					}));
				}
				if kind == "toggle"
					&& *k == "choice"
					&& let Some(p) = spec.as_ref().and_then(|s| s.params.iter().find(|p| &p.id == param))
				{
					for (key, v) in [("on", &w.on), ("off", &w.off)] {
						match v {
							Some(v) if !actions::param_takes(p, v) => push(join(path, key), format!("{} isn't one of {param}'s choices", found(v))),
							None => push(path.into(), format!("a toggle of the choice {param} needs on and off")),
							_ => {}
						}
					}
				}
			}
		}
	}
	for (p, v) in w.params.iter().flatten() {
		let ppath = join(&join(path, "params"), p);
		if !kinds.iter().any(|(k, _)| k == p) {
			push(ppath, format!("no parameter \"{p}\" in {action}{}", did_you_mean(p, kinds.iter().map(|(k, _)| k.as_str()))));
			continue;
		}
		let placeholder = v.as_str().is_some_and(|s| s.contains('{'));
		if let Some(param) = spec.as_ref().and_then(|s| s.params.iter().find(|x| &x.id == p))
			&& !placeholder
			&& !actions::param_takes(param, v)
		{
			push(ppath, format!("{} isn't a value {p} takes", found(v)));
		}
	}
}

/// A table's or a bar chart's collection, and the fields it names.
fn check_source(c: &Config, source: &str, w: &Widget, kind: &str, path: &str, push: &mut impl FnMut(String, String)) {
	let Some(def) = values::collection_def(source) else {
		push(join(path, "source"), no_collection(source));
		return;
	};
	let fields: Vec<&str> = std::iter::once("key").chain(def.fields.iter().map(|f| f.0)).collect();
	// Each field named, and where.
	let mut named: Vec<(&str, String)> = Vec::new();
	if kind == "bars" {
		if let Some(f) = &w.field {
			named.push((f, join(path, "field")));
		}
		if let Some(Labels::Field(f)) = &w.labels {
			named.push((f, join(path, "labels")));
		}
	}
	for (key, cols) in [("columns", &w.columns), ("expand", &w.expand)] {
		for (i, col) in cols.iter().flatten().enumerate() {
			let cpath = format!("{}[{i}]", join(path, key));
			named.push((&col.field, join(&cpath, "field")));
			if let Some(Bar { max: Bound::Name(m) }) = &col.bar
				&& m != "top"
			{
				named.push((m, join(&join(&cpath, "bar"), "max")));
			}
		}
	}
	if let Some(s) = &w.sort {
		named.push((&s.field, join(&join(path, "sort"), "field")));
	}
	for (i, s) in w.sorts.iter().flatten().enumerate() {
		named.push((s, format!("{}[{i}]", join(path, "sorts"))));
	}
	for f in w.filter.iter().flat_map(|f| f.keys()) {
		named.push((f, join(&join(path, "filter"), f)));
	}
	if let Some(f) = &w.row_open {
		named.push((f, join(path, "row_open")));
	}
	for (i, ra) in w.row_actions.iter().flatten().enumerate() {
		let rpath = format!("{}[{i}]", join(path, "row_actions"));
		if !c.has_action(&ra.action) {
			push(join(&rpath, "action"), format!("no action \"{}\"", ra.action));
			continue;
		}
		let kinds = c.param_kinds(&ra.action).unwrap_or_default();
		for (p, v) in ra.params.iter().flatten() {
			if !kinds.iter().any(|(k, _)| k == p) {
				push(join(&join(&rpath, "params"), p), format!("no parameter \"{p}\" in {}", ra.action));
			}
			if let Some(f) = v.as_str().and_then(|s| s.strip_prefix('{')).and_then(|s| s.strip_suffix('}')) {
				named.push((f, join(&join(&rpath, "params"), p)));
			}
		}
	}
	for (f, at) in named {
		if !fields.contains(&f) {
			push(at, format!("no field \"{f}\" in {source}{}", did_you_mean(f, fields.iter().copied())));
		}
	}
}

// ---- the JSON Schema ------------------------------------------------------------------------

fn schema_of(ty: &Ty) -> Value {
	use serde_json::json;
	let placeholder = json!({ "type": "string", "pattern": "\\{" });
	match ty {
		Ty::Str | Ty::Text | Ty::Address => json!({ "type": "string" }),
		Ty::Id => json!({ "type": "string", "pattern": ID_PATTERN }),
		Ty::Bool => json!({ "type": "boolean" }),
		Ty::Int(lo, hi) => json!({ "type": "integer", "minimum": lo, "maximum": hi }),
		Ty::Num => json!({ "type": "number" }),
		Ty::Scalar => json!({ "type": ["string", "number", "boolean"] }),
		Ty::Enum(_, words) => json!({ "anyOf": [{ "enum": words }, placeholder] }),
		Ty::Format => json!({ "anyOf": [{ "enum": format::FORMATS }, { "type": "string", "pattern": "^\\{:\\.[0-6]\\}$" }] }),
		Ty::List(t) => json!({ "type": "array", "items": schema_of(t) }),
		Ty::Map(t) => json!({ "type": "object", "additionalProperties": schema_of(t) }),
		Ty::IdMap(t) => json!({ "type": "object", "propertyNames": { "pattern": ID_PATTERN }, "additionalProperties": schema_of(t) }),
		Ty::Rec(name) => json!({ "$ref": format!("#/$defs/{name}") }),
		Ty::Any(ts) => json!({ "anyOf": ts.iter().map(schema_of).collect::<Vec<_>>() }),
		Ty::Widget => json!({ "$ref": "#/$defs/widget" }),
	}
}

fn object_schema(keys: &[Key]) -> Value {
	let props: Map<String, Value> = keys.iter().map(|k| (k.name.to_string(), schema_of(&k.ty))).collect();
	serde_json::json!({ "type": "object", "properties": props, "additionalProperties": false })
}

/// The JSON Schema of a settings file, from the definitions the settings are checked against. A
/// file may leave out what another gives, so nothing in it is required.
pub fn json_schema() -> Value {
	use serde_json::json;
	let mut defs = Map::new();
	for name in RECORDS.iter().filter(|n| **n != "settings") {
		defs.insert(name.to_string(), object_schema(&record(name)));
	}
	let mut widget = object_schema(&widget_keys(None));
	let per_type: Vec<Value> = WIDGET_TYPES
		.iter()
		.map(|t| {
			let keys = widget_keys(Some(t));
			let names: Vec<&str> = keys.iter().map(|k| k.name).collect();
			let props: Map<String, Value> = keys.iter().map(|k| (k.name.to_string(), schema_of(&k.ty))).collect();
			json!({
				"if": { "properties": { "type": { "const": t } }, "required": ["type"] },
				"then": { "propertyNames": { "enum": names }, "properties": props }
			})
		})
		.collect();
	widget["allOf"] = Value::Array(per_type);
	defs.insert("widget".into(), widget);
	let mut top = object_schema(&record("settings"));
	top["$schema"] = "https://json-schema.org/draft/2020-12/schema".into();
	top["title"] = "elamond settings (schema 2)".into();
	top["$defs"] = Value::Object(defs);
	top
}

#[cfg(test)]
mod tests {
	use super::*;

	fn read(files: &[(&str, &str)]) -> Result<Config, Vec<Mistake>> {
		read_sources(&files.iter().map(|(n, t)| Source::new(*n, *t)).collect::<Vec<_>>())
	}

	fn mistakes(files: &[(&str, &str)]) -> String {
		join_mistakes(&read(files).unwrap_err())
	}

	#[test]
	fn the_built_in_settings_and_the_fixture_are_sound() {
		let c = Config::default();
		assert_eq!(c.default_profile, "default");
		assert!(["overview", "processes", "sensors", "control", "cpu", "memory", "gpu", "network", "storage", "battery", "activity", "header"]
			.iter()
			.all(|d| c.dashboards.contains_key(*d)));
		let fixture = read(&[("ui-source.toml", include_str!("../fixtures/ui-source.toml"))]).unwrap();
		assert_eq!(fixture.actions.keys().collect::<Vec<_>>(), ["backup", "fan", "mode"]);
		assert_eq!(fixture.links["web"].unit.as_deref(), Some("web.service"));
		assert!(fixture.dashboards.contains_key("overview"), "the built-in dashboards stay");
	}

	#[test]
	fn tables_merge_key_by_key_and_lists_replace() {
		let c = read(&[
			("a.toml", "listen = [\"eth0\"]\n[dashboards.overview]\ntitle = \"Home\"\n[profiles.default]\ntheme = \"dark\"\n"),
			("b.toml", "[[dashboards.overview.widgets]]\ntype = \"text\"\ntext = \"Hello\"\n[profiles.mine]\nuse = \"default\"\nname = \"Mine\"\n"),
		]);
		let c = c.unwrap();
		let o = &c.dashboards["overview"];
		assert_eq!((o.title.as_str(), o.icon.as_deref(), o.widgets.len()), ("Home", Some("overview"), 1));
		let p = c.profile("mine");
		assert_eq!((p.name.as_str(), p.theme.as_str(), p.dashboards.len()), ("Mine", "dark", 4));
		assert_eq!(c.profile("default").name, "Default");
		// A profile that isn't there is the default one.
		assert_eq!(c.profile("gone").id, "default");
		let mut m = toml::Table::new();
		m.insert("listen".into(), toml::Value::Array(vec!["eth0".into(), "wg0".into()]));
		let mut a = serde_json::to_value(&m).unwrap().as_object().unwrap().clone();
		merge(&mut a, serde_json::json!({ "listen": ["tailscale0"] }).as_object().unwrap().clone());
		assert_eq!(a["listen"], serde_json::json!(["tailscale0"]));
	}

	#[test]
	fn mistakes_name_the_file_the_line_and_what_was_meant() {
		let e = mistakes(&[("power.toml", "[dashboards.power]\ntitle = \"Power\"\n\n[[dashboards.power.widgets]]\ntype = \"gauge\"\nvalue = \"cpu.usage\"\n\n[[dashboards.power.widgets]]\ntype = \"value\"\nvalue = \"cpu.temp\"\n\n[[dashboards.power.widgets]]\ntype = \"guage\"\nvalue = \"cpu.usage\"\n")]);
		assert_eq!(e, "power.toml:12: dashboards.power.widgets[2]: no widget type \"guage\"; did you mean gauge?");
		let e = mistakes(&[("a.toml", "port = \"x\"\n[links.web]\nname = \"Web\"\nurl = \"http://{host}\"\nlable = \"x\"\n")]);
		assert!(e.contains("a.toml:1: port: should be an integer from 1 to 65535, not \"x\""), "{e}");
		assert!(e.contains("a.toml:5: links.web.lable: no key \"lable\" here"), "{e}");
		let e = mistakes(&[("a.toml", "[[dashboards.overview.widgets]]\ntype = \"gauge\"\nvalue = \"cpu.usgae\"\nlable = \"CPU\"\nspan = 6\n")]);
		assert!(e.contains("a.toml:3: dashboards.overview.widgets[0].value: no value \"cpu.usgae\"; did you mean cpu.usage?"), "{e}");
		assert!(e.contains("a.toml:4: dashboards.overview.widgets[0].lable: no key \"lable\" in a gauge widget; did you mean label?"), "{e}");
		assert!(e.contains("a.toml:5: dashboards.overview.widgets[0].span: should be an integer from 1 to 4, not 6"), "{e}");
		let e = mistakes(&[("a.toml", "x = 1 = 2\n")]);
		assert!(e.starts_with("a.toml:1: "), "{e}");
		// Settings schema 1, with what replaces it.
		let e = mistakes(&[("old.toml", "units = [\"a.service\"]\n[[links]]\nname = \"a\"\nurl = \"http://{host}\"\n[[actions]]\nname = \"b\"\ncommand = [\"true\"]\n")]);
		assert!(e.contains("old.toml:1: units: settings schema 1's units are gone"), "{e}");
		assert!(e.contains("old.toml:2: links: settings schema 1's [[links]] is gone: give each a table of its own, [links.<id>]"), "{e}");
		assert!(e.contains("actions: settings schema 1's [[actions]] is gone"), "{e}");
	}

	#[test]
	fn what_refers_to_what() {
		let e = mistakes(&[(
			"a.toml",
			r#"
default_profile = "owner"

[profiles.mine]
use = "theirs"
dashboards = ["overveiw"]

[profiles.loop1]
use = "loop2"

[profiles.loop2]
use = "loop1"

[actions.mode]
name = "Mode"
command = ["modectl", "{mood}"]
params.mode = { choices = ["a", "b"], default = "c" }

[actions.fan]
name = "Fan"
command = ["fan", "{speed}"]
params.speed = { range = [0, 100], step = 5, default = 52 }

[actions.both]
name = "Both"
command = ["x"]
params.x = { choices = ["a"], type = "bool" }

[dashboards.mine]
title = "Mine {item.name}"

[[dashboards.mine.widgets]]
type = "select"
action = "fan"
param = "speed"
open = "nowhere"

[[dashboards.mine.widgets]]
id = "same"
type = "button"
action = "reboot"

[[dashboards.mine.widgets]]
id = "same"
type = "table"
source = "procs"
columns = []

[[dashboards.mine.widgets]]
type = "table"
source = "processes"
columns = [{ field = "nmae" }]
row_actions = [{ action = "process.signal", params = { pid = "{pdi}" } }]

[[dashboards.mine.widgets]]
use = "missing"

[[dashboards.mine.widgets]]
type = "text"
text = "{cpu.temp:tmep} {name}"

[[dashboards.mine.widgets]]
type = "value"
for_each = "gpus"
value = "gpu.{item}.usage"
label = "{item.nmae}"
"#,
		)]);
		for want in [
			"a.toml:2: default_profile: no profile \"owner\"",
			"a.toml:5: profiles.mine.use: no profile \"theirs\"",
			"a.toml:6: profiles.mine.dashboards[0]: no dashboard \"overveiw\"; did you mean overview?",
			"profiles.loop1.use: goes round in a loop: loop1 → loop2 → loop1",
			"actions.mode.command[1]: {mood} names no parameter",
			"actions.mode.params.mode.default: \"c\" isn't a value the parameter takes",
			"actions.fan.params.speed.default: 52 isn't a value the parameter takes",
			"actions.both.params.x: give exactly one of",
			"dashboards.mine.title: {item.name} with no for_each to give an item",
			"dashboards.mine.widgets[0].param: a select sets a choice parameter; speed of fan is a range",
			"dashboards.mine.widgets[0].open: no dashboard \"nowhere\"",
			"dashboards.mine.widgets[1].action: no action \"reboot\"",
			"dashboards.mine.widgets[2]: id \"same\" is the id of widgets[1] too",
			"dashboards.mine.widgets[2].source: no collection \"procs\"",
			"dashboards.mine.widgets[3].columns[0].field: no field \"nmae\" in processes; did you mean name?",
			"dashboards.mine.widgets[3].row_actions[0].params.pid: no field \"pdi\" in processes; did you mean pid?",
			"dashboards.mine.widgets[4].use: no template \"missing\"",
			"dashboards.mine.widgets[5].text: no format \"tmep\" in {cpu.temp:tmep}; did you mean temp?",
			"{name} is no value",
			"dashboards.mine.widgets[6].label: no field \"nmae\" in gpus; did you mean name?",
		] {
			assert!(e.contains(want), "missing: {want}\nin:\n{e}");
		}
	}

	#[test]
	fn templates_fill_in_and_give_way() {
		let c = read(&[(
			"a.toml",
			"[templates.hist]\ntype = \"graph\"\nrange = 300\nvalues = [\"{src}.usage\"]\nlabel = \"{name}\"\n\n[dashboards.t]\ntitle = \"T\"\n[[dashboards.t.widgets]]\nuse = \"hist\"\nwith = { src = \"cpu\", name = \"CPU\" }\nrange = 60\n",
		)])
		.unwrap();
		let w = apply_template(&c.dashboards["t"].widgets[0], &c.templates).unwrap();
		assert_eq!(w["values"], serde_json::json!(["cpu.usage"]));
		assert_eq!((w["label"].as_str(), w["range"].as_i64(), w.contains_key("use")), (Some("CPU"), Some(60), false));
		let e = mistakes(&[("a.toml", "[templates.a]\nuse = \"b\"\n[templates.b]\nuse = \"a\"\n")]);
		assert!(e.contains("use goes round in a loop: a → b → a"), "{e}");
		// A template's key that its type doesn't take is the widget's mistake.
		let e = mistakes(&[("a.toml", "[templates.t]\nspark = true\n[dashboards.t]\ntitle = \"T\"\n[[dashboards.t.widgets]]\nuse = \"t\"\ntype = \"text\"\ntext = \"x\"\n")]);
		assert!(e.contains("no key \"spark\" in a text widget") && e.contains("(with template t)"), "{e}");
	}

	#[test]
	fn suggestions_are_near_words_only() {
		assert_eq!(suggest("guage", WIDGET_TYPES), Some("gauge"));
		assert_eq!(suggest("lable", ["label", "level"]), Some("label"));
		assert_eq!(suggest("x", WIDGET_TYPES), None);
		assert_eq!(suggest("chart", WIDGET_TYPES), None);
		assert!(is_id("a-1_b") && !is_id("_a") && !is_id("A") && !is_id("a.b") && !is_id(&"a".repeat(33)));
		assert_eq!(parent("a.b[2].c"), Some("a.b[2]"));
		assert_eq!(parent("a.b[2]"), Some("a.b"));
		assert_eq!(parent("a.\"x.y\""), Some("a"));
		assert_eq!(join("map", "true"), "map.true");
		assert_eq!(join("map", "a b"), "map.\"a b\"");
	}

	/// Whether `v` is valid against `schema`, as far as the JSON Schema `json_schema` writes goes.
	fn valid(schema: &Value, v: &Value, root: &Value) -> bool {
		let s = |k: &str| schema.get(k);
		if let Some(r) = s("$ref").and_then(Value::as_str) {
			let def = &root["$defs"][r.trim_start_matches("#/$defs/")];
			return valid(def, v, root);
		}
		let type_ok = |t: &str| match t {
			"string" => v.is_string(),
			"boolean" => v.is_boolean(),
			"integer" => v.is_i64() || v.is_u64(),
			"number" => v.is_number(),
			"array" => v.is_array(),
			"object" => v.is_object(),
			_ => false,
		};
		match s("type") {
			Some(Value::String(t)) if !type_ok(t) => return false,
			Some(Value::Array(ts)) if !ts.iter().filter_map(Value::as_str).any(type_ok) => return false,
			_ => {}
		}
		if s("enum").and_then(Value::as_array).is_some_and(|e| !e.contains(v)) || s("const").is_some_and(|c| c != v) {
			return false;
		}
		if let (Some(p), Some(text)) = (s("pattern").and_then(Value::as_str), v.as_str()) {
			let ok = match p {
				ID_PATTERN => is_id(text),
				"\\{" => text.contains('{'),
				"^\\{:\\.[0-6]\\}$" => format::is_format(text) && text.starts_with('{'),
				other => panic!("a pattern the test doesn't know: {other}"),
			};
			if !ok {
				return false;
			}
		}
		if let Some(n) = v.as_f64()
			&& (s("minimum").and_then(Value::as_f64).is_some_and(|m| n < m) || s("maximum").and_then(Value::as_f64).is_some_and(|m| n > m))
		{
			return false;
		}
		if let (Some(items), Some(a)) = (s("items"), v.as_array())
			&& !a.iter().all(|e| valid(items, e, root))
		{
			return false;
		}
		if let Some(o) = v.as_object() {
			let props = s("properties").and_then(Value::as_object);
			for (k, e) in o {
				let ok = match (props.and_then(|p| p.get(k)), s("additionalProperties")) {
					(Some(p), _) => valid(p, e, root),
					(None, Some(Value::Bool(false))) => false,
					(None, Some(a)) => valid(a, e, root),
					(None, None) => true,
				};
				if !ok || s("propertyNames").is_some_and(|n| !valid(n, &Value::String(k.clone()), root)) {
					return false;
				}
			}
			if s("required").and_then(Value::as_array).is_some_and(|r| r.iter().filter_map(Value::as_str).any(|k| !o.contains_key(k))) {
				return false;
			}
		}
		if s("anyOf").and_then(Value::as_array).is_some_and(|a| !a.iter().any(|x| valid(x, v, root))) {
			return false;
		}
		if s("allOf").and_then(Value::as_array).is_some_and(|a| !a.iter().all(|x| valid(x, v, root))) {
			return false;
		}
		if let (Some(cond), Some(then)) = (s("if"), s("then"))
			&& valid(cond, v, root)
			&& !valid(then, v, root)
		{
			return false;
		}
		true
	}

	fn as_json(toml_text: &str) -> Value {
		serde_json::to_value(toml_text.parse::<toml::Table>().unwrap()).unwrap()
	}

	#[test]
	fn the_json_schema_takes_the_settings_and_refuses_mistakes() {
		let schema = json_schema();
		assert_eq!(schema["$schema"], "https://json-schema.org/draft/2020-12/schema");
		for text in [DEFAULTS, include_str!("../fixtures/ui-source.toml")] {
			assert!(valid(&schema, &as_json(text), &schema));
		}
		for bad in [
			"[[dashboards.a.widgets]]\ntype = \"guage\"\n",
			"[[dashboards.a.widgets]]\ntype = \"text\"\nspark = true\n",
			"[[dashboards.a.widgets]]\ntype = \"bars\"\nlabels = [\"a\"]\n",
			"[[dashboards.a.widgets]]\ntype = \"gauge\"\nspan = 5\n",
			"[links.Bad]\nname = \"x\"\nurl = \"y\"\n",
			"[profiles.p]\ntheme = \"blue\"\n",
			"[actions.a]\nname = \"a\"\ncommand = [\"a\"]\nrole = \"glance\"\n",
			"pages = 1\n",
		] {
			assert!(!valid(&schema, &as_json(bad), &schema), "{bad}");
		}
		assert!(valid(&schema, &as_json("[[dashboards.a.widgets]]\ntype = \"graph\"\nlabels = [\"a\"]\nicon = \"{item.icon}\"\n"), &schema));
	}

	/// docs/config.md names everything the settings take, so the reference can't fall behind them.
	#[test]
	fn the_reference_names_every_key_type_action_and_value() {
		let doc = include_str!("../../docs/config.md");
		// Every word in `…`, so a key listed as `{ field, desc }` counts.
		let words: BTreeSet<&str> = doc
			.split('`')
			.skip(1)
			.step_by(2)
			.flat_map(|span| span.split(|c: char| !(c.is_ascii_alphanumeric() || "_.<>-{}:".contains(c))))
			.collect();
		let named = |s: &str| words.contains(s) || doc.contains(&format!("`{s}`"));
		let mut missing = Vec::new();
		let mut want = |what: &str, s: String, ok: bool| {
			if !ok {
				missing.push(format!("{what} {s}"));
			}
		};
		for t in WIDGET_TYPES {
			want("widget type", t.into(), named(t));
		}
		for k in widget_keys(None) {
			want("widget key", k.name.into(), named(k.name));
		}
		for r in ["item", "column", "expand", "bar", "sort", "row_action", "link", "action", "param", "dashboard"] {
			for k in record(r) {
				want(r, k.name.into(), named(k.name));
			}
		}
		for k in record("settings") {
			want("top-level key", k.name.into(), named(k.name));
		}
		for k in record("profile").iter().chain(&record("unlock")).chain(&record("android")) {
			let ok = named(k.name) || doc.contains(&format!("`unlock.{}`", k.name));
			want("profile key", k.name.into(), ok);
		}
		for (id, ..) in actions::BUILT_IN {
			want("built-in action", id.into(), named(id));
		}
		for kind in actions::KINDS {
			want("action kind", kind.into(), named(kind));
		}
		let prefixes: BTreeSet<&str> = values::ADDRESSES.iter().filter_map(|a| a.pattern.split('.').next()).collect();
		for p in prefixes {
			want("address prefix", p.into(), doc.contains(&format!("`{p}.")));
		}
		for a in values::ADDRESSES.iter().filter(|a| !a.pattern.contains('<')) {
			let last = a.pattern.rsplit('.').next().unwrap_or("");
			want("address", a.pattern.into(), named(a.pattern) || doc.contains(&format!("`.{last}`")));
		}
		for c in values::COLLECTIONS {
			want("collection", c.name.into(), named(c.name));
			for (f, ..) in c.fields {
				want(&format!("field of {}", c.name), f.to_string(), named(f));
			}
		}
		for f in format::FORMATS.iter().chain(&ICONS).chain(&COLORS).chain(&THEMES).chain(&GRACES).chain(&HEIGHTS) {
			want("word", f.to_string(), named(f));
		}
		assert!(missing.is_empty(), "docs/config.md doesn't name: {}", missing.join(", "));
	}
}
