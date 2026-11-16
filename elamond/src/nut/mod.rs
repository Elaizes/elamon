//! UPS sources (docs/config.md, Sources): each `[sources.<id>]` read from upsd through upst-core's
//! poller, a thread of its own whose updates go to a task here. The task keeps the station as
//! upst-core models it (`Ups`), makes its values after each update and leaves them as a snapshot
//! that the collector takes at its tick: an upsd that is slow or gone never holds up a reading, a
//! stream or another source.

pub mod cli;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use tokio::sync::{mpsc, oneshot, watch};
use upst_core::history::recordable;
use upst_core::poll::{self, Poller, Update};
use upst_core::status::{flag_info, Severity, Status};
use upst_core::target::Server;
use upst_core::{fmt as nut_fmt, Event, Link, Ups};

use crate::access::Role;
use crate::consts::{limit, timeout};
use crate::format;
use crate::settings::{Config, NutSource};
use crate::values::{self, AddressDef, Item, Level, Scalar, Values, V};

// ---- what a source's addresses are ----------------------------------------------------------

/// What a source's address is from what follows its id: a NUT variable as upst-core reads its name
/// (`battery.charge` %, `outlet.2.realpower` W, `ambient.temperature` °C), the driver's, serial
/// numbers, versions, ids and dates for `monitor`; or what the service makes of them.
pub fn def_of(var: &str) -> AddressDef {
	let (unit, format, role) = match var {
		"status" | "status.power" | "status.flags" | "link" => ("", "text", Role::Glance),
		"link.error" => ("", "text", Role::Monitor),
		"status.since" | "estimate.to_empty" | "estimate.to_full" | "estimate.to_low" => ("s", "duration", Role::Glance),
		"estimate.watts" => ("W", "watts", Role::Glance),
		_ => {
			let unit = match nut_fmt::unit(var) {
				"C" => "°C",
				u => u,
			};
			let format = match unit {
				"%" => "percent",
				"°C" => "temp",
				"W" => "watts",
				"s" => "duration",
				// A number without a unit is a `number`, as its value tells (`values::source_scalar`).
				"" => "text",
				_ => "number",
			};
			(unit, format, if recordable(var) { Role::Glance } else { Role::Monitor })
		}
	};
	AddressDef { pattern: "", unit, format, role }
}

/// Where a station's power comes from, as its timeline has it: on line (or on bypass), on
/// battery, or not known.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Supply {
	Line,
	Battery,
	#[default]
	None,
}

impl Supply {
	pub fn as_str(self) -> &'static str {
		match self {
			Supply::Line => "line",
			Supply::Battery => "battery",
			Supply::None => "none",
		}
	}

	/// From `ups.status`: on battery with `OB`; on line with `OL`, or on bypass.
	pub fn of(status: &Status) -> Supply {
		if status.has("OB") {
			Supply::Battery
		} else if status.has("OL") || status.power() == upst_core::Power::Bypass {
			Supply::Line
		} else {
			Supply::None
		}
	}
}

/// The level of what upst-core weighs as `s`.
fn level_of(s: Severity) -> Level {
	match s {
		Severity::Good | Severity::Info => Level::Normal,
		Severity::Warn => Level::Warning,
		Severity::Crit => Level::Critical,
	}
}

/// `<id>.status` and its level: the status's headline while upsd is read, else how the link is.
fn status_of(ups: &Ups) -> (&'static str, Level) {
	match ups.link() {
		Link::Up => {
			let text = match ups.status().headline().0 {
				"FORCED SHUTDOWN" => "Forced shutdown",
				"LOW BATTERY" => "Low battery",
				"OVERLOAD" => "Overload",
				"ON BATTERY" => "On battery",
				"ON BYPASS" => "On bypass",
				"OUTPUT OFF" => "Output off",
				"ON LINE" => "On line",
				"NO STATUS" => "No status",
				_ => "Unknown",
			};
			(text, level_of(ups.status().severity()))
		}
		Link::Connecting => ("Connecting", Level::Normal),
		Link::Stale(_) => ("No data", Level::Warning),
		Link::Down(_) => ("Unreachable", Level::Critical),
	}
}

fn link_word(link: &Link) -> &'static str {
	match link {
		Link::Connecting => "connecting",
		Link::Up => "up",
		Link::Stale(_) => "stale",
		Link::Down(_) => "down",
	}
}

