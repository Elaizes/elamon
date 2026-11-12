//! Every reading by its address (docs/config.md, Values): `cpu.usage`, `gpu.nvidia.temp`,
//! `disk.root.percent`…, each with its unit, format and limits, and the collections (`processes`,
//! `temps`, `disks`…) that tables and `for_each` go through. Built from one reading of the
//! computer; a `View` shows them to one client: as far as its role sees, in its profile's units.

use std::collections::{BTreeMap, HashMap};

use crate::access::Role;
use crate::activity::Entry;
use crate::control::Media;
use crate::format::{self, Shown};
use crate::model::{Info, Process, Sample};
use crate::settings;

// ---- what there is --------------------------------------------------------------------------

/// An address the service may have: its pattern (`<key>` a thing's key), unit, format and the least
/// role that sees it.
#[derive(Clone, Copy, Debug)]
pub struct AddressDef {
	pub pattern: &'static str,
	pub unit: &'static str,
	pub format: &'static str,
	pub role: Role,
}

const fn a(pattern: &'static str, unit: &'static str, format: &'static str) -> AddressDef {
	AddressDef { pattern, unit, format, role: Role::Glance }
}

const fn m(pattern: &'static str, unit: &'static str, format: &'static str) -> AddressDef {
	AddressDef { pattern, unit, format, role: Role::Monitor }
}

/// Every address, as docs/config.md lists them.
pub const ADDRESSES: &[AddressDef] = &[
	a("cpu.usage", "%", "percent"),
	a("cpu.temp", "°C", "temp"),
	a("cpu.freq", "MHz", "mhz"),
	a("cpu.freq_max", "MHz", "mhz"),
	a("cpu.load1", "", "load"),
	a("cpu.load5", "", "load"),
	a("cpu.load15", "", "load"),
	a("cpu.model", "", "text"),
	a("cpu.cores", "", "integer"),
	a("cpu.threads", "", "integer"),
	a("mem.used", "B", "bytes"),
	a("mem.available", "B", "bytes"),
	a("mem.total", "B", "bytes"),
	a("mem.percent", "%", "percent"),
	a("swap.used", "B", "bytes"),
	a("swap.total", "B", "bytes"),
	a("swap.percent", "%", "percent"),
	a("gpu.<key>.usage", "%", "percent"),
	a("gpu.<key>.mem_percent", "%", "percent"),
	a("gpu.<key>.temp", "°C", "temp"),
	a("gpu.<key>.power", "W", "watts"),
	a("gpu.<key>.mem_used", "B", "bytes"),
	a("gpu.<key>.mem_total", "B", "bytes"),
	a("gpu.<key>.freq", "MHz", "mhz"),
	a("gpu.<key>.freq_max", "MHz", "mhz"),
	a("gpu.<key>.mem_freq", "MHz", "mhz"),
	a("gpu.<key>.fan", "%", "percent"),
	a("gpu.<key>.state", "", "text"),
	a("gpu.<key>.name", "", "text"),
	a("gpu.<key>.short_name", "", "text"),
	a("gpu.<key>.vendor", "", "text"),
	a("gpu.<key>.driver", "", "text"),
	a("gpu.<key>.summary", "", "text"),
	a("temp.<key>", "°C", "temp"),
	a("temp.<key>.high", "°C", "temp"),
	a("temp.<key>.crit", "°C", "temp"),
	a("fan.<key>", "RPM", "rpm"),
	a("fans.summary", "", "text"),
	a("disk.<key>.used", "B", "bytes"),
	a("disk.<key>.available", "B", "bytes"),
	a("disk.<key>.total", "B", "bytes"),
	a("disk.<key>.percent", "%", "percent"),
	a("disk.read", "B/s", "rate"),
	a("disk.write", "B/s", "rate"),
	a("disk_io.<key>.read", "B/s", "rate"),
	a("disk_io.<key>.write", "B/s", "rate"),
	a("net.rx", "B/s", "rate"),
	a("net.tx", "B/s", "rate"),
	a("net.<key>.rx", "B/s", "rate"),
	a("net.<key>.tx", "B/s", "rate"),
	a("net.<key>.rx_total", "B", "bytes"),
	a("net.<key>.tx_total", "B", "bytes"),
	a("net.<key>.up", "", "bool"),
	a("net.<key>.speed", "Mbit/s", "integer"),
	a("net.up_names", "", "text"),
	a("battery.percent", "%", "percent"),
	a("battery.health", "%", "percent"),
	a("battery.power", "W", "watts"),
	a("battery.energy_now", "Wh", "wh"),
	a("battery.energy_full", "Wh", "wh"),
	a("battery.energy_design", "Wh", "wh"),
	a("battery.time_left", "s", "duration"),
	a("battery.status", "", "text"),
	a("battery.status_text", "", "text"),
	a("battery.time_text", "", "text"),
	a("battery.cycles", "", "integer"),
	a("battery.model", "", "text"),
	a("power.on_ac", "", "bool"),
	a("power.profile", "", "text"),
	a("power.platform_profile", "", "text"),
	a("power.governor", "", "text"),
	a("power.epp", "", "text"),
	m("volume.level", "%", "percent"),
	m("volume.muted", "", "bool"),
	m("link.<key>.up", "", "bool"),
	m("link.<key>.unit_state", "", "text"),
	a("system.name", "", "text"),
	a("system.os", "", "text"),
	a("system.kernel", "", "text"),
	a("system.arch", "", "text"),
	a("system.version", "", "text"),
	a("system.uptime", "s", "duration"),
	a("system.boot_time", "s", "time"),
	m("displays.summary", "", "text"),
];

