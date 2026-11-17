//! The settings, schema 3 (docs/config.md): elamond.toml, then elamond.d/*.toml, a table with an
//! id merging key by key with the one before and a list replacing it whole. Each file is checked on its own first,
//! so a mistake names the file and the line it is in; then the settings as merged, for what refers
//! to what. The same definitions give the JSON Schema `elamond schema` prints.

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

/// The settings elamond starts with, as `elamond setup` and the first start write them: the starter
/// elamond.toml, then elamond.d/dashboards.toml, the standard dashboards and their templates.
pub const STARTER: &str = include_str!("starter.toml");
pub const DASHBOARDS: &str = include_str!("dashboards.toml");
/// The two as one file of settings schema 3: `elamond defaults`, defaults.toml beside the settings.
pub const DEFAULTS: &str = concat!(include_str!("starter.toml"), "\n", include_str!("dashboards.toml"));
/// What mistakes in them are said to be in.
pub const DEFAULTS_NAME: &str = "built-in defaults";
/// The settings schema this elamond reads: `schema = 3`, a whole file.
pub const SCHEMA: i64 = 3;

pub const WIDGET_TYPES: [&str; 17] = [
	"value", "gauge", "meter", "status", "graph", "list", "bars", "table", "button", "toggle", "select", "slider", "text", "group", "media", "timeline",
	"log",
];
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
/// Which journal a `log` shows, and the levels it may show from, the least first.
pub const LOG_SCOPES: [&str; 3] = ["system", "user", "unit"];
pub const LOG_LEVELS: [&str; 6] = ["debug", "info", "notice", "warning", "error", "critical"];
/// What a timeline's legend tells of each state: how long, what share of the known time, or both.
pub const LEGENDS: [&str; 3] = ["duration", "percent", "both"];
/// The roles an action may ask for.
pub const ACTION_ROLES: [&str; 2] = ["operate", "admin"];
/// The widgets that are 4 columns wide unless they say otherwise.
pub const WIDE: [&str; 8] = ["graph", "table", "list", "group", "media", "text", "timeline", "log"];

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
	/// UPSes and power stations, as upsd serves them: the sources of kind `nut`.
	pub sources: BTreeMap<String, NutSource>,
	/// The sources of kind `command`, `http` and `file`, taken out of `[sources]` as the files are
	/// read: what a program prints, a web service answers or a file holds.
	#[serde(skip)]
	pub feeds: BTreeMap<String, Feed>,
	/// `[history]`: how long the hourly history is kept.
	pub history: HistoryDef,
	/// Where each profile's table is: the file that set it last, and the line of its
	/// `[profiles.<id>]` there. Filled in as the files are read, not a key of theirs.
	#[serde(skip)]
	pub profile_sources: BTreeMap<String, Whence>,
}

/// A place in the settings files: a file as `Source::name` gives it, and a line from 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Whence {
	pub file: String,
	pub line: usize,
}

/// `[history]`.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct HistoryDef {
	/// Days the hourly points are kept; `None` for all of them (`"all"`, the default).
	#[serde(deserialize_with = "keep_days")]
	pub keep: Option<u64>,
}

/// `keep`: `"all"` or a number of days, as checked before.
fn keep_days<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
	#[derive(Deserialize)]
	#[serde(untagged)]
	enum Keep {
		Days(u64),
		Word(serde::de::IgnoredAny),
	}
	Ok(match Keep::deserialize(d)? {
		Keep::Days(n) => Some(n),
		Keep::Word(_) => None,
	})
}

/// `[sources.<id>]`: a UPS or a power station on upsd (`kind = "nut"`), its values named
/// `<id>.<variable>`.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct NutSource {
	pub kind: String,
	/// The UPS's name on upsd.
	pub ups: String,
	#[serde(default = "localhost")]
	pub host: String,
	#[serde(default = "nut_port")]
	pub port: u16,
	/// The battery's energy, for `<id>.estimate.watts`.
	#[serde(default)]
	pub capacity_wh: Option<f64>,
	/// How often upsd is asked while a client shows one of the source's values.
	#[serde(default = "second")]
	pub interval_ms: u64,
	/// How often otherwise, while its history is kept or a graph shows it.
	#[serde(default)]
	pub background_ms: Option<u64>,
	/// Keep the long history of every number it has.
	#[serde(default = "yes")]
	pub history: bool,
	/// An `upst record` to fill the history from: a socket path or `host[:port]`.
	#[serde(default)]
	pub recorder: Option<String>,
	/// The NUT user that runs commands and settings, and the file whose first line is its password.
	#[serde(default)]
	pub user: Option<String>,
	#[serde(default)]
	pub password_file: Option<PathBuf>,
	/// Patterns of the station's commands and writable variables clients may run and set.
	#[serde(default)]
	pub commands: Vec<String>,
	#[serde(default)]
	pub settings: Vec<String>,
	/// The least role that may run them.
	#[serde(default = "admin")]
	pub role: Role,
	/// Outlets by number.
	#[serde(default)]
	pub outlets: BTreeMap<String, OutletDef>,
}

/// `outlets."<N>"` of a source.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct OutletDef {
	/// It powers this computer: its commands but the one that turns it on are typed in.
	pub critical: bool,
}

impl NutSource {
	/// How often upsd is asked while nobody looks: `background_ms`, or 10 seconds, or `interval_ms`
	/// when that is more.
	pub fn background(&self) -> u64 {
		self.background_ms.unwrap_or(crate::consts::cadence::NUT_BACKGROUND_MS.max(self.interval_ms))
	}

	/// Whether outlet `n` powers this computer.
	pub fn critical(&self, n: u32) -> bool {
		self.outlets.get(&n.to_string()).is_some_and(|o| o.critical)
	}
}

/// `[sources.<id>]` of kind `command`, `http` or `file`: what a program prints, a web service
/// answers or a file holds, read every `interval` while it is needed, its values named
/// `<id>.<path>`. Each kind's own keys are left out of the others (`elamond check`).
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Feed {
	pub kind: FeedKind,
	/// `command`: the program and its arguments, run without a shell.
	#[serde(default)]
	pub command: Vec<String>,
	/// `http`: what is asked with GET.
	#[serde(default)]
	pub url: Option<String>,
	/// `http`: the file whose first line is the token sent with each request; the path alone is
	/// kept here, the token is read at each request (`feeds::Token`).
	#[serde(default)]
	pub token_file: Option<PathBuf>,
	/// `http`: the header the token goes in; `Authorization: Bearer <token>` without one.
	#[serde(default)]
	pub header: Option<String>,
	/// `file`: what is read.
	#[serde(default)]
	pub path: Option<PathBuf>,
	#[serde(default)]
	pub format: FeedFormat,
	/// Seconds between reads while it is needed.
	#[serde(default = "feed_interval")]
	pub interval: u64,
	/// Seconds a read may take.
	#[serde(default = "feed_timeout")]
	pub timeout: u64,
	/// Keep the long history of its numbers.
	#[serde(default)]
	pub history: bool,
	/// The least role that sees its values.
	#[serde(default = "monitor")]
	pub role: Role,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FeedKind {
	Command,
	Http,
	File,
}

/// How what a source reads is taken: as JSON, as one text, or as `key = value` lines.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum FeedFormat {
	#[default]
	Json,
	Text,
	Lines,
}

fn feed_interval() -> u64 {
	crate::consts::cadence::FEED_INTERVAL_SECS
}

fn feed_timeout() -> u64 {
	crate::consts::timeout::FEED_SECS
}

fn monitor() -> Role {
	Role::Monitor
}

fn localhost() -> String {
	"localhost".into()
}

fn nut_port() -> u16 {
	upst_core::nut::DEFAULT_PORT
}

fn second() -> u64 {
	1000
}