/// Whether one of `patterns` (`*` any run of characters, as nutflow's `allow`) matches `name`.
pub fn allows(patterns: &[String], name: &str) -> bool {
	fn matches(p: &[u8], s: &[u8]) -> bool {
		match p.split_first() {
			None => s.is_empty(),
			Some((b'*', rest)) => (0..=s.len()).any(|i| matches(rest, &s[i..])),
			Some((c, rest)) => s.split_first().is_some_and(|(d, tail)| c == d && matches(rest, tail)),
		}
	}
	patterns.iter().any(|p| matches(p.as_bytes(), name.as_bytes()))
}

// ---- one station ----------------------------------------------------------------------------

/// The fields of an outlet the station tells, by `outlet.<N>.<field>`.
const OUTLET_FIELDS: [&str; 6] = ["desc", "status", "realpower", "voltage", "current", "switchable"];

/// What changes with every reading of an outlet: gone while upsd isn't read.
const OUTLET_LIVE: [&str; 4] = ["status", "realpower", "voltage", "current"];

/// What the service knows of one source: the station as upst-core keeps it, and what is kept
/// beside it since the source started.
pub struct Station {
	id: String,
	ups: Ups,
	/// Where the power comes from, and since when (Unix seconds).
	supply: (Supply, Option<u64>),
	/// The variables made values: no more than `limit::NUT_VARS_MAX`.
	vars: BTreeSet<String>,
	/// More than those were left out, which the journal said.
	too_many: bool,
	/// Every address it has had, with its format.
	known: BTreeMap<String, &'static str>,
	/// Every outlet it has had, by number, with its fields as last told.
	outlets: BTreeMap<u32, BTreeMap<&'static str, V>>,
}

/// What a source has, at its last update: what the collector puts among the values.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
	/// Its values by address, `status.since` aside: the station's variables while upsd is read, and
	/// what the service makes of them.
	pub scalars: BTreeMap<String, Scalar>,
	/// Every address it has had since the source started, with the format it had.
	pub known: BTreeMap<String, &'static str>,
	/// Its numbers the long history may keep.
	pub kept: Vec<String>,
	/// Where its power comes from, and since when (Unix seconds).
	pub supply: Supply,
	pub since: Option<u64>,
	/// Its item of the `ups` collection, and of `outlets` (all but `source` and `critical`).
	pub item: BTreeMap<&'static str, V>,
	pub outlets: BTreeMap<u32, BTreeMap<&'static str, V>>,
	/// Polls taken in, which the tests count.
	#[cfg_attr(not(test), allow(dead_code))]
	pub polls: u64,
}

impl Station {
	pub fn new(id: &str, src: &NutSource) -> Station {
		let label = Server { host: src.host.clone(), port: src.port }.to_string();
		// What any station comes to have, known from the start: the first time upsd is lost, or the
		// power comes from somewhere, makes no new `ui`.
		let known = [("status.since", "duration"), ("link.error", "text")].map(|(var, f)| (format!("{id}.{var}"), f)).into();
		Station {
			id: id.to_string(),
			ups: Ups::builder(&src.ups, &label).capacity_wh(src.capacity_wh).history(None).build(),
			supply: (Supply::None, None),
			vars: BTreeSet::new(),
			too_many: false,
			known,
			outlets: BTreeMap::new(),
		}
	}

	fn supplied(&mut self, s: Supply, at: SystemTime) {
		if s != self.supply.0 {
			self.supply = (s, Some(upst_core::unix(at) as u64));
		}
	}

	/// A poll of every variable, answered at `at`; the first is where watching starts. Returns
	/// what it implies.
	pub fn polled(&mut self, vars: Vec<(String, String)>, at: SystemTime) -> Vec<Event> {
		let events = self.ups.apply(vars, at);
		self.supplied(Supply::of(self.ups.status()), at);
		events
	}

	/// upsd answers without data for the station (`Link::Stale`), or can't be reached
	/// (`Link::Down`), since `at`.
	pub fn lost(&mut self, link: Link, at: SystemTime) -> Option<Event> {
		let event = self.ups.lost(link, at);
		self.supplied(Supply::None, at);
		event
	}