/// A field of a collection's items: its name, unit and format.
pub type FieldDef = (&'static str, &'static str, &'static str);

/// A collection: its name, the least role that sees it, its fields, and the field its rows'
/// level is judged by.
#[derive(Clone, Copy, Debug)]
pub struct CollectionDef {
	pub name: &'static str,
	pub role: Role,
	pub fields: &'static [FieldDef],
	pub level_field: Option<&'static str>,
}

/// Every collection, as docs/config.md lists them. Each item also has `key`.
pub const COLLECTIONS: &[CollectionDef] = &[
	CollectionDef {
		name: "processes",
		role: Role::Monitor,
		fields: &[
			("pid", "", "integer"),
			("ppid", "", "integer"),
			("name", "", "text"),
			("cmd", "", "text"),
			("user", "", "text"),
			("cpu", "%", "percent"),
			("mem", "B", "bytes"),
			("status", "", "text"),
			("start", "s", "time"),
			("read", "B/s", "rate"),
			("write", "B/s", "rate"),
			("io", "B/s", "rate"),
			("gpu", "%", "percent"),
			("gpu_mem", "B", "bytes"),
		],
		level_field: Some("cpu"),
	},
	CollectionDef { name: "cores", role: Role::Glance, fields: &[("usage", "%", "percent"), ("freq", "MHz", "mhz")], level_field: Some("usage") },
	CollectionDef {
		name: "gpus",
		role: Role::Glance,
		fields: &[
			("name", "", "text"),
			("short_name", "", "text"),
			("vendor", "", "text"),
			("driver", "", "text"),
			("state", "", "text"),
			("summary", "", "text"),
			("usage", "%", "percent"),
			("temp", "°C", "temp"),
			("power", "W", "watts"),
			("mem_used", "B", "bytes"),
			("mem_total", "B", "bytes"),
			("freq", "MHz", "mhz"),
			("freq_max", "MHz", "mhz"),
			("fan", "%", "percent"),
		],
		level_field: Some("usage"),
	},
	CollectionDef {
		name: "temps",
		role: Role::Glance,
		fields: &[
			("chip", "", "text"),
			("kind", "", "text"),
			("device", "", "text"),
			("label", "", "text"),
			("value", "°C", "temp"),
			("high", "°C", "temp"),
			("crit", "°C", "temp"),
		],
		level_field: Some("value"),
	},
	CollectionDef { name: "fans", role: Role::Glance, fields: &[("chip", "", "text"), ("label", "", "text"), ("rpm", "RPM", "rpm")], level_field: None },
	CollectionDef {
		name: "disks",
		role: Role::Glance,
		fields: &[
			("device", "", "text"),
			("mount", "", "text"),
			("fs", "", "text"),
			("used", "B", "bytes"),
			("available", "B", "bytes"),
			("total", "B", "bytes"),
			("percent", "%", "percent"),
			("removable", "", "bool"),
		],
		level_field: Some("percent"),
	},
	CollectionDef {
		name: "disk_io",
		role: Role::Glance,
		fields: &[("name", "", "text"), ("model", "", "text"), ("read", "B/s", "rate"), ("write", "B/s", "rate")],
		level_field: None,
	},
	CollectionDef {
		name: "net",
		role: Role::Glance,
		fields: &[
			("name", "", "text"),
			("kind", "", "text"),
			("up", "", "bool"),
			("addrs", "", "text"),
			("rx", "B/s", "rate"),
			("tx", "B/s", "rate"),
			("rx_total", "B", "bytes"),
			("tx_total", "B", "bytes"),
			("speed", "Mbit/s", "integer"),
		],
		level_field: None,
	},
	CollectionDef {
		name: "displays",
		role: Role::Monitor,
		fields: &[
			("name", "", "text"),
			("state", "", "text"),
			("connector", "", "text"),
			("gpu", "", "text"),
			("connected", "", "bool"),
			("active", "", "bool"),
			("dpms", "", "text"),
			("internal", "", "bool"),
			("vendor", "", "text"),
			("model", "", "text"),
			("year", "", "integer"),
			("size", "", "text"),
			("mode", "", "text"),
			("preferred", "", "text"),
			("max_refresh", "Hz", "number"),
			("position", "", "text"),
		],
		level_field: None,
	},
	CollectionDef {
		name: "links",
		role: Role::Monitor,
		fields: &[
			("id", "", "text"),
			("name", "", "text"),
			("url", "", "text"),
			("unit", "", "text"),
			("scope", "", "text"),
			("port", "", "integer"),
			("up", "", "bool"),
			("local_only", "", "bool"),
			("unit_state", "", "text"),
		],
		level_field: None,
	},
	CollectionDef {
		name: "activity",
		role: Role::Monitor,
		fields: &[("t", "ms", "time"), ("phone", "", "text"), ("what", "", "text"), ("result", "", "text"), ("message", "", "text")],
		level_field: None,
	},
	CollectionDef { name: "actions", role: Role::Operate, fields: &[("name", "", "text"), ("icon", "", "text")], level_field: None },
];

pub fn collection_def(name: &str) -> Option<&'static CollectionDef> {
	COLLECTIONS.iter().find(|c| c.name == name)
}

/// A field of `collection`, `key` included.
pub fn field_def(collection: &str, field: &str) -> Option<FieldDef> {
	if field == "key" {
		return Some(("key", "", "text"));
	}
	collection_def(collection)?.fields.iter().find(|f| f.0 == field).copied()
}

/// What `address` is, `{…}` in it standing for any one key: `gpu.{item}.usage` is
/// `gpu.<key>.usage`.
pub fn address_def(address: &str) -> Option<&'static AddressDef> {
	let parts = segments(address);
	ADDRESSES.iter().find(|d| {
		let p: Vec<&str> = d.pattern.split('.').collect();
		p.len() == parts.len() && p.iter().zip(&parts).all(|(p, s)| *p == "<key>" || s.starts_with('{') || p == s)
	})
}

/// The first parts of the addresses there are: an address that starts with one of these is the
/// service's, and must be one of `ADDRESSES`.
pub fn known_prefix(address: &str) -> bool {
	let first = segments(address).first().copied().unwrap_or("");
	ADDRESSES.iter().any(|d| d.pattern.split('.').next() == Some(first))
}

/// An address's parts between dots, a `{…}` (which may hold dots) counting as one part.
fn segments(address: &str) -> Vec<&str> {
	let mut out = Vec::new();
	let (mut start, mut depth) = (0, 0);
	for (i, c) in address.char_indices() {
		match c {
			'{' => depth += 1,
			'}' => depth -= 1,
			'.' if depth == 0 => {
				out.push(&address[start..i]);
				start = i + 1;
			}
			_ => {}
		}
	}
	out.push(&address[start..]);
	out
}

/// The address of `ADDRESSES` closest to an unknown one, to suggest.
pub fn suggest_address(address: &str) -> Option<String> {
	let parts = segments(address);
	let candidates: Vec<String> = ADDRESSES
		.iter()
		.filter_map(|d| {
			let p: Vec<&str> = d.pattern.split('.').collect();
			(p.len() == parts.len()).then(|| p.iter().zip(&parts).map(|(p, s)| if *p == "<key>" { *s } else { *p }).collect::<Vec<_>>().join("."))
		})
		.collect();
	settings::suggest(address, candidates.iter().map(String::as_str)).map(str::to_string)
}

/// The least role that sees `address`: glance for an address that isn't the service's own (a
/// value an integration adds sees as the basics do).
pub fn address_role(address: &str) -> Role {
	address_def(address).map_or(Role::Glance, |d| d.role)
}

pub fn collection_role(name: &str) -> Role {
	collection_def(name).map_or(Role::Admin, |c| c.role)
}

/// A thing's key from what names it: lower case, `a-z`, `0-9` and `_`, anything else `_`.
pub fn key_of(name: &str) -> String {
	let k: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect();
	if k.is_empty() { "_".into() } else { k }
}

/// Keys for things that may share a name: the second `x` is `x2`, the third `x3`.
#[derive(Default)]
struct Keys(HashMap<String, usize>);

impl Keys {
	fn take(&mut self, base: String) -> String {
		let n = self.0.entry(base.clone()).or_insert(0);
		*n += 1;
		if *n == 1 { base } else { format!("{base}{n}") }
	}
}

// ---- a value --------------------------------------------------------------------------------

/// A value as read: a whole number, a fraction, a text or a yes or no.
#[derive(Clone, Debug, PartialEq)]
pub enum V {
	Int(i64),
	Float(f64),
	Text(String),
	Bool(bool),
}

impl V {
	pub fn num(&self) -> Option<f64> {
		match self {
			V::Int(i) => Some(*i as f64),
			V::Float(f) => Some(*f),
			_ => None,
		}
	}