fn admin() -> Role {
	Role::Admin
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
	/// A graph offers the long history (`/api/v1/history`) unless this is false or it is small.
	pub ranges: Option<bool>,
	pub items: Option<Vec<ListItem>>,
	pub source: Option<String>,
	pub field: Option<String>,
	pub columns: Option<Vec<Column>>,
	pub sort: Option<Sort>,
	pub sorts: Option<Vec<String>>,
	/// What the chips that pick another sort say, by field.
	pub sort_labels: Option<BTreeMap<String, String>>,
	pub search: Option<Search>,
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
	/// A `log`'s journal: `system`, `user` or `unit` (of `unit`).
	pub scope: Option<String>,
	/// The lines a client keeps of a `log`.
	pub lines: Option<u64>,
	/// What a `timeline`'s legend tells: `duration`, `percent` or `both`.
	pub legend: Option<String>,
}

/// A table's search: on or off, or the fields it searches.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum Search {
	On(bool),
	Fields(Vec<String>),
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

/// What a table's row does: runs an action with its fields, or opens its own dashboard of those
/// `open` names (`unit_log` → `unit_log.<the row's key>`).
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct RowAction {
	pub action: Option<String>,
	pub open: Option<String>,
	pub label: Option<String>,
	pub params: Option<Map<String, Value>>,
}

impl Default for Config {
	/// The settings elamond starts with, `DEFAULTS` read as the one file.
	fn default() -> Self {
		static BUILT_IN: std::sync::OnceLock<Config> = std::sync::OnceLock::new();
		BUILT_IN
			.get_or_init(|| {
				read_sources(&[Source::new(DEFAULTS_NAME, DEFAULTS)])
					.unwrap_or_else(|m| panic!("the built-in defaults have mistakes: {}", join_mistakes(&m)))
			})
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
			sources: BTreeMap::new(),
			feeds: BTreeMap::new(),
			history: HistoryDef::default(),
			profile_sources: BTreeMap::new(),
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
			unlock_for: ["power", "nut", "signal", "actions", "units"].map(String::from).to_vec(),
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
		self.actions.contains_key(id) || actions::BUILT_IN.iter().any(|b| b.0 == id) || self.station_param(id).is_some()
	}