	/// Its values now, `at` the time of the last update.
	pub fn snapshot(&mut self, at: SystemTime) -> Snapshot {
		let mut s = Snapshot { supply: self.supply.0, since: self.supply.1, polls: self.ups.polls(), ..Default::default() };
		let id = self.id.clone();
		let address = |var: &str| format!("{id}.{var}");
		let up = self.ups.is_live();
		if up {
			for (name, var) in self.ups.vars() {
				if !self.vars.contains(name) {
					if self.vars.len() >= limit::NUT_VARS_MAX {
						if !self.too_many {
							eprintln!("nut {id}: the station has more than {} variables; those after are left out", limit::NUT_VARS_MAX);
							self.too_many = true;
						}
						continue;
					}
					self.vars.insert(name.clone());
				}
				let v = match nut_fmt::parse_num(&var.value) {
					Some(x) => V::Float(x),
					None => V::Text(var.value.clone()),
				};
				if v.num().is_some() && recordable(name) {
					s.kept.push(address(name));
				}
				s.scalars.insert(address(name), values::source_scalar(name, v, None));
			}
			let flags: Vec<&str> = self.ups.status().extra_flags().map(|f| Some(flag_info(f).label).filter(|l| !l.is_empty()).unwrap_or(f)).collect();
			if !flags.is_empty() {
				s.scalars.insert(address("status.flags"), values::source_scalar("status.flags", V::Text(flags.join(format::PARTS)), None));
			}
			if let Some(e) = self.ups.estimate(at) {
				for (var, x) in [("estimate.watts", e.watts), ("estimate.to_empty", e.to_empty), ("estimate.to_full", e.to_full), ("estimate.to_low", e.to_low)] {
					if let Some(x) = x.filter(|x| x.is_finite()) {
						s.kept.push(address(var));
						s.scalars.insert(address(var), values::source_scalar(var, V::Float(x), None));
					}
				}
			}
			s.kept.truncate(limit::NUT_VARS_MAX);
		}
		let (status, level) = status_of(&self.ups);
		s.scalars.insert(address("status"), values::source_scalar("status", V::Text(status.into()), Some(level)));
		s.scalars.insert(address("status.power"), values::source_scalar("status.power", V::Text(self.supply.0.as_str().into()), None));
		s.scalars.insert(address("link"), values::source_scalar("link", V::Text(link_word(self.ups.link()).into()), None));
		if let Link::Stale(why) | Link::Down(why) = self.ups.link() {
			s.scalars.insert(address("link.error"), values::source_scalar("link.error", V::Text(why.clone()), None));
		}
		for (a, scalar) in &s.scalars {
			self.known.insert(a.clone(), scalar.format);
		}
		s.known = self.known.clone();
		self.read_outlets(up);
		s.outlets = self.outlets.clone();
		s.item = self.item(status);
		s
	}

	/// The outlets from `outlet.<N>.<field>`: each kept once seen, what changes with every reading
	/// gone while upsd isn't read.
	fn read_outlets(&mut self, up: bool) {
		if !up {
			for o in self.outlets.values_mut() {
				o.retain(|f, _| !OUTLET_LIVE.contains(f));
			}
			return;
		}
		let mut now: BTreeMap<u32, BTreeMap<&'static str, V>> = BTreeMap::new();
		for (name, var) in self.ups.vars() {
			let Some((n, field)) = name.strip_prefix("outlet.").and_then(|r| r.split_once('.')) else { continue };
			let (Ok(n), Some(field)) = (n.parse::<u32>(), OUTLET_FIELDS.iter().find(|f| **f == field)) else { continue };
			let v = match *field {
				"switchable" => V::Bool(var.value.trim().eq_ignore_ascii_case("yes")),
				"desc" | "status" => V::Text(var.value.clone()),
				_ => match nut_fmt::parse_num(&var.value) {
					Some(x) => V::Float(x),
					None => continue,
				},
			};
			now.entry(n).or_default().insert(field, v);
		}
		for (n, fields) in now {
			let o = self.outlets.entry(n).or_default();
			o.retain(|f, _| !OUTLET_LIVE.contains(f));
			o.extend(fields);
		}
	}