	pub fn shown(&self) -> Shown<'_> {
		match self {
			V::Int(i) => Shown::Num(*i as f64),
			V::Float(f) => Shown::Num(*f),
			V::Text(t) => Shown::Text(t),
			V::Bool(b) => Shown::Bool(*b),
		}
	}

	/// As the protocol sends it: a fraction rounded to one decimal.
	pub fn json(&self) -> serde_json::Value {
		match self {
			V::Int(i) => (*i).into(),
			V::Float(f) => num(format::round(*f, 1)),
			V::Text(t) => t.clone().into(),
			V::Bool(b) => (*b).into(),
		}
	}

	/// What a `map` or `levels` key is matched against: `true`, `12`, `discharging`.
	pub fn map_key(&self) -> String {
		match self {
			V::Int(i) => i.to_string(),
			V::Float(f) => format!("{f}"),
			V::Text(t) => t.clone(),
			V::Bool(b) => b.to_string(),
		}
	}

	/// Whether it equals what a `filter` asks for.
	pub fn matches(&self, want: &serde_json::Value) -> bool {
		match (self, want) {
			(V::Bool(a), serde_json::Value::Bool(b)) => a == b,
			(V::Text(a), serde_json::Value::String(b)) => a == b,
			(v, serde_json::Value::Number(n)) => v.num().zip(n.as_f64()).is_some_and(|(a, b)| a == b),
			(v, serde_json::Value::String(s)) => v.map_key() == *s,
			_ => false,
		}
	}
}

/// A JSON number from a float; not-a-number as null.
pub fn num(x: f64) -> serde_json::Value {
	serde_json::Number::from_f64(x).map_or(serde_json::Value::Null, serde_json::Value::Number)
}

fn int(x: impl TryInto<i64>) -> V {
	V::Int(x.try_into().unwrap_or(i64::MAX))
}

fn float(x: impl Into<f64>) -> V {
	V::Float(x.into())
}

fn text(s: impl Into<String>) -> V {
	V::Text(s.into())
}

// ---- levels ---------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
	Normal,
	Warning,
	Critical,
}

impl Level {
	pub fn as_str(self) -> &'static str {
		match self {
			Level::Normal => "normal",
			Level::Warning => "warning",
			Level::Critical => "critical",
		}
	}

	pub fn parse(s: &str) -> Option<Level> {
		[Level::Normal, Level::Warning, Level::Critical].into_iter().find(|l| l.as_str() == s)
	}
}

/// Where a value turns `warning` and `critical`, and how far before `warning` its colour starts
/// turning (`soft`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Limits {
	pub warning_above: Option<f64>,
	pub critical_above: Option<f64>,
	pub warning_below: Option<f64>,
	pub critical_below: Option<f64>,
	pub soft: f64,
}

impl Limits {
	/// The loads: warning above 75, critical above 90.
	pub fn load() -> Limits {
		Limits { warning_above: Some(75.0), critical_above: Some(90.0), ..Default::default() }
	}

	/// A temperature's, from its sensor's limits, as the 0.5 app judged them: critical at `crit`
	/// (or `high`, or 95), warning from `high` (or 20 below critical, never under 40), turning
	/// over the 20 before.
	pub fn temp(high: Option<f64>, crit: Option<f64>) -> Limits {
		let hot = crit.or(high).unwrap_or(95.0);
		let warm = high.filter(|h| *h < hot).unwrap_or(hot - 20.0).max(40.0);
		Limits { warning_above: Some(warm), critical_above: Some(hot), soft: 20.0, ..Default::default() }
	}

	/// The battery's charge: warning below 30, critical below 15.
	pub fn battery() -> Limits {
		Limits { warning_below: Some(30.0), critical_below: Some(15.0), ..Default::default() }
	}

	/// A process's share of the CPU, as the 0.5 app coloured it: a quarter of the CPU is a lot
	/// already, judged as a load of that.
	pub fn process_cpu() -> Limits {
		Limits { warning_above: Some(18.75), critical_above: Some(22.5), ..Default::default() }
	}

	fn map(&self, f: impl Fn(f64) -> f64, scale: f64) -> Limits {
		Limits {
			warning_above: self.warning_above.map(&f),
			critical_above: self.critical_above.map(&f),
			warning_below: self.warning_below.map(&f),
			critical_below: self.critical_below.map(&f),
			soft: self.soft * scale,
		}
	}

	/// The level of `x`, and how far it has turned toward the next (0–1).
	pub fn judge(&self, x: f64) -> (Level, f64) {
		let up = side(x, self.warning_above, self.critical_above, self.soft);
		let down = side(-x, self.warning_below.map(|v| -v), self.critical_below.map(|v| -v), self.soft);
		if (down.0, down.1) > (up.0, up.1) { down } else { up }
	}
}

/// Judges `x` against limits it mustn't rise above.
fn side(x: f64, warning: Option<f64>, critical: Option<f64>, soft: f64) -> (Level, f64) {
	let turn = |from: f64, to: f64| if to > from { ((x - from) / (to - from)).clamp(0.0, 1.0) } else { 0.0 };
	if critical.is_some_and(|c| x >= c) {
		return (Level::Critical, 0.0);
	}
	match (warning, critical) {
		(Some(w), c) if x >= w => (Level::Warning, c.map_or(0.0, |c| turn(w, c))),
		(Some(w), _) if soft > 0.0 && x >= w - soft => (Level::Normal, turn(w - soft, w)),
		(None, Some(c)) if soft > 0.0 && x >= c - soft => (Level::Normal, turn(c - soft, c)),
		_ => (Level::Normal, 0.0),
	}
}

// ---- the values of a reading ----------------------------------------------------------------

/// The full scale of a value: a number, or another address (`mem.used` is out of `mem.total`).
#[derive(Clone, Debug, PartialEq)]
pub enum Max {
	Num(serde_json::Number),
	Address(String),
}

/// A value with what it needs to be shown and judged.
#[derive(Clone, Debug, PartialEq)]
pub struct Scalar {
	pub v: V,
	pub unit: &'static str,
	pub format: &'static str,
	pub limits: Option<Limits>,
	pub max: Option<Max>,
}

/// An item of a collection: its key, fields, the limits its level is judged by, and the least role
/// that sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
	pub key: String,
	pub fields: BTreeMap<&'static str, V>,
	pub limits: Option<Limits>,
	pub role: Role,
}

impl Item {
	fn new(key: String, role: Role) -> Item {
		Item { key, fields: BTreeMap::new(), limits: None, role }
	}

	fn set(&mut self, field: &'static str, v: Option<V>) {
		if let Some(v) = v {
			self.fields.insert(field, v);
		}
	}

	/// A field, `key` included.
	pub fn get(&self, field: &str) -> Option<V> {
		if field == "key" {
			return Some(V::Text(self.key.clone()));
		}
		self.fields.get(field).cloned()
	}
}

/// What one reading of the computer gives: the sample, the facts, and what is read for some
/// clients only.
pub struct Reading<'a> {
	pub sample: &'a Sample,
	pub info: &'a Info,
	pub media: Option<&'a Media>,
	pub processes: Option<&'a [Process]>,
	/// The activity log, newest first.
	pub activity: &'a [Entry],
	/// The actions the settings add.
	pub actions: &'a BTreeMap<String, settings::Action>,
}

/// Every value of one reading, by address, and the collections.
#[derive(Clone, Debug, Default)]
pub struct Values {
	/// When it was read, Unix ms.
	pub t: u64,
	pub scalars: BTreeMap<String, Scalar>,
	pub collections: BTreeMap<&'static str, Vec<Item>>,
	/// The players and the volume, for `media` widgets.
	pub media: Option<Media>,
	/// power-profiles-daemon's profiles, for `profile.set`.
	pub power_profiles: Vec<String>,
}