	/// The one parameter of a station's action of one of the sources (`nut.command.eco`: `cmd`,
	/// `nut.set.eco.<variable>`: `value`). Which commands and variables the station has, and what
	/// each takes, only upsd says, which the settings are never checked against.
	pub fn station_param(&self, id: &str) -> Option<&'static str> {
		use crate::nut::actions::Target;
		match crate::nut::actions::target(id)? {
			Target::Command { source } => self.sources.contains_key(source).then_some("cmd"),
			Target::Set { source, .. } => self.sources.contains_key(source).then_some("value"),
		}
	}

	/// The kinds of an action's parameters by id: `choice`, `range`, `bool`, `integer`, `number`
	/// or `string`.
	pub fn param_kinds(&self, action: &str) -> Option<Vec<(String, &'static str)>> {
		if let Some(a) = self.actions.get(action) {
			return Some(actions::settings_spec(action, a).params.iter().map(|p| (p.id.clone(), p.kind.name())).collect());
		}
		// A station's: its kind is the station's to say.
		if let Some(p) = self.station_param(action) {
			return Some(vec![(p.to_string(), "station")]);
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

/// The settings from `files`, the main file first; those not there are left out.
pub fn load(files: &[PathBuf]) -> Result<Config, Vec<Mistake>> {
	let sources = read_files(files)?;
	read_sources(&sources)
}

/// Whether the first of `sources`, as read from `files`, is the main file (the first of `files`),
/// which may not be there.
pub fn main_in(files: &[PathBuf], sources: &[Source]) -> bool {
	files.first().zip(sources.first()).is_some_and(|(f, s)| s.name == f.display().to_string())
}

/// Test settings written as drop-ins over `DEFAULTS`, which they lean on for the standard dashboards.
#[cfg(test)]
pub fn read_over_defaults(sources: &[Source]) -> Result<Config, Vec<Mistake>> {
	let all: Vec<Source> = std::iter::once(Source::new(DEFAULTS_NAME, DEFAULTS)).chain(sources.iter().cloned()).collect();
	read_sources(&all)
}

/// The settings from `sources`, in that order, or every mistake found.
pub fn read_sources(sources: &[Source]) -> Result<Config, Vec<Mistake>> {
	read_merged(sources).map(|(c, _)| c)
}

/// `read_settings`, with the settings as merged from the files before they were taken up: what
/// two ways of writing them are compared by.
pub fn read_merged(sources: &[Source]) -> Result<(Config, Map<String, Value>), Vec<Mistake>> {
	let all = sources;
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
	let raw = merged.clone();
	let mut merged = Value::Object(merged);
	let mut bad = Vec::new();
	check_top(&merged, false, &mut bad);
	if !bad.is_empty() {
		return Err(bad.into_iter().map(|(p, m)| at(&p, m)).collect());
	}
	// `sources` keeps the UPSes; the other kinds are taken out to `feeds`.
	let mut feeds = Map::new();
	if let Some(Value::Object(s)) = merged.get_mut("sources") {
		let ids: Vec<String> = s.iter().filter(|(_, v)| v.get("kind").and_then(Value::as_str) != Some("nut")).map(|(id, _)| id.clone()).collect();
		for id in ids {
			if let Some(v) = s.remove(&id) {
				feeds.insert(id, v);
			}
		}
	}
	let read = |e: serde_json::Error| vec![Mistake { file: None, line: None, path: String::new(), message: e.to_string() }];
	let mut config: Config = serde_json::from_value(merged).map_err(read)?;
	config.feeds = serde_json::from_value(Value::Object(feeds)).map_err(read)?;
	for id in config.profiles.keys() {
		if let Some((i, line)) = whence.get(&join("profiles", id)) {
			config.profile_sources.insert(id.clone(), Whence { file: all[*i].name.clone(), line: *line });
		}
	}
	let bad = semantics(&config);
	if !bad.is_empty() {
		return Err(bad.into_iter().map(|(p, m)| at(&p, m)).collect());
	}
	Ok((config, raw))
}

/// What `elamond check` warns of in settings that are sound: what leaves phones with little or
/// nothing to show.
pub fn warnings(c: &Config) -> Vec<String> {
	let mut out = Vec::new();
	if c.dashboards.is_empty() {
		out.push("there are no dashboards: paired phones show nothing (`elamond defaults` prints the standard ones)".into());
	}
	if !c.profiles.contains_key(&c.default_profile) {
		let there = if c.profiles.is_empty() { "there are no profiles".to_string() } else { format!("the profiles are {}", c.profiles.keys().map(String::as_str).collect::<Vec<_>>().join(", ")) };
		out.push(format!(
			"default_profile \"{}\" is none of the profiles ({there}){}: a phone whose profile is gone gets elamond's plain one, and the table pairing makes a phone uses none",
			c.default_profile,
			did_you_mean(&c.default_profile, c.profiles.keys().map(String::as_str))
		));
	}
	for (id, s) in &c.sources {
		if let Some(p) = &s.password_file {
			out.extend(secret_file_warning(id, "password_file", p, "its station's commands and settings fail until it is"));
		}
	}
	for (id, f) in &c.feeds {
		if let Some(p) = &f.token_file {
			out.extend(secret_file_warning(id, "token_file", p, "its reads fail until it is"));
		}
	}
	out
}

/// What is wrong with a source's password or token file (its `key`) as `elamond check` finds it,
/// without reading it: one that isn't there, can't be read, may be read by others, or would be
/// read as settings; `then`, what follows. A warning, not a mistake: the Nix build checks the
/// settings where the file isn't.
fn secret_file_warning(id: &str, key: &str, p: &std::path::Path, then: &str) -> Vec<String> {
	use std::os::unix::fs::PermissionsExt;
	let mut out = Vec::new();
	let shown = p.display();
	match std::fs::File::open(p).and_then(|f| f.metadata()) {
		Err(e) => out.push(format!("sources.{id}.{key} {shown} can't be read ({e}): {then}")),
		Ok(m) if m.permissions().mode() & 0o077 != 0 => out.push(format!("sources.{id}.{key} {shown} may be read by others: chmod 600 it; {then}")),
		Ok(_) => {}
	}
	let dropin = p.parent().and_then(std::path::Path::file_name).is_some_and(|d| d == crate::store::DROPIN_DIR);
	if dropin && p.extension().is_some_and(|e| e == "toml") {
		out.push(format!("sources.{id}.{key} {shown} is read as settings, being a .toml in elamond.d/: keep it elsewhere"));
	}
	out
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
	/// A source: the keys of its `kind`'s record.
	Source,
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
pub const RECORDS: [&str; 21] = [
	"settings", "history", "source_nut", "source_command", "source_http", "source_file", "outlet", "link", "action", "param", "profile", "unlock", "android", "web",
	"dashboard", "item", "column", "expand", "bar", "sort", "row_action",
];

/// The kinds of source, each with its record (`source_<kind>`).
pub const SOURCE_KINDS: [&str; 4] = ["nut", "command", "http", "file"];
/// How a `command`, `http` or `file` source's output is taken.
pub const SOURCE_FORMATS: [&str; 3] = ["json", "text", "lines"];

/// The keys every `command`, `http` and `file` source takes besides its own.
fn feed_keys() -> Vec<Key> {
	vec![
		k("format", Ty::Enum("source format", &SOURCE_FORMATS)),
		k("interval", Ty::Int(1, 86_400)),
		k("timeout", Ty::Int(1, 60)),
		k("history", Ty::Bool),
		k("role", role()),
	]
}

/// `more` added to `keys`, a key in both taking either's values.
fn merge_keys(keys: &mut Vec<Key>, more: Vec<Key>) {
	for key in more {
		match keys.iter_mut().find(|k| k.name == key.name) {
			Some(have) if format!("{:?}", have.ty) != format!("{:?}", key.ty) => {
				have.ty = Ty::Any(vec![have.ty.clone(), key.ty]);
			}
			Some(_) => {}
			None => keys.push(key),
		}
	}
}

/// The keys a source of `kind` takes, or, without one of the kinds, any kind's (none required).
pub fn source_keys(kind: Option<&str>) -> Vec<Key> {
	if let Some(kind) = kind.filter(|k| SOURCE_KINDS.contains(k)) {
		return record(&format!("source_{kind}"));
	}
	let mut keys = vec![k("kind", Ty::Enum("source kind", &SOURCE_KINDS))];
	for kind in SOURCE_KINDS {
		merge_keys(&mut keys, record(&format!("source_{kind}")).into_iter().filter(|k| k.name != "kind").map(|k| Key { required: false, ..k }).collect());
	}
	keys
}

/// The keys of a record.
pub fn record(name: &str) -> Vec<Key> {
	match name {
		"settings" => vec![
			k("schema", Ty::Int(SCHEMA, SCHEMA)),
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
			k("sources", id_map(Ty::Source)),
			k("history", Ty::Rec("history")),
		],
		"history" => vec![k("keep", Ty::Any(vec![Ty::Enum("word", &["all"]), Ty::Int(30, i64::MAX)]))],
		"source_command" => [req("kind", Ty::Enum("source kind", &["command"])), req("command", list(Ty::Str))].into_iter().chain(feed_keys()).collect(),
		"source_http" => [req("kind", Ty::Enum("source kind", &["http"])), req("url", Ty::Str), k("token_file", Ty::Str), k("header", Ty::Str)]
			.into_iter()
			.chain(feed_keys())
			.collect(),
		"source_file" => [req("kind", Ty::Enum("source kind", &["file"])), req("path", Ty::Str)].into_iter().chain(feed_keys()).collect(),
		"source_nut" => vec![
			req("kind", Ty::Enum("source kind", &["nut"])),
			req("ups", Ty::Str),
			k("host", Ty::Str),
			k("port", Ty::Int(1, 65535)),
			k("capacity_wh", Ty::Num),
			k("interval_ms", Ty::Int(200, 60_000)),
			k("background_ms", Ty::Int(200, 60_000)),
			k("history", Ty::Bool),
			k("recorder", Ty::Str),
			k("user", Ty::Str),
			k("password_file", Ty::Str),
			k("commands", list(Ty::Str)),
			k("settings", list(Ty::Str)),
			k("role", Ty::Enum("role", &ACTION_ROLES)),
			k("outlets", map(Ty::Rec("outlet"))),
		],
		"outlet" => vec![k("critical", Ty::Bool)],
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
		"row_action" => vec![k("action", Ty::Str), k("open", Ty::Str), k("label", Ty::Str), k("params", map(Ty::Scalar))],
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
			k("ranges", Ty::Bool),
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
			k("sort_labels", map(Ty::Str)),
			k("search", Ty::Any(vec![Ty::Bool, list(Ty::Str)])),
			k("limit", Ty::Int(1, 10_000)),
			k("filter", map(Ty::Scalar)),
			k("hide_empty", Ty::Bool),
			k("row_open", Ty::Str),
			k("row_actions", list(Ty::Rec("row_action"))),
			k("expand", list(Ty::Rec("expand"))),
			k("collapsed", Ty::Bool),
		],
		"button" => vec![k("action", Ty::Str), k("params", map(Ty::Scalar))],
		"toggle" => action().into_iter().chain([k("on", Ty::Scalar), k("off", Ty::Scalar), k("map", map(Ty::Str))]).collect(),
		"select" | "slider" => action(),
		"text" => vec![k("text", Ty::Text), k("level", Ty::Enum("level", &LEVELS))],
		"group" => vec![k("widgets", list(Ty::Widget)), k("collapsed", Ty::Bool)],
		"timeline" => vec![
			k("source", Ty::Str),
			k("range", Ty::Int(600, 2_592_000)),
			k("ranges", Ty::Bool),
			k("legend", Ty::Enum("legend", &LEGENDS)),
		],
		"log" => vec![
			k("scope", Ty::Enum("log scope", &LOG_SCOPES)),
			k("unit", Ty::Str),
			k("level", Ty::Enum("log level", &LOG_LEVELS)),
			k("lines", Ty::Int(10, 1000)),
			k("search", Ty::Bool),
		],
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
		merge_keys(&mut keys, type_keys(t));
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
		Ty::Int(lo, i64::MAX) => format!("an integer, {lo} or more"),
		Ty::Int(lo, hi) => format!("an integer from {lo} to {hi}"),
		Ty::Num => "a number".into(),
		Ty::Scalar => "a string, a number or true or false".into(),
		Ty::Enum(what, words) => format!("a {what}: {}", words.join(", ")),
		Ty::Format => format!("a format: {}, or a pattern like \"{{:.1}}\"", format::FORMATS.join(", ")),
		Ty::List(t) => format!("a list of {}", plural_of(t)),
		Ty::Map(_) | Ty::IdMap(_) | Ty::Rec(_) => "a table".into(),
		Ty::Any(ts) => ts.iter().map(describe).collect::<Vec<_>>().join(" or "),
		Ty::Widget => "a widget (a table with a type)".into(),
		Ty::Source => "a source (a table with a kind)".into(),
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
	// Which settings these are: 3, whole; one this elamond doesn't read is said plainly.
	if let Some(schema) = top.remove("schema") {
		match schema.as_i64() {
			Some(SCHEMA) => {}
			Some(n) if n > SCHEMA => out.push(("schema".into(), format!("settings schema {n}: this elamond reads {SCHEMA} — written for a newer elamond?"))),
			Some(n) => out.push(("schema".into(), format!("settings schema {n}: this elamond reads {SCHEMA}"))),
			None => out.push(("schema".into(), format!("should be {SCHEMA}, not {}", found(&schema)))),
		}
	}
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
		Ty::Source => match v.as_object() {
			Some(o) => check_source_keys(o, path, partial, out),
			None => wrong(out),
		},
	}
}

/// A source's keys, against its kind's: without a kind (in a file that leaves it to another), or
/// of one there isn't, any kind's.
fn check_source_keys(o: &Map<String, Value>, path: &str, partial: bool, out: &mut Found) {
	match o.get("kind").and_then(Value::as_str) {
		Some(kind) if SOURCE_KINDS.contains(&kind) => check_keys(o, &source_keys(Some(kind)), path, partial, "here", out),
		Some(kind) => {
			let line = format!("no source kind \"{kind}\"; a source is of kind nut, command, http or file{}", did_you_mean(kind, SOURCE_KINDS));
			out.push((join(path, "kind"), line));
			let mut rest = o.clone();
			rest.remove("kind");
			check_keys(&rest, &source_keys(None), path, partial, "here", out);
		}
		None => {
			if !partial && !o.contains_key("kind") {
				out.push((path.into(), "kind is missing".into()));
			}
			check_keys(o, &source_keys(None), path, partial, "here", out);
		}
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
	// A default_profile that is none of the profiles is a warning, not a mistake: settings without
	// any profile are sound, if bare.
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
	for (id, s) in &c.sources {
		check_nut_source(id, s, &mut push);
	}
	for (id, f) in &c.feeds {
		check_feed(id, f, &mut push);
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

/// A source's id is the first part of its values' addresses: not one of the computer's, nor `item`.
fn check_source_id(id: &str, push: &mut impl FnMut(String, String)) {
	let path = join("sources", id);
	if values::known_prefix(id) {
		push(path, format!("\"{id}\" is the first part of this computer's own values ({id}.…): give the source another id"));
	} else if id == "item" {
		push(path, "\"item\" stands for the item of a for_each: give the source another id".into());
	}
}

/// A `command`, `http` or `file` source as the settings can tell it is sound, without running,
/// asking or reading anything.
fn check_feed(id: &str, f: &Feed, push: &mut impl FnMut(String, String)) {
	let path = join("sources", id);
	let at = |key: &str| join(&path, key);
	check_source_id(id, push);
	let absolute = |key: &str, p: &Option<PathBuf>, push: &mut dyn FnMut(String, String)| {
		if let Some(p) = p.as_ref().filter(|p| !p.is_absolute()) {
			push(at(key), format!("{} should be an absolute path", p.display()));
		}
	};
	match f.kind {
		FeedKind::Command => {
			if f.command.first().is_none_or(|p| p.trim().is_empty()) {
				push(at("command"), "is empty: give the program and its arguments".into());
			}
		}
		FeedKind::Http => {
			if let Some(Err(why)) = f.url.as_deref().map(url_shape) {
				push(at("url"), why.into());
			}
			if let Some(h) = &f.header {
				if !(1..=64).contains(&h.len()) || !h.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
					push(at("header"), format!("\"{h}\" isn't a header's name: 1 to 64 of A-Z, a-z, 0-9 and -"));
				}
				if f.token_file.is_none() {
					push(at("header"), "is the header the token goes in: give token_file too".into());
				}
			}
			absolute("token_file", &f.token_file, push);
		}
		FeedKind::File => absolute("path", &f.path, push),
	}
}

/// Whether `url` is one a source asks: `http://` or `https://`, a host, no spaces, and no user or
/// password in it. What is wrong never repeats the URL, which may hold them.
fn url_shape(url: &str) -> Result<(), &'static str> {
	let rest = url.split_once("://").map_or(url, |(_, r)| r);
	let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
	if authority.contains('@') {
		return Err("put the credentials in token_file, not in the URL");
	}
	let Some(rest) = url.strip_prefix("http://").or_else(|| url.strip_prefix("https://")) else {
		return Err("should start with http:// or https://");
	};
	if url.chars().any(char::is_whitespace) {
		return Err("has spaces in it: write them as %20");
	}
	let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
	let host = match authority.strip_prefix('[') {
		Some(v6) => v6.split_once(']').map_or("", |(h, _)| h),
		None => authority.split(':').next().unwrap_or(""),
	};
	if host.is_empty() {
		return Err("has no host: http://<host>[:port]/…");
	}
	Ok(())
}

/// A source as the settings can tell it is sound, without asking upsd anything.
fn check_nut_source(id: &str, s: &NutSource, push: &mut impl FnMut(String, String)) {
	let path = join("sources", id);
	let at = |key: &str| join(&path, key);
	check_source_id(id, push);
	if s.ups.is_empty() || s.ups.chars().count() > 64 || s.ups.chars().any(char::is_whitespace) {
		push(at("ups"), format!("\"{}\" isn't a UPS's name on upsd: 1 to 64 characters, no spaces", s.ups));
	}
	if s.host.trim().is_empty() {
		push(at("host"), "is empty: leave it out for localhost".into());
	}
	if s.capacity_wh.is_some_and(|w| w.is_nan() || w <= 0.0) {
		push(at("capacity_wh"), "should be above 0".into());
	}
	if let Some(b) = s.background_ms
		&& b < s.interval_ms
	{
		push(at("background_ms"), format!("should be interval_ms ({}) or more, not {b}", s.interval_ms));
	}
	if let Some(r) = &s.recorder
		&& let Err(e) = r.parse::<upst_core::recorder::Addr>()
	{
		push(at("recorder"), format!("\"{r}\" isn't a socket path or host[:port]: {e}"));
	}
	match (&s.user, &s.password_file) {
		(Some(_), None) | (None, Some(_)) => push(path.clone(), "user and password_file go together: give both, or neither".into()),
		_ => {}
	}
	if let Some(p) = &s.password_file
		&& !p.is_absolute()
	{
		push(at("password_file"), format!("{} should be an absolute path", p.display()));
	}
	let login = s.user.is_some() && s.password_file.is_some();
	for (key, list) in [("commands", &s.commands), ("settings", &s.settings)] {
		if !list.is_empty() && !login {
			push(at(key), format!("{key} run as a NUT user: give user and password_file too"));
		}
		for (i, p) in list.iter().enumerate() {
			if p.is_empty() || !p.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-*".contains(&b)) {
				push(format!("{}[{i}]", at(key)), format!("\"{p}\" isn't a pattern: a-z, 0-9, ., _, - and * (any run of characters)"));
			}
		}
	}
	// As `NutSource::critical` looks it up: `1`, not `01`.
	for n in s.outlets.keys() {
		if n.parse::<u32>().ok().map(|x| x.to_string()).as_ref() != Some(n) {
			push(join(&at("outlets"), n), format!("\"{n}\" isn't an outlet: its number, as in outlet.1.status"));
		}
	}
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
		"timeline" => need("source", w.source.is_some(), push),
		// A unit's journal is a link's unit's: the link says whether it is the system's or the user's.
		"log" if w.scope.as_deref() == Some("unit") => match w.unit.as_deref() {
			None => push(path.into(), "a log of scope = \"unit\" needs unit".into()),
			Some(unit) if !unit.contains('{') && !c.links.values().any(|l| l.unit.as_deref() == Some(unit)) => {
				let units = c.links.values().filter_map(|l| l.unit.as_deref());
				push(join(path, "unit"), format!("no link has the unit \"{unit}\": a log shows a link's unit{}", did_you_mean(unit, units)));
			}
			Some(_) => {}
		},
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
	match &w.source {
		// A timeline's is a source of the settings', or the item of a dashboard of each.
		Some(source) if kind == "timeline" => {
			if !source.contains('{') && !c.sources.contains_key(source) {
				let there = c.sources.keys().map(String::as_str);
				push(join(path, "source"), format!("no source \"{source}\" in [sources]{}", did_you_mean(source, there)));
			}
		}
		Some(source) => check_source(c, source, &w, &kind, path, push),
		None => {}
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
	// A station's parameter takes what the station has, which isn't known here.
	let station = c.station_param(action).is_some();
	if let Some(param) = &w.param {
		match kinds.iter().find(|(p, _)| p == param) {
			None => push(join(path, "param"), format!("no parameter \"{param}\" in {action}{}", did_you_mean(param, kinds.iter().map(|(p, _)| p.as_str())))),
			Some((_, k)) => {
				let fits = match kind {
					_ if station => true,
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
	for f in w.sort_labels.iter().flat_map(|l| l.keys()) {
		let sorted = w.sorts.iter().flatten().any(|s| s == f) || w.sort.as_ref().is_some_and(|s| &s.field == f);
		if !sorted {
			push(join(&join(path, "sort_labels"), f), format!("\"{f}\" isn't one of the table's sorts"));
		}
		named.push((f, join(&join(path, "sort_labels"), f)));
	}
	if let Some(Search::Fields(fields)) = &w.search {
		for (i, f) in fields.iter().enumerate() {
			named.push((f, format!("{}[{i}]", join(path, "search"))));
		}
	}
	for f in w.filter.iter().flat_map(|f| f.keys()) {
		named.push((f, join(&join(path, "filter"), f)));
	}
	if let Some(f) = &w.row_open {
		named.push((f, join(path, "row_open")));
	}
	for (i, ra) in w.row_actions.iter().flatten().enumerate() {
		let rpath = format!("{}[{i}]", join(path, "row_actions"));
		let action = match (&ra.action, &ra.open) {
			(Some(action), None) => action,
			// A row opens its own of a dashboard made for each of the table's rows.
			(None, Some(open)) => {
				let each = c.dashboards.get(open).map(|d| d.for_each.as_deref().and_then(values::collection_def).map(|d| d.name));
				match each {
					None => push(join(&rpath, "open"), format!("no dashboard \"{open}\"{}", did_you_mean(open, c.dashboards.keys().map(String::as_str)))),
					Some(each) if each != Some(def.name) => {
						push(join(&rpath, "open"), format!("dashboard \"{open}\" isn't made for_each = \"{source}\", as the table's rows are"))
					}
					Some(_) => {}
				}
				continue;
			}
			_ => {
				push(rpath, "give either action or open".into());
				continue;
			}
		};
		if !c.has_action(action) {
			push(join(&rpath, "action"), format!("no action \"{action}\""));
			continue;
		}
		let kinds = c.param_kinds(action).unwrap_or_default();
		for (p, v) in ra.params.iter().flatten() {
			if !kinds.iter().any(|(k, _)| k == p) {
				push(join(&join(&rpath, "params"), p), format!("no parameter \"{p}\" in {action}"));
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
		Ty::Int(lo, i64::MAX) => json!({ "type": "integer", "minimum": lo }),
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
		Ty::Source => json!({ "$ref": "#/$defs/source" }),
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
	// A source is of one kind, with that kind's keys.
	let kinds: Vec<Value> = SOURCE_KINDS.iter().map(|k| json!({ "$ref": format!("#/$defs/source_{k}") })).collect();
	defs.insert("source".into(), json!({ "anyOf": kinds }));
	let mut top = object_schema(&record("settings"));
	top["$schema"] = "https://json-schema.org/draft/2020-12/schema".into();
	top["title"] = format!("elamond settings (schema {SCHEMA})").into();
	top["$defs"] = Value::Object(defs);
	top
}

#[cfg(test)]
mod tests {
	use super::*;

	fn read(files: &[(&str, &str)]) -> Result<Config, Vec<Mistake>> {
		read_sources(&files.iter().map(|(n, t)| Source::new(*n, *t)).collect::<Vec<_>>())
	}

	/// `files` as drop-ins over the defaults, which what they refer to is in.
	fn over<'a>(files: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
		let mut all = vec![(DEFAULTS_NAME, DEFAULTS)];
		all.extend_from_slice(files);
		all
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
		// A whole file, of schema 3: nothing is read under it.
		assert!(!fixture.dashboards.contains_key("overview"));
	}

	#[test]
	fn tables_merge_key_by_key_and_lists_replace() {
		let c = read(&over(&[
			("a.toml", "listen = [\"eth0\"]\n[dashboards.overview]\ntitle = \"Home\"\n[profiles.default]\ntheme = \"dark\"\n"),
			("b.toml", "[[dashboards.overview.widgets]]\ntype = \"text\"\ntext = \"Hello\"\n[profiles.mine]\nuse = \"default\"\nname = \"Mine\"\n"),
		]));
		let c = c.unwrap();
		let o = &c.dashboards["overview"];
		assert_eq!((o.title.as_str(), o.icon.as_deref(), o.widgets.len()), ("Home", Some("overview"), 1));
		let p = c.profile("mine");
		assert_eq!((p.name.as_str(), p.theme.as_str(), p.dashboards.len()), ("Mine", "dark", 5));
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
	fn the_hourly_history_is_kept_for_good_or_30_days_and_more() {
		let keep = |text: &str| read(&[("a.toml", text)]).unwrap().history.keep;
		assert_eq!(keep(""), None);
		assert_eq!(keep("[history]\nkeep = \"all\"\n"), None);
		assert_eq!(keep("[history]\nkeep = 30\n"), Some(30));
		assert_eq!(keep("[history]\nkeep = 3650\n"), Some(3650));
		assert_eq!(mistakes(&[("a.toml", "[history]\nkeep = 29\n")]), "a.toml:2: history.keep: should be a word: all or an integer, 30 or more, not 29");
		assert!(mistakes(&[("a.toml", "[history]\nkeep = \"forever\"\n")]).contains("a.toml:2: history.keep: no word \"forever\""));
		assert!(mistakes(&[("a.toml", "[history]\nkeep = 30.5\n")]).contains("history.keep: should be"));
		let e = mistakes(&[("a.toml", "[history]\nkeeps = 30\n")]);
		assert!(e.contains("no key \"keeps\" here; did you mean keep?"), "{e}");
		// As a drop-in over the defaults, too.
		assert_eq!(read(&over(&[("a.toml", "[history]\nkeep = 400\n")])).unwrap().history.keep, Some(400));
	}

	#[test]
	fn a_ups_source_takes_every_key_and_is_checked_without_upsd() {
		// Every key a source takes, as the fixture has them.
		let c = read(&[("ui-ups-source.toml", include_str!("../fixtures/0.7/ui-ups-source.toml"))]).unwrap();
		let s = &c.sources["eco"];
		assert_eq!((s.ups.as_str(), s.host.as_str(), s.port, s.capacity_wh, s.interval_ms, s.background()), ("ups", "localhost", 3493, Some(2048.0), 1000, 10_000));
		assert_eq!((s.recorder.as_deref(), s.user.as_deref(), s.role), (Some("/run/upst/upst.sock"), Some("elamon"), Role::Admin));
		assert_eq!(s.password_file.as_deref(), Some(std::path::Path::new("/etc/elamon/nut.password")));
		assert_eq!((s.commands.len(), s.settings.len()), (2, 2));
		assert!(s.history && s.critical(1) && !s.critical(2));
		// What a source leaves out.
		let bare = read(&[("a.toml", "[sources.ups]\nkind = \"nut\"\nups = \"ups\"\n")]).unwrap();
		let s = &bare.sources["ups"];
		assert_eq!((s.host.as_str(), s.port, s.interval_ms, s.background(), s.history, s.role), ("localhost", 3493, 1000, 10_000, true, Role::Admin));
		assert!(s.commands.is_empty() && s.settings.is_empty() && s.user.is_none() && s.recorder.is_none() && s.capacity_wh.is_none());
		// Asked less often than shown, never more.
		let slow = read(&[("a.toml", "[sources.ups]\nkind = \"nut\"\nups = \"ups\"\ninterval_ms = 30000\n")]).unwrap();
		assert_eq!(slow.sources["ups"].background(), 30_000);
		// Keys and their kinds.
		let e = mistakes(&[("a.toml", "[sources.a]\nkind = \"nut\"\nups = \"u\"\nport = 0\ninterval_ms = 100\noutlets.\"1\".power = true\nlable = 1\n")]);
		for want in [
			"a.toml:4: sources.a.port: should be an integer from 1 to 65535, not 0",
			"a.toml:5: sources.a.interval_ms: should be an integer from 200 to 60000, not 100",
			"a.toml:6: sources.a.outlets.1.power: no key \"power\" here",
			"a.toml:7: sources.a.lable: no key \"lable\" here",
		] {
			assert!(e.contains(want), "missing: {want}\nin:\n{e}");
		}
		assert!(mistakes(&[("a.toml", "[sources.a]\nkind = \"nut\"\n")]).contains("sources.a: ups is missing"));
		// What the settings can tell without asking upsd.
		let e = mistakes(&[(
			"a.toml",
			r#"
[sources.cpu]
kind = "nut"
ups = "ups"

[sources.item]
kind = "nut"
ups = "ups"

[sources.bad]
kind = "nut"
ups = "my ups"
capacity_wh = 0
interval_ms = 2000
background_ms = 1000
recorder = ""
user = "elamon"
commands = ["Beeper.*", ""]
outlets.a.critical = true
outlets."01".critical = true

[sources.half]
kind = "nut"
ups = "ups"
password_file = "nut.password"
settings = ["battery.charge.low"]
"#,
		)]);
		for want in [
			"a.toml:2: sources.cpu: \"cpu\" is the first part of this computer's own values (cpu.…): give the source another id",
			"sources.item: \"item\" stands for the item of a for_each",
			"sources.bad.ups: \"my ups\" isn't a UPS's name on upsd: 1 to 64 characters, no spaces",
			"sources.bad.capacity_wh: should be above 0",
			"sources.bad.background_ms: should be interval_ms (2000) or more, not 1000",
			"sources.bad.recorder: \"\" isn't a socket path or host[:port]: empty address",
			"sources.bad: user and password_file go together: give both, or neither",
			"sources.bad.commands: commands run as a NUT user: give user and password_file too",
			"sources.bad.commands[0]: \"Beeper.*\" isn't a pattern",
			"sources.bad.commands[1]: \"\" isn't a pattern",
			"sources.bad.outlets.a: \"a\" isn't an outlet",
			"sources.bad.outlets.01: \"01\" isn't an outlet: its number, as in outlet.1.status",
			"sources.half: user and password_file go together",
			"sources.half.password_file: nut.password should be an absolute path",
			"sources.half.settings: settings run as a NUT user",
		] {
			assert!(e.contains(want), "missing: {want}\nin:\n{e}");
		}
		// A timeline shows a source of the settings, or a dashboard's item.
		let timeline = |src: &str| {
			read(&[("a.toml", format!("[sources.eco]\nkind = \"nut\"\nups = \"ups\"\n[dashboards.d]\ntitle = \"D\"\nfor_each = \"ups\"\n[[dashboards.d.widgets]]\ntype = \"timeline\"\n{src}").as_str())])
		};
		assert!(timeline("source = \"eco\"\nrange = 600\n").is_ok() && timeline("source = \"{item}\"\n").is_ok());
		let no = |src: &str| join_mistakes(&timeline(src).unwrap_err());
		assert!(no("source = \"ecco\"\n").contains("dashboards.d.widgets[0].source: no source \"ecco\" in [sources]; did you mean eco?"), "{}", no("source = \"ecco\"\n"));
		assert!(no("").contains("a timeline widget needs source"));
		assert!(no("source = \"eco\"\nrange = 300\n").contains("range: should be an integer from 600 to 2592000"));
		// Its legend: one of three words, and on a timeline alone.
		for l in LEGENDS {
			assert!(timeline(&format!("source = \"eco\"\nlegend = \"{l}\"\n")).is_ok(), "{l}");
		}
		let percents = no("source = \"eco\"\nlegend = \"percents\"\n");
		assert!(percents.contains("dashboards.d.widgets[0].legend: no legend \"percents\"; did you mean percent?"), "{percents}");
		let hours = no("source = \"eco\"\nlegend = \"hours\"\n");
		assert!(hours.contains("dashboards.d.widgets[0].legend: no legend \"hours\""), "{hours}");
		let on_a_graph = join_mistakes(
			&read(&[("a.toml", "[dashboards.d]\ntitle = \"D\"\n[[dashboards.d.widgets]]\ntype = \"graph\"\nvalues = [\"cpu.usage\"]\nlegend = \"both\"\n")]).unwrap_err(),
		);
		assert!(on_a_graph.contains("dashboards.d.widgets[0].legend"), "{on_a_graph}");
	}

	#[test]
	fn a_station_s_actions_are_named_but_what_the_station_has_isnt_checked() {
		// As `elamond defaults dashboards.ups` gives them, `{item}` written out.
		let with = |widgets: &str| {
			read(&[("a.toml", format!("[sources.eco]\nkind = \"nut\"\nups = \"ups\"\n[dashboards.d]\ntitle = \"D\"\n[[dashboards.d.widgets]]\n{widgets}").as_str())])
		};
		for w in [
			"type = \"toggle\"\nvalue = \"eco.ups.beeper.status\"\naction = \"nut.command.eco\"\nparam = \"cmd\"\non = \"beeper.enable\"\noff = \"beeper.disable\"\n",
			"type = \"slider\"\nvalue = \"eco.experimental.battery.charge.max\"\naction = \"nut.set.eco.experimental.battery.charge.max\"\nparam = \"value\"\n",
			"type = \"select\"\nvalue = \"eco.experimental.output.xboost\"\naction = \"nut.set.eco.experimental.output.xboost\"\nparam = \"value\"\n",
			"type = \"button\"\naction = \"nut.command.eco\"\nparams = { cmd = \"beeper.enable\" }\n",
		] {
			assert!(with(w).is_ok(), "{w}: {}", join_mistakes(&with(w).unwrap_err()));
		}
		let no = |w: &str| join_mistakes(&with(w).unwrap_err());
		assert!(no("type = \"button\"\naction = \"nut.command.ecco\"\n").contains("no action \"nut.command.ecco\""));
		assert!(no("type = \"slider\"\nvalue = \"eco.x\"\naction = \"nut.set.eco.x\"\nparam = \"cmd\"\n").contains("no parameter \"cmd\" in nut.set.eco.x"));
		assert!(no("type = \"button\"\naction = \"nut.command.eco\"\nparams = { value = 1 }\n").contains("no parameter \"value\" in nut.command.eco"));
	}

	#[test]
	fn a_command_a_web_service_and_a_file_are_sources_of_their_own() {
		// Every key, as the fixture has them (its sources: the journal it shows is S2's).
		let text = include_str!("../fixtures/0.8/ui-sources.toml");
		let c = read(&[("ui-sources.toml", text)]).or_else(|_| read(&[("ui-sources.toml", text.split("[profiles.owner]").next().unwrap())])).unwrap();
		assert!(c.sources.is_empty());
		assert_eq!(c.feeds.keys().collect::<Vec<_>>(), ["app", "backup", "zone"]);
		let backup = &c.feeds["backup"];
		assert_eq!((backup.kind, backup.command.join(" "), backup.format), (FeedKind::Command, "backup-status --short".to_string(), FeedFormat::Lines));
		assert_eq!((backup.interval, backup.timeout, backup.history, backup.role), (300, 30, false, Role::Monitor));
		let app = &c.feeds["app"];
		assert_eq!((app.kind, app.url.as_deref(), app.header.as_deref(), app.format), (FeedKind::Http, Some("http://localhost:8080/status"), Some("X-Api-Key"), FeedFormat::Json));
		assert_eq!(app.token_file.as_deref(), Some(std::path::Path::new("/etc/elamon/app.token")));
		let zone = &c.feeds["zone"];
		assert_eq!((zone.kind, zone.path.as_deref(), zone.format), (FeedKind::File, Some(std::path::Path::new("/sys/class/thermal/thermal_zone0/temp")), FeedFormat::Text));
		assert_eq!((zone.interval, zone.timeout, zone.history, zone.role), (5, 1, true, Role::Glance));
		// What a source leaves out.
		let bare = read(&[("a.toml", "[sources.up]\nkind = \"command\"\ncommand = [\"uptime\"]\n")]).unwrap();
		let up = &bare.feeds["up"];
		assert_eq!((up.format, up.interval, up.timeout, up.history, up.role), (FeedFormat::Json, 10, 5, false, Role::Monitor));
		assert!(up.url.is_none() && up.token_file.is_none() && up.header.is_none() && up.path.is_none());
		// Beside a UPS, which stays where it was.
		let both = read(&[("a.toml", "[sources.eco]\nkind = \"nut\"\nups = \"ups\"\n[sources.up]\nkind = \"file\"\npath = \"/proc/uptime\"\n")]).unwrap();
		assert!(both.sources.keys().eq(["eco"]) && both.feeds.keys().eq(["up"]));
	}

	#[test]
	fn each_kind_takes_its_own_keys() {
		let e = mistakes(&[(
			"a.toml",
			r#"[sources.a]
kind = "command"
command = ["x"]
url = "http://localhost"

[sources.b]
kind = "http"
url = "http://localhost"
ups = "u"
interval = 0
timeout = 61
format = "xml"
role = "root"

[sources.c]
kind = "file"

[sources.d]
kind = "htp"
url = "http://localhost"
lable = 1

[sources.e]
kind = "nut"
ups = "u"
path = "/x"

[sources.f]
kind = "http"
interval = 86401
"#,
		)]);
		for want in [
			"a.toml:4: sources.a.url: no key \"url\" here",
			"a.toml:9: sources.b.ups: no key \"ups\" here",
			"a.toml:10: sources.b.interval: should be an integer from 1 to 86400, not 0",
			"a.toml:11: sources.b.timeout: should be an integer from 1 to 60, not 61",
			"a.toml:12: sources.b.format: no source format \"xml\"",
			"a.toml:13: sources.b.role: no role \"root\"",
			"a.toml:19: sources.d.kind: no source kind \"htp\"; a source is of kind nut, command, http or file; did you mean http?",
			"a.toml:21: sources.d.lable: no key \"lable\" here",
			"a.toml:26: sources.e.path: no key \"path\" here",
			"sources.f.interval: should be an integer from 1 to 86400, not 86401",
		] {
			assert!(e.contains(want), "missing: {want}\nin:\n{e}");
		}
		// What each kind needs, of the settings as merged.
		let e = mistakes(&[("a.toml", "[sources.c]\nkind = \"file\"\n[sources.f]\nkind = \"http\"\n[sources.g]\nkind = \"command\"\n")]);
		assert!(e.contains("sources.c: path is missing") && e.contains("sources.f: url is missing") && e.contains("sources.g: command is missing"), "{e}");
		// What every one of the three takes, a UPS doesn't.
		assert!(mistakes(&[("a.toml", "[sources.e]\nkind = \"nut\"\nups = \"u\"\ninterval = 5\n")]).contains("sources.e.interval: no key \"interval\" here"));
		assert!(mistakes(&[("a.toml", "[sources.e]\ncommand = [\"x\"]\n")]).contains("sources.e: kind is missing"));
		// A file that leaves the kind to another takes any kind's keys; the settings as merged, only
		// the kind's.
		let base = ("elamond.toml", "[sources.app]\nkind = \"http\"\nurl = \"http://localhost:8080/status\"\n");
		let c = read(&[base, ("elamond.d/app.toml", "[sources.app]\ninterval = 30\ntoken_file = \"/etc/elamon/app.token\"\n")]).unwrap();
		assert_eq!((c.feeds["app"].interval, c.feeds["app"].token_file.is_some()), (30, true));
		let e = mistakes(&[base, ("elamond.d/app.toml", "[sources.app]\npath = \"/x\"\n")]);
		assert!(e.contains("sources.app.path: no key \"path\" here"), "{e}");
		let e = mistakes(&[base, ("elamond.d/app.toml", "[sources.app]\npaht = \"/x\"\n")]);
		assert!(e.contains("elamond.d/app.toml:2: sources.app.paht: no key \"paht\" here; did you mean path?"), "{e}");
	}

	#[test]
	fn a_source_is_checked_without_running_asking_or_reading_it() {
		let e = mistakes(&[(
			"a.toml",
			r#"
[sources.cpu]
kind = "command"
command = ["true"]

[sources.item]
kind = "file"
path = "/x"

[sources.none]
kind = "command"
command = []

[sources.blank]
kind = "command"
command = ["", "x"]

[sources.ftp]
kind = "http"
url = "ftp://example.net/status"

[sources.creds]
kind = "http"
url = "https://user:secret-pass@example.net/status"

[sources.hostless]
kind = "http"
url = "http:///status"

[sources.spaced]
kind = "http"
url = "http://example.net/a status"

[sources.header]
kind = "http"
url = "http://localhost:8080/status"
header = "X Api:Key"

[sources.relative]
kind = "http"
url = "http://[::1]:8080/status"
token_file = "app.token"

[sources.where]
kind = "file"
path = "sys/temp"
"#,
		)]);
		for want in [
			"a.toml:2: sources.cpu: \"cpu\" is the first part of this computer's own values (cpu.…): give the source another id",
			"sources.item: \"item\" stands for the item of a for_each",
			"a.toml:12: sources.none.command: is empty: give the program and its arguments",
			"sources.blank.command: is empty: give the program and its arguments",
			"sources.ftp.url: should start with http:// or https://",
			"sources.creds.url: put the credentials in token_file, not in the URL",
			"sources.hostless.url: has no host",
			"sources.spaced.url: has spaces in it",
			"sources.header.header: \"X Api:Key\" isn't a header's name: 1 to 64 of A-Z, a-z, 0-9 and -",
			"sources.header.header: is the header the token goes in: give token_file too",
			"sources.relative.token_file: app.token should be an absolute path",
			"sources.where.path: sys/temp should be an absolute path",
		] {
			assert!(e.contains(want), "missing: {want}\nin:\n{e}");
		}
		// The credentials go nowhere, the mistake included; a sound URL is no mistake.
		assert!(!e.contains("secret-pass") && !e.contains("relative.url"), "{e}");
		for good in ["http://localhost", "https://example.net:8443/a?b=c#d", "http://192.0.2.1/status", "http://[::1]/"] {
			assert_eq!(url_shape(good), Ok(()), "{good}");
		}
		assert!(url_shape("user:secret-pass@example.net").is_err_and(|e| e.contains("credentials")));
		// A timeout longer than the interval makes a slow source, not a mistake.
		read(&[("a.toml", "[sources.slow]\nkind = \"command\"\ncommand = [\"x\"]\ninterval = 1\ntimeout = 60\n")]).unwrap();
	}

	#[test]
	fn a_token_file_is_warned_of_but_never_read() {
		use std::os::unix::fs::PermissionsExt;
		let dir = std::env::temp_dir().join(format!("elamond-token-{}", data_encoding::HEXLOWER.encode(&crate::store::random_bytes::<6>())));
		let dropins = dir.join(crate::store::DROPIN_DIR);
		std::fs::create_dir_all(&dropins).unwrap();
		let file = |path: &std::path::Path, mode: u32| {
			std::fs::write(path, "marker-token\n").unwrap();
			std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
			path.display().to_string()
		};
		let warned = |path: String| {
			let c = read(&[("a.toml", format!("[sources.app]\nkind = \"http\"\nurl = \"http://localhost:8080\"\ntoken_file = \"{path}\"\n").as_str())]).unwrap();
			let w: Vec<String> = warnings(&c).into_iter().filter(|w| w.starts_with("sources.")).collect();
			assert!(w.iter().all(|w| !w.contains("marker-token")), "{w:?}");
			w
		};
		assert!(warned(file(&dir.join("private"), 0o600)).is_empty());
		let open = warned(file(&dir.join("open"), 0o640));
		assert!(open[0].starts_with("sources.app.token_file ") && open[0].ends_with("/open may be read by others: chmod 600 it; its reads fail until it is"), "{open:?}");
		let gone = warned(dir.join("gone").display().to_string());
		assert!(gone[0].contains("/gone can't be read (") && gone[0].ends_with("its reads fail until it is"), "{gone:?}");
		let settings = warned(file(&dropins.join("app.toml"), 0o600));
		assert!(settings[0].ends_with("is read as settings, being a .toml in elamond.d/: keep it elsewhere"), "{settings:?}");
		// An empty one isn't read, so isn't warned of.
		std::fs::write(dir.join("private"), "").unwrap();
		assert!(warned(dir.join("private").display().to_string()).is_empty());
		std::fs::remove_dir_all(&dir).unwrap();
	}

	#[test]
	fn a_password_file_is_warned_of_but_never_read() {
		use std::os::unix::fs::PermissionsExt;
		let dir = std::env::temp_dir().join(format!("elamond-password-{}", data_encoding::HEXLOWER.encode(&crate::store::random_bytes::<6>())));
		let dropins = dir.join(crate::store::DROPIN_DIR);
		std::fs::create_dir_all(&dropins).unwrap();
		let file = |path: &std::path::Path, mode: u32| {
			std::fs::write(path, "marker-password\n").unwrap();
			std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
			path.display().to_string()
		};
		let warned = |path: String| {
			let c = read(&[("a.toml", format!("[sources.eco]\nkind = \"nut\"\nups = \"ups\"\nuser = \"elamon\"\npassword_file = \"{path}\"\n").as_str())]).unwrap();
			let w: Vec<String> = warnings(&c).into_iter().filter(|w| w.starts_with("sources.")).collect();
			assert!(w.iter().all(|w| !w.contains("marker-password")), "{w:?}");
			w
		};
		assert!(warned(file(&dir.join("private"), 0o600)).is_empty());
		let open = warned(file(&dir.join("open"), 0o644));
		assert!(open[0].ends_with("/open may be read by others: chmod 600 it; its station's commands and settings fail until it is"), "{open:?}");
		let gone = warned(dir.join("gone").display().to_string());
		assert!(gone[0].starts_with("sources.eco.password_file ") && gone[0].contains("/gone can't be read ("), "{gone:?}");
		let settings = warned(file(&dropins.join("nut.toml"), 0o600));
		assert!(settings[0].ends_with("is read as settings, being a .toml in elamond.d/: keep it elsewhere"), "{settings:?}");
		std::fs::remove_dir_all(&dir).unwrap();
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
		let e = mistakes(&over(&[(
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
		)]));
		for want in [
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

	/// A `log`'s unit is a link's; a row action runs an action or opens a dashboard made for each of
	/// the table's rows.
	#[test]
	fn a_log_and_a_row_that_opens_a_dashboard_are_checked() {
		let links = "[links.web]\nname = \"Web\"\nurl = \"http://{host}:8080\"\nunit = \"web.service\"\n";
		let log = |keys: &str| format!("{links}[dashboards.d]\ntitle = \"D\"\n[[dashboards.d.widgets]]\ntype = \"log\"\n{keys}\n");
		read(&over(&[("a.toml", &log("scope = \"unit\"\nunit = \"web.service\"\nlevel = \"debug\"\nlines = 1000\nsearch = false"))])).unwrap();
		read(&over(&[("a.toml", &log("scope = \"user\""))])).unwrap();
		for (keys, want) in [
			("scope = \"unit\"", "dashboards.d.widgets[0]: a log of scope = \"unit\" needs unit"),
			("scope = \"unit\"\nunit = \"web.servce\"", "dashboards.d.widgets[0].unit: no link has the unit \"web.servce\": a log shows a link's unit; did you mean web.service?"),
			("scope = \"units\"", "no log scope \"units\"; did you mean unit?"),
			("level = \"loud\"", "no log level \"loud\""),
			("lines = 5", "dashboards.d.widgets[0].lines: should be"),
			("search = [\"text\"]", "dashboards.d.widgets[0].search: should be"),
			("value = \"cpu.usage\"", "no key \"value\" in a log widget"),
		] {
			let e = mistakes(&over(&[("a.toml", &log(keys))]));
			assert!(e.contains(want), "{keys}: missing {want}\nin:\n{e}");
		}
		// A unit's journal for each link, as the starter's.
		let each = "[dashboards.e]\ntitle = \"{item.name}\"\nfor_each = \"links\"\n[[dashboards.e.widgets]]\ntype = \"log\"\nscope = \"unit\"\nunit = \"{item.unit}\"\n";
		read(&over(&[("a.toml", each)])).unwrap();
		let table = |actions: &str| format!("{each}[dashboards.t]\ntitle = \"T\"\n[[dashboards.t.widgets]]\ntype = \"table\"\nsource = \"links\"\ncolumns = [{{ field = \"name\" }}]\nrow_actions = [{actions}]\n");
		read(&over(&[("a.toml", &table("{ open = \"e\", label = \"Journal\" }, { action = \"unit.restart\", params = { link = \"{id}\" } }"))])).unwrap();
		for (actions, want) in [
			("{ open = \"overview\" }", "dashboards.t.widgets[0].row_actions[0].open: dashboard \"overview\" isn't made for_each = \"links\", as the table's rows are"),
			("{ open = \"gpu\" }", "dashboards.t.widgets[0].row_actions[0].open: dashboard \"gpu\" isn't made for_each = \"links\""),
			("{ open = \"ee\" }", "dashboards.t.widgets[0].row_actions[0].open: no dashboard \"ee\"; did you mean e?"),
			("{ open = \"e\", action = \"unit.restart\" }", "dashboards.t.widgets[0].row_actions[0]: give either action or open"),
			("{ label = \"Nothing\" }", "dashboards.t.widgets[0].row_actions[0]: give either action or open"),
		] {
			let e = mistakes(&over(&[("a.toml", &table(actions))]));
			assert!(e.contains(want), "{actions}: missing {want}\nin:\n{e}");
		}
	}

	#[test]
	fn the_schema_says_which_settings_and_bare_ones_are_warned_of() {
		// A schema this elamond doesn't read, in the main file or a drop-in.
		let e = mistakes(&[("elamond.toml", "schema = 4\ncontrol = true\n")]);
		assert_eq!(e, "elamond.toml:1: schema: settings schema 4: this elamond reads 3 — written for a newer elamond?");
		let e = mistakes(&[("elamond.toml", "schema = 3\n"), ("elamond.d/a.toml", "schema = 2\n")]);
		assert!(e.starts_with("elamond.d/a.toml:1: schema: settings schema 2: this elamond reads 3"), "{e}");
		let e = mistakes(&[("elamond.toml", "schema = \"3\"\n")]);
		assert!(e.contains("schema: should be 3, not"), "{e}");
		// A drop-in may say 3 too.
		read(&[("elamond.toml", "schema = 3\n"), ("elamond.d/a.toml", "schema = 3\n")]).unwrap();
		// An empty file of schema 3 is sound, if bare: warned of, not refused.
		let bare = read(&[("elamond.toml", "schema = 3\n")]).unwrap();
		let w = warnings(&bare);
		assert!(w[0].starts_with("there are no dashboards") && w[1].starts_with("default_profile \"default\" is none of the profiles (there are no profiles)"), "{w:?}");
		let typo = read(&over(&[("elamond.toml", "default_profile = \"defualt\"\n")])).unwrap();
		assert!(warnings(&typo)[0].contains("(the profiles are default); did you mean default?"), "{:?}", warnings(&typo));
		assert!(warnings(&Config::default()).is_empty());
		let alone = read_sources(&[Source::new("elamond.d/a.toml", "[profiles.mine]\n")]).unwrap();
		assert!(alone.dashboards.is_empty());
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
		// The schema these settings are (G39).
		assert_eq!(schema["title"], format!("elamond settings (schema {SCHEMA})"));
		// The sources of 0.8's fixture, of each kind, and one a drop-in leaves the kind of to another.
		let sources = include_str!("../fixtures/0.8/ui-sources.toml").split("[profiles.owner]").next().unwrap();
		let partial = "[sources.app]\ninterval = 30\n";
		for text in [DEFAULTS, include_str!("../fixtures/ui-source.toml"), include_str!("../fixtures/0.7/ui-ups-source.toml"), sources, partial] {
			assert!(valid(&schema, &as_json(text), &schema), "{text}");
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
			"[sources.a]\nkind = \"ftp\"\n",
			"[sources.a]\nkind = \"http\"\nups = \"u\"\n",
			"[sources.a]\nkind = \"nut\"\ninterval = 5\n",
			"[sources.a]\nkind = \"file\"\nformat = \"xml\"\n",
		] {
			assert!(!valid(&schema, &as_json(bad), &schema), "{bad}");
		}
		assert!(valid(&schema, &as_json("[[dashboards.a.widgets]]\ntype = \"graph\"\nlabels = [\"a\"]\nicon = \"{item.icon}\"\n"), &schema));
	}

	/// docs/config.md names everything the settings take, so the reference can't fall behind them.
	#[test]
	fn the_reference_names_every_key_type_action_and_value() {
		// Read where the repository has it; a build of the service alone (Nix builds elamond/ by
		// itself) has no docs to check.
		let Ok(doc) = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/config.md")) else {
			eprintln!("docs/config.md isn't beside elamond/ here; not checked");
			return;
		};
		let doc = doc.as_str();
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
		for r in ["item", "column", "expand", "bar", "sort", "row_action", "link", "action", "param", "dashboard", "source_nut", "source_command", "source_http", "source_file"] {
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