	/// The station's item of the `ups` collection: its name kept from its last variables.
	fn item(&self, status: &str) -> BTreeMap<&'static str, V> {
		let text = |k: &str| self.ups.text(k).map(|t| V::Text(t.to_string()));
		let model = text("device.model").or_else(|| text("ups.model"));
		let mut item = BTreeMap::new();
		item.insert("name", model.clone().unwrap_or_else(|| V::Text(self.id.clone())));
		if let Some(m) = model {
			item.insert("model", m);
		}
		if let Some(m) = text("device.mfr").or_else(|| text("ups.mfr")) {
			item.insert("mfr", m);
		}
		item.insert("status", V::Text(status.into()));
		item.insert("power", V::Text(self.supply.0.as_str().into()));
		item.insert("link", V::Text(link_word(self.ups.link()).into()));
		item.insert("ecoflow", V::Bool(self.ups.ecoflow()));
		item
	}

	/// Takes in what the poller reports; whether the station's values changed.
	fn update(&mut self, src: &NutSource, u: Update) -> Option<SystemTime> {
		let now = SystemTime::now();
		let events: Vec<Event> = match u {
			Update::Vars { ups, vars, at } if ups == src.ups => {
				let e = self.polled(vars, at);
				self.say(&e);
				return Some(at);
			}
			Update::NoData { ups, error } if ups == src.ups => self.lost(Link::Stale(error), now).into_iter().collect(),
			Update::Down { error, .. } => self.lost(Link::Down(error), now).into_iter().collect(),
			_ => return None,
		};
		self.say(&events);
		Some(now)
	}

	/// Writes in the journal what happened to the power, the battery's charge and the link.
	fn say(&self, events: &[Event]) {
		for e in events {
			let code = e.code();
			if ["power.", "battery.low", "upsd.", "data."].iter().any(|p| code.starts_with(p)) {
				eprintln!("nut {}: {code}: {e}", self.id);
			}
		}
	}
}

/// The sources' values, with `status.since` as of the reading's `t`, the `ups` and `outlets`
/// collections, the addresses they have had and the numbers the history keeps: one for each source
/// of `config`, from its snapshot, or as one connecting before it has any.
pub fn put(values: &mut Values, config: &Config, snapshots: &BTreeMap<String, Arc<Snapshot>>) {
	let now = values.t / 1000;
	let mut ups = Vec::new();
	let mut outlets = Vec::new();
	for (id, src) in &config.sources {
		let waiting;
		let snap = match snapshots.get(id) {
			Some(s) => s.as_ref(),
			None => {
				waiting = Station::new(id, src).snapshot(SystemTime::now());
				&waiting
			}
		};
		values.scalars.extend(snap.scalars.iter().map(|(a, s)| (a.clone(), s.clone())));
		if snap.supply != Supply::None
			&& let Some(since) = snap.since
		{
			let age = V::Int(now.saturating_sub(since).try_into().unwrap_or(i64::MAX));
			values.scalars.insert(format!("{id}.status.since"), values::source_scalar("status.since", age, None));
		}
		values.known.extend(snap.known.iter().map(|(a, f)| (a.clone(), *f)));
		if src.history {
			values.kept.extend(snap.kept.iter().cloned());
		}
		let mut item = Item::new(id.clone(), Role::Glance);
		for (f, v) in &snap.item {
			item.set(f, Some(v.clone()));
		}
		ups.push(item);
		for (n, fields) in &snap.outlets {
			let mut o = Item::new(format!("{id}.{n}"), Role::Glance);
			o.set("source", Some(V::Text(id.clone())));
			o.set("id", Some(V::Int(i64::from(*n))));
			for (f, v) in fields {
				o.set(f, Some(v.clone()));
			}
			o.set("critical", Some(V::Bool(src.critical(*n))));
			outlets.push(o);
		}
	}
	values.collections.insert("ups", ups);
	values.collections.insert("outlets", outlets);
}

// ---- the sources, running -------------------------------------------------------------------

/// How often a source is asked: as often as its `interval_ms` while a client shows its values,
/// every `background_ms` while its history is kept or a graph shows it, else not at all.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Demand {
	Live,
	#[default]
	Background,
	Pause,
}

/// One source, running: its settings, its last snapshot and how often it is to be asked. Dropped,
/// its task ends and the poller's thread with it.
struct Running {
	src: NutSource,
	snapshot: Arc<Mutex<Arc<Snapshot>>>,
	demand: watch::Sender<Demand>,
	_stop: oneshot::Sender<()>,
}

/// Every source of the settings, each read by a poller of its own.
#[derive(Default)]
pub struct Runtime {
	inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
	/// Where the sources' tasks run, once the service has started them.
	handle: Option<tokio::runtime::Handle>,
	sources: BTreeMap<String, Running>,
}

impl Runtime {
	/// Starts the sources of `config`, their tasks on `handle`.
	pub fn start(&self, handle: tokio::runtime::Handle, config: &Config) {
		self.lock().handle = Some(handle);
		self.apply(config);
	}

	fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
		self.inner.lock().unwrap_or_else(|p| p.into_inner())
	}

	/// Takes newly read settings: a source taken out of them, or changed, is stopped; one new or
	/// changed starts. Before `start`, nothing runs.
	pub fn apply(&self, config: &Config) {
		let mut inner = self.lock();
		let Some(handle) = inner.handle.clone() else { return };
		inner.sources.retain(|id, r| {
			let keep = config.sources.get(id) == Some(&r.src);
			if !keep {
				eprintln!("nut {id}: stopped");
			}
			keep
		});
		for (id, src) in &config.sources {
			if !inner.sources.contains_key(id) {
				match spawn(&handle, id, src) {
					Ok(r) => {
						inner.sources.insert(id.clone(), r);
					}
					Err(e) => eprintln!("nut {id}: can't start reading upsd: {e}"),
				}
			}
		}
	}

	/// Each source's last snapshot.
	pub fn snapshots(&self) -> BTreeMap<String, Arc<Snapshot>> {
		let inner = self.lock();
		inner.sources.iter().map(|(id, r)| (id.clone(), r.snapshot.lock().unwrap_or_else(|p| p.into_inner()).clone())).collect()
	}

	/// How often each source is to be asked now: `live` while a client shows a source's values,
	/// `tracked` what graphs and sparks show.
	pub fn set_demand(&self, live: bool, tracked: &BTreeSet<String>) {
		let inner = self.lock();
		for (id, r) in &inner.sources {
			let graphed = tracked.iter().any(|a| a.split('.').next() == Some(id.as_str()));
			let want = if live {
				Demand::Live
			} else if r.src.history || graphed {
				Demand::Background
			} else {
				Demand::Pause
			};
			r.demand.send_if_modified(|d| std::mem::replace(d, want) != want);
		}
	}
}

/// A poller for `src`, and the task that takes in what it reports.
fn spawn(handle: &tokio::runtime::Handle, id: &str, src: &NutSource) -> std::io::Result<Running> {
	let (tx, updates) = mpsc::unbounded_channel();
	let mut cfg = poll::Config::new(Server { host: src.host.clone(), port: src.port });
	cfg.names = Some(vec![src.ups.clone()]);
	cfg.interval = Duration::from_millis(src.background());
	cfg.timeout = timeout::NUT;
	cfg.meta = !src.commands.is_empty() || !src.settings.is_empty();
	let poller = Poller::spawn(cfg, move |u| tx.send(u).is_ok())?;
	let mut station = Station::new(id, src);
	let snapshot = Arc::new(Mutex::new(Arc::new(station.snapshot(SystemTime::now()))));
	let (demand, wanted) = watch::channel(Demand::Background);
	let (stop, stopped) = oneshot::channel();
	handle.spawn(run(station, src.clone(), poller, updates, wanted, stopped, snapshot.clone()));
	Ok(Running { src: src.clone(), snapshot, demand, _stop: stop })
}

/// A source's task: what the poller reports becomes the source's snapshot, and the poller asks as
/// often as wanted, until the source is stopped.
async fn run(
	mut station: Station,
	src: NutSource,
	poller: Poller,
	mut updates: mpsc::UnboundedReceiver<Update>,
	mut wanted: watch::Receiver<Demand>,
	mut stopped: oneshot::Receiver<()>,
	out: Arc<Mutex<Arc<Snapshot>>>,
) {
	let mut asking = Asking { interval: src.background(), paused: false };
	asking.set(&poller, &src, *wanted.borrow_and_update());
	loop {
		tokio::select! {
			_ = &mut stopped => break,
			changed = wanted.changed() => {
				if changed.is_err() {
					break;
				}
				asking.set(&poller, &src, *wanted.borrow_and_update());
			}
			u = updates.recv() => {
				let Some(u) = u else { break };
				if let Some(at) = station.update(&src, u) {
					let s = Arc::new(station.snapshot(at));
					*out.lock().unwrap_or_else(|p| p.into_inner()) = s;
				}
			}
		}
	}
	// Dropped, the poller tells its thread to end, which it does once a call in flight returns.
	drop(poller);
}

/// How the poller asks now.
struct Asking {
	interval: u64,
	paused: bool,
}

impl Asking {
	fn set(&mut self, poller: &Poller, src: &NutSource, d: Demand) {
		let interval = match d {
			Demand::Live => src.interval_ms,
			Demand::Background => src.background(),
			Demand::Pause => {
				if !self.paused {
					poller.pause(true);
					self.paused = true;
				}
				return;
			}
		};
		// A new interval asks at once, unless paused: then going on does.
		if interval != self.interval {
			poller.set_interval(Duration::from_millis(interval));
			self.interval = interval;
		}
		if self.paused {
			poller.pause(false);
			self.paused = false;
		}
	}
}

#[cfg(test)]
pub mod tests;