impl Values {
	fn put(&mut self, address: String, v: Option<V>) {
		let Some(v) = v else { return };
		let def = address_def(&address).copied().unwrap_or(AddressDef { pattern: "", unit: "", format: "text", role: Role::Glance });
		let limits = default_limits(&address, def.unit);
		let max = default_max(&address, def.unit);
		self.scalars.insert(address, Scalar { v, unit: def.unit, format: def.format, limits, max });
	}

	/// Sets the limits of a value that is there.
	fn limit(&mut self, address: &str, limits: Limits) {
		if let Some(s) = self.scalars.get_mut(address) {
			s.limits = Some(limits);
		}
	}

	/// The keys of a collection's items, in order: what decides which dashboards and widgets
	/// `for_each` makes.
	#[cfg(test)]
	pub fn keys(&self, collection: &str) -> Vec<&str> {
		self.collections.get(collection).map(|c| c.iter().map(|i| i.key.as_str()).collect()).unwrap_or_default()
	}
}

/// The limits an address has of its own.
fn default_limits(address: &str, unit: &str) -> Option<Limits> {
	let loads = ["cpu.usage", "mem.percent", "swap.percent"];
	let ends = [".usage", ".mem_percent"];
	if loads.contains(&address) || (address.starts_with("gpu.") && ends.iter().any(|e| address.ends_with(e))) {
		return Some(Limits::load());
	}
	if address.starts_with("disk.") && address.ends_with(".percent") {
		return Some(Limits::load());
	}
	if address == "battery.percent" {
		return Some(Limits::battery());
	}
	(unit == "°C").then(|| Limits::temp(None, None))
}

/// The full scale an address has of its own: 100 for a percentage, the total for what is used of it.
fn default_max(address: &str, unit: &str) -> Option<Max> {
	if unit == "%" {
		return Some(Max::Num(100.into()));
	}
	let total = |a: &str, from: &str, to: &str| a.strip_suffix(from).map(|base| Max::Address(format!("{base}{to}")));
	match address {
		"mem.used" | "mem.available" => Some(Max::Address("mem.total".into())),
		"swap.used" => Some(Max::Address("swap.total".into())),
		"battery.energy_now" => Some(Max::Address("battery.energy_full".into())),
		a if a.starts_with("disk.") => total(a, ".used", ".total").or_else(|| total(a, ".available", ".total")),
		a if a.starts_with("gpu.") => total(a, ".mem_used", ".mem_total"),
		_ => None,
	}
}

/// "NVIDIA GeForce RTX 3050 Ti Laptop GPU" → "RTX 3050 Ti": the part people say.
pub fn short_gpu_name(name: &str) -> String {
	let mut n = name;
	for p in ["NVIDIA ", "GeForce ", "AMD ", "Radeon "] {
		n = n.strip_prefix(p).unwrap_or(n);
	}
	for s in [" Mobile", " Laptop GPU"] {
		n = n.strip_suffix(s).unwrap_or(n);
	}
	n.to_string()
}

/// "1920 × 1080 @ 144 Hz".
fn mode_text(m: &crate::model::Mode) -> String {
	format!("{} × {} @ {}", m.width, m.height, hz(m.refresh as f64))
}

fn hz(x: f64) -> String {
	let whole = x.round();
	if (x - whole).abs() < 0.05 { format!("{whole} Hz") } else { format!("{} Hz", format::fixed(x, 2)) }
}

fn plural(n: usize, one: &str, many: &str) -> String {
	format!("{n} {}", if n == 1 { one } else { many })
}

/// The values of one reading.
pub fn read(r: &Reading) -> Values {
	let s = r.sample;
	let mut v = Values { t: s.t, media: r.media.cloned(), power_profiles: s.power.ppd_profiles.clone(), ..Default::default() };
	let pct = |part: f64, whole: f64| (whole > 0.0).then(|| part / whole * 100.0);

	// CPU and the system
	v.put("cpu.usage".into(), Some(float(s.cpu.usage)));
	v.put("cpu.temp".into(), s.cpu.temp.map(float));
	if !s.cpu.freq.is_empty() {
		let mean = s.cpu.freq.iter().map(|f| *f as f64).sum::<f64>() / s.cpu.freq.len() as f64;
		v.put("cpu.freq".into(), Some(int(mean.round() as i64)));
		v.put("cpu.freq_max".into(), s.cpu.freq.iter().max().map(|f| int(*f)));
	}
	for (i, name) in ["cpu.load1", "cpu.load5", "cpu.load15"].into_iter().enumerate() {
		v.put(name.into(), Some(float(s.cpu.load[i])));
	}
	v.put("cpu.model".into(), Some(text(r.info.cpu_model.clone())));
	v.put("cpu.cores".into(), r.info.cpu_physical_cores.map(int));
	v.put("cpu.threads".into(), Some(int(r.info.cpu_logical_cores)));
	let mem = &s.memory;
	v.put("mem.used".into(), Some(int(mem.used)));
	v.put("mem.available".into(), Some(int(mem.available)));
	v.put("mem.total".into(), Some(int(mem.total)));
	v.put("mem.percent".into(), pct(mem.used as f64, mem.total as f64).map(float));
	v.put("swap.used".into(), Some(int(mem.swap_used)));
	v.put("swap.total".into(), Some(int(mem.swap_total)));
	v.put("swap.percent".into(), pct(mem.swap_used as f64, mem.swap_total as f64).map(float));
	v.put("system.name".into(), Some(text(r.info.name.clone())));
	v.put("system.os".into(), Some(text(r.info.os.clone())));
	v.put("system.kernel".into(), Some(text(r.info.kernel.clone())));
	v.put("system.arch".into(), Some(text(r.info.arch.clone())));
	v.put("system.version".into(), Some(text(r.info.service_version)));
	v.put("system.uptime".into(), Some(int(s.uptime)));
	v.put("system.boot_time".into(), Some(int(r.info.boot_time)));

	// The CPU's temperature is judged by its sensor's limits, as the 0.5 app found it.
	if let Some(t) = s.cpu.temp
		&& let Some(sensor) = cpu_sensor(&s.temps, t)
	{
		v.limit("cpu.temp", Limits::temp(sensor.high.map(f64::from), sensor.crit.map(f64::from)));
	}

	read_gpus(&mut v, r);
	read_sensors(&mut v, s);
	read_storage(&mut v, s);
	read_network(&mut v, s);

	// Cores
	let cores = s
		.cpu
		.cores
		.iter()
		.enumerate()
		.map(|(i, usage)| {
			let mut item = Item::new(i.to_string(), Role::Glance);
			item.set("usage", Some(float(*usage)));
			item.set("freq", s.cpu.freq.get(i).map(|f| int(*f)));
			item.limits = Some(Limits::load());
			item
		})
		.collect();
	v.collections.insert("cores", cores);

	// Battery and power
	if let Some(b) = &s.battery {
		v.put("battery.percent".into(), b.percent.map(float));
		let health = b.energy_full.zip(b.energy_design).and_then(|(f, d)| pct(f as f64, d as f64));
		v.put("battery.health".into(), health.map(float));
		v.put("battery.power".into(), b.power.map(float));
		v.put("battery.energy_now".into(), b.energy_now.map(float));
		v.put("battery.energy_full".into(), b.energy_full.map(float));
		v.put("battery.energy_design".into(), b.energy_design.map(float));
		v.put("battery.time_left".into(), b.time_left.map(int));
		v.put("battery.status".into(), Some(text(b.status.clone())));
		let status = match b.status.as_str() {
			"charging" => "Charging",
			"discharging" => "Discharging",
			"full" => "Full",
			"not_charging" => "Not charging",
			_ => "Unknown",
		};
		v.put("battery.status_text".into(), Some(text(status)));
		let left = b.time_left.map(|t| format::format(Shown::Num(t as f64), "duration", "s"));
		let time = left.map(|d| if b.status == "charging" { format!("full in {d}") } else { format!("{d} left") });
		v.put("battery.time_text".into(), time.map(text));
		v.put("battery.cycles".into(), b.cycles.map(int));
		v.put("battery.model".into(), b.model.clone().map(text));
	}
	let p = &s.power;
	v.put("power.on_ac".into(), p.on_ac.map(V::Bool));
	v.put("power.profile".into(), p.ppd_profile.clone().map(text));
	v.put("power.platform_profile".into(), p.profile.clone().map(text));
	v.put("power.governor".into(), p.governor.clone().map(text));
	v.put("power.epp".into(), p.epp.clone().map(text));

	// The volume
	if let Some(vol) = r.media.and_then(|m| m.volume.as_ref()) {
		v.put("volume.level".into(), Some(float(vol.level as f64 * 100.0)));
		v.put("volume.muted".into(), Some(V::Bool(vol.muted)));
	}

	read_displays(&mut v, s);
	read_links(&mut v, s);

	// Processes, while some client asked for them
	if let Some(list) = r.processes {
		let items = list
			.iter()
			.map(|p| {
				let mut item = Item::new(p.pid.to_string(), Role::Monitor);
				item.set("pid", Some(int(p.pid)));
				item.set("ppid", p.ppid.map(int));
				item.set("name", Some(text(p.name.clone())));
				item.set("cmd", Some(text(p.cmd.clone())));
				item.set("user", p.user.clone().map(text));
				item.set("cpu", Some(float(p.cpu)));
				item.set("mem", Some(int(p.mem)));
				item.set("status", Some(text(p.status.clone())));
				item.set("start", Some(int(p.start)));
				item.set("read", Some(float(p.read)));
				item.set("write", Some(float(p.write)));
				item.set("io", Some(float(p.read + p.write)));
				item.set("gpu", p.gpu.map(float));
				item.set("gpu_mem", p.gpu_mem.map(int));
				item.limits = Some(Limits::process_cpu());
				item
			})
			.collect();
		v.collections.insert("processes", items);
	}

	// The activity log
	let mut keys = Keys::default();
	let activity = r
		.activity
		.iter()
		.map(|e| {
			let mut item = Item::new(keys.take(e.t.to_string()), Role::Monitor);
			item.set("t", Some(int(e.t)));
			item.set("phone", Some(text(e.phone.clone())));
			item.set("what", Some(text(e.what.clone())));
			item.set("result", Some(text(e.result.clone())));
			item.set("message", e.message.clone().map(text));
			item
		})
		.collect();
	v.collections.insert("activity", activity);

	// The actions the settings add
	let actions = r
		.actions
		.iter()
		.map(|(id, a)| {
			let mut item = Item::new(id.clone(), a.role.max(Role::Operate));
			item.set("name", Some(text(a.name.clone())));
			item.set("icon", Some(text(a.icon.clone().unwrap_or_else(|| "terminal".into()))));
			item
		})
		.collect();
	v.collections.insert("actions", actions);
	v
}

/// The sensor the CPU's temperature came from, for its limits: the first CPU sensor named like the
/// whole package, else the first CPU sensor, else the ACPI zone; only one that reads the same.
fn cpu_sensor(temps: &[crate::model::Temp], cpu: f32) -> Option<&crate::model::Temp> {
	let package = |l: &str| {
		let l = l.to_lowercase();
		l.starts_with("package") || l == "tctl" || l == "tdie" || l == "cpu"
	};
	let cpus: Vec<&crate::model::Temp> = temps.iter().filter(|t| t.kind == "cpu").collect();
	let found = cpus.iter().find(|t| package(&t.label)).or(cpus.first()).copied();
	let found = found.or_else(|| temps.iter().find(|t| t.chip == "acpitz" || t.chip == "x86_pkg_temp"));
	found.filter(|t| (t.value - cpu).abs() < 0.05)
}

fn read_gpus(v: &mut Values, r: &Reading) {
	let mut keys = Keys::default();
	let mut items = Vec::new();
	for g in &r.sample.gpus {
		let info = r.info.gpus.iter().find(|i| i.id == g.id);
		let vendor = info.map_or("other", |i| i.vendor);
		let key = keys.take(if vendor == "other" { "gpu".into() } else { vendor.to_string() });
		let name = info.map_or_else(|| "GPU".to_string(), |i| i.name.clone());
		let summary = if g.state == "sleeping" {
			"Asleep".to_string()
		} else if g.freq == Some(0) {
			"Idle".to_string()
		} else if let Some(f) = g.freq {
			format::format(Shown::Num(f as f64), "mhz", "MHz")
		} else if g.state == "active" {
			"Active".to_string()
		} else {
			"No readings".to_string()
		};
		let mem_percent = g.mem_used.zip(g.mem_total).filter(|(_, t)| *t > 0).map(|(u, t)| u as f64 / t as f64 * 100.0);
		let mut item = Item::new(key.clone(), Role::Glance);
		let fields: [(&'static str, Option<V>); 17] = [
			("name", Some(text(name.clone()))),
			("short_name", Some(text(short_gpu_name(&name)))),
			("vendor", Some(text(vendor))),
			("driver", info.map(|i| text(i.driver.clone()))),
			("state", Some(text(g.state))),
			("summary", Some(text(summary))),
			("usage", g.usage.map(float)),
			("mem_percent", mem_percent.map(float)),
			("temp", g.temp.map(float)),
			("power", g.power.map(float)),
			("mem_used", g.mem_used.map(int)),
			("mem_total", g.mem_total.map(int)),
			("freq", g.freq.map(int)),
			("freq_max", g.freq_max.map(int)),
			("mem_freq", g.mem_freq.map(int)),
			("fan", g.fan.map(int)),
			("key", None),
		];
		for (field, value) in fields {
			if field == "key" {
				continue;
			}
			v.put(format!("gpu.{key}.{field}"), value.clone());
			if collection_def("gpus").is_some_and(|c| c.fields.iter().any(|f| f.0 == field)) {
				item.set(field, value);
			}
		}
		// A GPU's temperature by the limits of its own sensor, when one reads the same.
		if let Some(t) = g.temp
			&& let Some(sensor) = r.sample.temps.iter().find(|s| s.kind == "gpu" && (s.value - t).abs() < 0.05)
		{
			v.limit(&format!("gpu.{key}.temp"), Limits::temp(sensor.high.map(f64::from), sensor.crit.map(f64::from)));
		}
		item.limits = Some(Limits::load());
		items.push(item);
	}
	v.collections.insert("gpus", items);
}

fn read_sensors(v: &mut Values, s: &Sample) {
	let mut keys = Keys::default();
	let mut temps = Vec::new();
	for t in &s.temps {
		let key = keys.take(key_of(format!("{}_{}", t.chip, t.label).trim_end_matches('_')));
		let limits = Limits::temp(t.high.map(f64::from), t.crit.map(f64::from));
		v.put(format!("temp.{key}"), Some(float(t.value)));
		v.limit(&format!("temp.{key}"), limits.clone());
		v.put(format!("temp.{key}.high"), t.high.map(float));
		v.put(format!("temp.{key}.crit"), t.crit.map(float));
		let mut item = Item::new(key, Role::Glance);
		item.set("chip", Some(text(t.chip.clone())));
		item.set("kind", Some(text(t.kind)));
		item.set("device", t.device.clone().map(text));
		item.set("label", Some(text(t.label.clone())));
		item.set("value", Some(float(t.value)));
		item.set("high", t.high.map(float));
		item.set("crit", t.crit.map(float));
		item.limits = Some(limits);
		temps.push(item);
	}
	v.collections.insert("temps", temps);

	let mut keys = Keys::default();
	let mut fans = Vec::new();
	for f in &s.fans {
		let key = keys.take(key_of(&format!("{}_{}", f.chip, f.label)));
		v.put(format!("fan.{key}"), Some(int(f.rpm)));
		let mut item = Item::new(key, Role::Glance);
		item.set("chip", Some(text(f.chip.clone())));
		item.set("label", Some(text(f.label.clone())));
		item.set("rpm", Some(int(f.rpm)));
		fans.push(item);
	}
	v.collections.insert("fans", fans);

	let spinning: Vec<u32> = s.fans.iter().map(|f| f.rpm).filter(|r| *r > 0).collect();
	let summary = if let Some(top) = spinning.iter().max() {
		Some(format!("{} · {top} RPM", plural(spinning.len(), "fan", "fans")))
	} else if !s.fans.is_empty() {
		Some("Fans stopped".into())
	} else if !s.temps.is_empty() {
		Some(plural(s.temps.len(), "sensor", "sensors"))
	} else {
		None
	};
	v.put("fans.summary".into(), summary.map(text));
}

fn read_storage(v: &mut Values, s: &Sample) {
	let mut keys = Keys::default();
	let mut disks = Vec::new();
	for d in &s.disks {
		let trimmed = d.mount.trim_matches('/');
		let key = keys.take(if trimmed.is_empty() { "root".into() } else { key_of(trimmed) });
		let used = d.total.saturating_sub(d.available);
		let percent = (d.total > 0).then(|| used as f64 / d.total as f64 * 100.0);
		v.put(format!("disk.{key}.used"), Some(int(used)));
		v.put(format!("disk.{key}.available"), Some(int(d.available)));
		v.put(format!("disk.{key}.total"), Some(int(d.total)));
		v.put(format!("disk.{key}.percent"), percent.map(float));
		let mut item = Item::new(key, Role::Glance);
		item.set("device", Some(text(d.device.clone())));
		item.set("mount", Some(text(d.mount.clone())));
		item.set("fs", Some(text(d.fs.clone())));
		item.set("used", Some(int(used)));
		item.set("available", Some(int(d.available)));
		item.set("total", Some(int(d.total)));
		item.set("percent", percent.map(float));
		item.set("removable", Some(V::Bool(d.removable)));
		item.limits = Some(Limits::load());
		disks.push(item);
	}
	v.collections.insert("disks", disks);

	let mut keys = Keys::default();
	let mut io = Vec::new();
	for d in &s.disk_io {
		let key = keys.take(key_of(&d.name));
		v.put(format!("disk_io.{key}.read"), Some(float(d.read)));
		v.put(format!("disk_io.{key}.write"), Some(float(d.write)));
		let mut item = Item::new(key, Role::Glance);
		item.set("name", Some(text(d.name.clone())));
		item.set("model", d.model.clone().map(text));
		item.set("read", Some(float(d.read)));
		item.set("write", Some(float(d.write)));
		io.push(item);
	}
	v.collections.insert("disk_io", io);
	v.put("disk.read".into(), Some(float(s.disk_io.iter().map(|d| d.read).sum::<f64>())));
	v.put("disk.write".into(), Some(float(s.disk_io.iter().map(|d| d.write).sum::<f64>())));
}

fn read_network(v: &mut Values, s: &Sample) {
	let mut keys = Keys::default();
	let mut items = Vec::new();
	for n in &s.net {
		let key = keys.take(key_of(&n.name));
		v.put(format!("net.{key}.rx"), Some(float(n.rx)));
		v.put(format!("net.{key}.tx"), Some(float(n.tx)));
		v.put(format!("net.{key}.rx_total"), Some(int(n.rx_total)));
		v.put(format!("net.{key}.tx_total"), Some(int(n.tx_total)));
		v.put(format!("net.{key}.up"), Some(V::Bool(n.up)));
		v.put(format!("net.{key}.speed"), n.speed.map(int));
		let mut item = Item::new(key, Role::Glance);
		item.set("name", Some(text(n.name.clone())));
		item.set("kind", Some(text(n.kind)));
		item.set("up", Some(V::Bool(n.up)));
		item.set("addrs", Some(text(n.addrs.join(", "))));
		item.set("rx", Some(float(n.rx)));
		item.set("tx", Some(float(n.tx)));
		item.set("rx_total", Some(int(n.rx_total)));
		item.set("tx_total", Some(int(n.tx_total)));
		item.set("speed", n.speed.map(int));
		items.push(item);
	}
	v.collections.insert("net", items);
	// Virtual links mirror real traffic (bridges, a VPN over Wi-Fi); counting them would double it.
	let real = |n: &&crate::model::Net| n.kind == "wired" || n.kind == "wireless";
	v.put("net.rx".into(), Some(float(s.net.iter().filter(real).map(|n| n.rx).sum::<f64>())));
	v.put("net.tx".into(), Some(float(s.net.iter().filter(real).map(|n| n.tx).sum::<f64>())));
	let up: Vec<&str> = s.net.iter().filter(|n| n.up && n.kind != "virtual").map(|n| n.name.as_str()).collect();
	v.put("net.up_names".into(), (!up.is_empty()).then(|| text(up.join(format::PARTS))));
}

fn read_displays(v: &mut Values, s: &Sample) {
	let mut keys = Keys::default();
	let mut items = Vec::new();
	for d in &s.displays {
		let mut item = Item::new(keys.take(key_of(&d.connector)), Role::Monitor);
		let name = [d.vendor.as_deref(), d.model.as_deref()].into_iter().flatten().filter(|s| !s.trim().is_empty()).collect::<Vec<_>>().join(" ");
		let state = if !d.connected {
			"Disconnected"
		} else if d.active {
			"On"
		} else if matches!(d.dpms.as_deref(), Some("Standby") | Some("Suspend")) {
			"Standby"
		} else if d.mode.is_some() || d.dpms.as_deref() == Some("Off") {
			"Off"
		} else {
			"Not in use"
		};
		let size = d.width_mm.zip(d.height_mm).filter(|(w, h)| *w > 0 && *h > 0).map(|(w, h)| {
			let inches = ((w as f64).powi(2) + (h as f64).powi(2)).sqrt() / 25.4;
			format!("{}″ · {} × {} cm", format::fixed(inches, 1), w / 10, h / 10)
		});
		item.set("name", Some(text(if name.is_empty() { d.connector.clone() } else { name })));
		item.set("state", Some(text(state)));
		item.set("connector", Some(text(d.connector.clone())));
		item.set("gpu", Some(text(d.gpu.clone())));
		item.set("connected", Some(V::Bool(d.connected)));
		item.set("active", Some(V::Bool(d.active)));
		item.set("dpms", d.dpms.clone().map(text));
		item.set("internal", Some(V::Bool(d.internal)));
		item.set("vendor", d.vendor.clone().map(text));
		item.set("model", d.model.clone().map(text));
		item.set("year", d.year.map(int));
		item.set("size", size.map(text));
		item.set("mode", d.mode.as_ref().map(|m| text(mode_text(m))));
		item.set("preferred", d.preferred.as_ref().map(|m| text(mode_text(m))));
		item.set("max_refresh", d.max_refresh.map(float));
		item.set("position", d.position.filter(|_| d.mode.is_some()).map(|p| text(format!("{}, {}", p[0], p[1]))));
		items.push(item);
	}
	let connected = s.displays.iter().filter(|d| d.connected).count();
	let on = s.displays.iter().filter(|d| d.connected && d.active).count();
	let summary = match (s.displays.is_empty(), connected) {
		(true, _) => None,
		(false, 0) => Some("No monitors connected".to_string()),
		(false, n) => Some(format!("{n} connected{}{on} on", format::PARTS)),
	};
	v.put("displays.summary".into(), summary.map(text));
	v.collections.insert("displays", items);
}

fn read_links(v: &mut Values, s: &Sample) {
	let mut items = Vec::new();
	for l in &s.links {
		v.put(format!("link.{}.up", l.id), l.up.map(V::Bool));
		v.put(format!("link.{}.unit_state", l.id), l.unit_state.clone().map(text));
		let mut item = Item::new(l.id.clone(), Role::Monitor);
		item.set("id", Some(text(l.id.clone())));
		item.set("name", Some(text(l.name.clone())));
		item.set("url", Some(text(l.url.clone())));
		item.set("unit", l.unit.clone().map(text));
		item.set("scope", Some(text(l.scope.clone())));
		item.set("port", l.port.map(int));
		item.set("up", l.up.map(V::Bool));
		item.set("local_only", Some(V::Bool(l.local_only)));
		item.set("unit_state", l.unit_state.clone().map(text));
		items.push(item);
	}
	v.collections.insert("links", items);
}

// ---- what one client sees -------------------------------------------------------------------

/// A profile's units: temperatures in °F, network rates in bit/s.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Units {
	pub fahrenheit: bool,
	pub bits: bool,
}

fn to_f(c: f64) -> f64 {
	c * 9.0 / 5.0 + 32.0
}

/// A value in a profile's units: `unit` its own, `network` whether it is a network rate.
fn convert(v: &V, unit: &str, network: bool, units: Units) -> (V, Option<&'static str>) {
	match (unit, v.num()) {
		("°C", Some(x)) if units.fahrenheit => (V::Float(to_f(x)), Some("°F")),
		("B/s", Some(x)) if units.bits && network => (V::Float(x * 8.0), Some("bit/s")),
		_ => (v.clone(), None),
	}
}

/// A number `address` had, in a profile's units.
pub fn in_units(address: &str, x: f64, units: Units) -> f64 {
	let unit = address_def(address).map_or("", |d| d.unit);
	convert(&V::Float(x), unit, address.starts_with("net."), units).0.num().unwrap_or(x)
}

fn convert_limits(l: &Limits, unit: &str, network: bool, units: Units) -> Limits {
	match unit {
		"°C" if units.fahrenheit => l.map(to_f, 9.0 / 5.0),
		"B/s" if units.bits && network => l.map(|x| x * 8.0, 8.0),
		_ => l.clone(),
	}
}

/// The values as one client sees them: only what its role does, in its profile's units.
#[derive(Clone, Copy)]
pub struct View<'a> {
	pub values: &'a Values,
	pub role: Role,
	pub units: Units,
}

impl<'a> View<'a> {
	pub fn new(values: &'a Values, role: Role, units: Units) -> Self {
		View { values, role, units }
	}

	/// A value, if there is one this client may see.
	pub fn get(&self, address: &str) -> Option<Scalar> {
		if self.role < address_role(address) {
			return None;
		}
		let s = self.values.scalars.get(address)?;
		let network = address.starts_with("net.");
		let (v, unit) = convert(&s.v, s.unit, network, self.units);
		let limits = s.limits.as_ref().map(|l| convert_limits(l, s.unit, network, self.units));
		Some(Scalar { v, unit: unit.unwrap_or(s.unit), format: s.format, limits, max: s.max.clone() })
	}

	/// The unit `address` has for this client, whether or not it is there now.
	pub fn unit_of(&self, address: &str) -> &'static str {
		let unit = address_def(address).map_or("", |d| d.unit);
		self.convert_unit(unit, address.starts_with("net."))
	}

	fn convert_unit(&self, unit: &'static str, network: bool) -> &'static str {
		match unit {
			"°C" if self.units.fahrenheit => "°F",
			"B/s" if self.units.bits && network => "bit/s",
			u => u,
		}
	}

	/// The unit of a collection's field for this client.
	pub fn field_unit(&self, collection: &str, field: &str) -> &'static str {
		let unit = field_def(collection, field).map_or("", |f| f.1);
		self.convert_unit(unit, collection == "net")
	}

	/// A value written into a string: by `format` if given, else its own; `None` when missing.
	pub fn text(&self, address: &str, format: Option<&str>) -> Option<String> {
		let s = self.get(address)?;
		Some(format::format(s.v.shown(), format.unwrap_or(s.format), s.unit))
	}

	/// A collection's items this client may see, in its units; `None` when its role doesn't see
	/// the collection.
	pub fn collection(&self, name: &str) -> Option<Vec<Item>> {
		if self.role < collection_role(name) {
			return None;
		}
		let items = self.values.collections.get(name).map(Vec::as_slice).unwrap_or_default();
		let network = name == "net";
		Some(
			items
				.iter()
				.filter(|i| self.role >= i.role)
				.map(|i| {
					let mut item = i.clone();
					for (field, v) in item.fields.iter_mut() {
						let unit = field_def(name, field).map_or("", |f| f.1);
						*v = convert(v, unit, network, self.units).0;
					}
					if let Some(l) = &item.limits {
						let unit = collection_def(name).and_then(|c| c.level_field).and_then(|f| field_def(name, f)).map_or("", |f| f.1);
						item.limits = Some(convert_limits(l, unit, network, self.units));
					}
					item
				})
				.collect(),
		)
	}

	/// The level of an item of `collection`, by its collection's field and its own limits.
	pub fn item_level(&self, collection: &str, item: &Item) -> Option<(Level, f64)> {
		let field = collection_def(collection)?.level_field?;
		let x = item.fields.get(field)?.num()?;
		Some(item.limits.as_ref()?.judge(x))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	pub fn fixture_values() -> Values {
		let sample = crate::fixtures::sample();
		let info = crate::fixtures::info();
		let media = crate::fixtures::media();
		let procs = crate::fixtures::processes();
		let actions = BTreeMap::new();
		read(&Reading { sample: &sample, info: &info, media: Some(&media), processes: Some(&procs), activity: &[], actions: &actions })
	}

	#[test]
	fn addresses_from_a_reading() {
		let v = fixture_values();
		let get = |a: &str| v.scalars.get(a).map(|s| s.v.clone());
		assert_eq!(get("cpu.usage"), Some(V::Float(12.5)));
		assert_eq!(get("cpu.freq_max"), Some(V::Int(3100)));
		assert_eq!(get("cpu.freq"), Some(V::Int(2750)));
		assert_eq!(get("cpu.cores"), Some(V::Int(8)));
		assert_eq!(get("mem.percent"), Some(V::Float(50.0)));
		assert_eq!(get("gpu.nvidia.usage"), Some(V::Float(33.0)));
		assert_eq!(get("gpu.nvidia.short_name"), Some(V::Text("Some GPU".into())));
		assert_eq!(get("gpu.nvidia.summary"), Some(V::Text("1.50 GHz".into())));
		assert_eq!(get("gpu.nvidia.mem_percent"), Some(V::Float(25.0)));
		assert_eq!(get("temp.nvme_composite"), Some(V::Float(41.5)));
		assert_eq!(get("temp.nvme_composite.crit"), Some(V::Float(85.0)));
		assert_eq!(get("fan.hp_cpu"), Some(V::Int(2500)));
		assert_eq!(get("fans.summary"), Some(V::Text("1 fan · 2500 RPM".into())));
		assert_eq!(get("disk.root.percent"), Some(V::Float(80.0)));
		assert_eq!(get("disk.root.used"), Some(V::Int(400_000_000_000)));
		assert_eq!(get("disk.read"), Some(V::Float(1024.5)));
		assert_eq!(get("disk_io.nvme0n1.write"), Some(V::Float(2048.5)));
		assert_eq!(get("net.rx"), Some(V::Float(1500.5)));
		assert_eq!(get("net.eno1.up"), Some(V::Bool(true)));
		assert_eq!(get("net.up_names"), Some(V::Text("eno1".into())));
		assert_eq!(get("battery.health").and_then(|v| v.num()).map(|h| format::round(h, 1)), Some(95.2));
		assert_eq!(get("battery.time_text"), Some(V::Text("2 h 0 min left".into())));
		assert_eq!(get("battery.status_text"), Some(V::Text("Discharging".into())));
		assert_eq!(get("battery.model"), Some(V::Text("Some battery".into())));
		assert_eq!(get("power.profile"), Some(V::Text("balanced".into())));
		assert_eq!(get("power.platform_profile"), Some(V::Text("balanced".into())));
		assert_eq!(get("volume.level"), Some(V::Float(75.0)));
		assert_eq!(get("link.config-0123456789ab.up"), Some(V::Bool(true)));
		assert_eq!(get("system.version"), Some(V::Text("9.9.9".into())));
		assert_eq!(get("displays.summary"), Some(V::Text("1 connected · 1 on".into())));
		// Every value read has its address among those there are.
		for a in v.scalars.keys() {
			assert!(address_def(a).is_some(), "{a}");
		}
		let display = &v.collections["displays"][0];
		assert_eq!(display.get("size"), Some(V::Text("24.0″ · 53 × 30 cm".into())));
		assert_eq!(display.get("mode"), Some(V::Text("1920 × 1080 @ 180 Hz".into())));
		assert_eq!(display.get("name"), Some(V::Text("Some vendor Some monitor".into())));
		assert_eq!(v.collections["processes"][0].get("io"), Some(V::Float(31.0)));
		assert_eq!(v.keys("cores"), ["0", "1"]);
	}

	#[test]
	fn keys_name_things_by_what_they_are() {
		assert_eq!(key_of("Package id 0"), "package_id_0");
		assert_eq!(key_of("run/media/usb"), "run_media_usb");
		let mut k = Keys::default();
		assert_eq!((k.take("amd".into()), k.take("amd".into()), k.take("nvidia".into())), ("amd".into(), "amd2".into(), "nvidia".into()));
		assert_eq!(short_gpu_name("NVIDIA GeForce RTX 3050 Ti Laptop GPU"), "RTX 3050 Ti");
		assert!(address_def("gpu.{item}.usage").is_some() && address_def("gpu.amd2.temp").is_some());
		assert!(address_def("gpu.amd.usgae").is_none() && known_prefix("gpu.amd.usgae") && !known_prefix("ups.charge"));
		assert_eq!(suggest_address("cpu.usgae").as_deref(), Some("cpu.usage"));
		assert_eq!(suggest_address("gpu.amd.tmp").as_deref(), Some("gpu.amd.temp"));
	}

	#[test]
	fn levels_turn_as_the_0_5_app_turned_them() {
		// A temperature without a sensor's limits: 75 warning, 95 critical, turning from 55.
		let t = Limits::temp(None, None);
		assert_eq!(t.judge(55.5), (Level::Normal, 0.025));
		assert_eq!(t.judge(50.0), (Level::Normal, 0.0));
		assert_eq!(t.judge(85.0), (Level::Warning, 0.5));
		assert_eq!(t.judge(95.0), (Level::Critical, 0.0));
		// The sensor's: warning at high, critical at crit; never warning under 40.
		assert_eq!(Limits::temp(Some(80.0), Some(85.0)), Limits { warning_above: Some(80.0), critical_above: Some(85.0), soft: 20.0, ..Default::default() });
		assert_eq!(Limits::temp(Some(30.0), Some(50.0)).warning_above, Some(40.0));
		assert_eq!(Limits::temp(None, Some(100.0)).warning_above, Some(80.0));
		let load = Limits::load();
		assert_eq!(load.judge(80.0).0, Level::Warning);
		assert!((load.judge(80.0).1 - 1.0 / 3.0).abs() < 1e-9);
		assert_eq!(load.judge(74.9), (Level::Normal, 0.0));
		assert_eq!(Limits::battery().judge(80.0), (Level::Normal, 0.0));
		assert_eq!(Limits::battery().judge(20.0).0, Level::Warning);
		assert_eq!(Limits::battery().judge(10.0), (Level::Critical, 0.0));
	}

	#[test]
	fn a_view_sees_what_its_role_does_in_its_units() {
		let v = fixture_values();
		let glance = View::new(&v, Role::Glance, Units { fahrenheit: true, bits: true });
		assert!(glance.get("volume.level").is_none() && glance.collection("processes").is_none());
		let t = glance.get("cpu.temp").unwrap();
		assert_eq!((t.v.num().map(|x| format::round(x, 1)), t.unit), (Some(131.9), "°F"));
		assert_eq!(t.limits.unwrap().critical_above, Some(203.0));
		let rx = glance.get("net.rx").unwrap();
		assert_eq!((rx.v, rx.unit), (V::Float(12004.0), "bit/s"));
		// Disk rates aren't the network's.
		assert_eq!(glance.get("disk.read").unwrap().unit, "B/s");
		assert_eq!(glance.collection("temps").unwrap()[0].get("value"), Some(V::Float(to_f(41.5))));
		assert_eq!(glance.text("mem.used", Some("bytes")).as_deref(), Some("7.45 GiB"));
		let monitor = View::new(&v, Role::Monitor, Units::default());
		assert_eq!(monitor.get("volume.muted").map(|s| s.v), Some(V::Bool(false)));
		assert!(monitor.collection("actions").is_none() && monitor.collection("processes").is_some());
	}
}
