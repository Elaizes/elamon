//! UPS sources (docs/config.md, Sources): each `[sources.<id>]` read from upsd through upst-core's
//! poller, a thread of its own whose updates go to a task here. The task keeps the station as
//! upst-core models it (`Ups`), makes its values after each update and leaves them as a snapshot
//! that the collector takes at its tick: an upsd that is slow or gone never holds up a reading, a
//! stream or another source.

pub mod actions;
pub mod cli;
pub mod recorder;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use tokio::sync::{mpsc, oneshot, watch};
use upst_core::history::recordable;
use upst_core::journal::Record;
use upst_core::nut::VarInfo;
use upst_core::poll::{self, Action, Creds, Failure, Poller, Update};
use upst_core::status::{flag_info, Severity};
use upst_core::target::Server;
use upst_core::{fmt as nut_fmt, Event, Link, Ups};

use crate::access::Role;
use crate::actions::Spec;
use crate::consts::{limit, timeout};
use crate::format;
pub use crate::history::timeline::Supply;
use crate::history::timeline::{Change, Timeline};
use crate::history::Store;
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
	src: NutSource,
	ups: Ups,
	/// Where the power comes from now.
	supply: Supply,
	/// And since when, as far as the service has seen.
	timeline: Timeline,
	/// The variables made values: no more than `limit::NUT_VARS_MAX`.
	vars: BTreeSet<String>,
	/// More than those were left out, which the journal said.
	too_many: bool,
	/// Every address it has had, with its format.
	known: BTreeMap<String, &'static str>,
	/// Every outlet it has had, by number, with its fields as last told.
	outlets: BTreeMap<u32, BTreeMap<&'static str, V>>,
	/// What upsd last said the station offers, kept while it can't be reached; and what it said of
	/// each writable variable the source's `settings` allow.
	meta: Option<actions::Meta>,
	infos: BTreeMap<String, VarInfo>,
	/// The variables upsd was asked about since it last said what the station offers.
	asked: BTreeSet<String>,
	/// Those that take neither a list of values nor one range, which the journal said.
	unoffered: BTreeSet<String>,
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
	/// Where its power comes from, and since when (Unix seconds) while it comes from somewhere.
	pub supply: Supply,
	pub since: Option<u64>,
	/// Its power's changes, oldest first.
	pub timeline: Arc<Vec<Change>>,
	/// Its item of the `ups` collection, and of `outlets` (all but `source` and `critical`).
	pub item: BTreeMap<&'static str, V>,
	pub outlets: BTreeMap<u32, BTreeMap<&'static str, V>>,
	/// Its actions, once upsd has said what the station offers.
	pub actions: Vec<Spec>,
	/// Polls taken in, which the tests count.
	#[cfg_attr(not(test), allow(dead_code))]
	pub polls: u64,
}

impl Station {
	/// The station of the source `id`, its timeline kept in memory alone.
	pub fn new(id: &str, src: &NutSource) -> Station {
		Station::keeping(id, src, Timeline::memory())
	}

	/// The station of the source `id`, with its timeline as far as it goes.
	pub fn keeping(id: &str, src: &NutSource, timeline: Timeline) -> Station {
		let label = Server { host: src.host.clone(), port: src.port }.to_string();
		// What any station comes to have, known from the start: the first time upsd is lost, or the
		// power comes from somewhere, makes no new `ui`.
		let known = [("status.since", "duration"), ("link.error", "text")].map(|(var, f)| (format!("{id}.{var}"), f)).into();
		Station {
			id: id.to_string(),
			src: src.clone(),
			ups: Ups::builder(&src.ups, &label).capacity_wh(src.capacity_wh).history(None).build(),
			supply: Supply::None,
			timeline,
			vars: BTreeSet::new(),
			too_many: false,
			known,
			outlets: BTreeMap::new(),
			meta: None,
			infos: BTreeMap::new(),
			asked: BTreeSet::new(),
			unoffered: BTreeSet::new(),
		}
	}

	/// The power comes from `s` from `at` on, as `events` say.
	fn supplied(&mut self, s: Supply, at: SystemTime, events: &[Event]) {
		self.supply = s;
		let code = events.first().map_or("-", Event::code);
		self.timeline.set(s, upst_core::unix(at) as u64, code);
	}

	/// The last hour of the source's journal at its recorder (`records`, to `until`) taken in as if
	/// it had been read: estimates and events go on from it. What happened since the service last
	/// read the source (`since`) is said in the journal.
	pub fn warm(&mut self, records: &[Record], until: SystemTime, since: Option<u64>) {
		let events = self.ups.replay(records, until);
		for e in events.iter().filter(|e| since.is_none_or(|s| upst_core::unix(e.at) as u64 >= s)) {
			let ago = until.duration_since(e.at).unwrap_or_default().as_secs_f64();
			self.say_as(e, &format!(" ({} ago, from the recorder)", format::format(format::Shown::Num(ago), "duration", "s")));
		}
	}

	/// What a recorder kept from `from` to `until` (Unix seconds) in place of what the timeline has
	/// of that time: `window`, its changes then.
	fn recalled(&mut self, from: u64, until: u64, window: Vec<Change>) {
		self.timeline.merge(from, until, window);
	}

	/// The source stops being read at `at`: its timeline ends.
	fn close(&mut self, at: SystemTime) {
		self.timeline.close(upst_core::unix(at) as u64);
	}

	/// A poll of every variable, answered at `at`; the first is where watching starts. Returns
	/// what it implies.
	pub fn polled(&mut self, vars: Vec<(String, String)>, at: SystemTime) -> Vec<Event> {
		let events = self.ups.apply(vars, at);
		self.supplied(Supply::of(self.ups.status()), at, &events);
		events
	}

	/// upsd answers without data for the station (`Link::Stale`), or can't be reached
	/// (`Link::Down`), since `at`.
	pub fn lost(&mut self, link: Link, at: SystemTime) -> Option<Event> {
		let event = self.ups.lost(link, at);
		self.supplied(Supply::None, at, event.as_slice());
		event
	}

	/// Its values now, `at` the time of the last update.
	pub fn snapshot(&mut self, at: SystemTime) -> Snapshot {
		let timeline = self.timeline.changes();
		// Since its last change, which is to how it is now.
		let since = timeline.last().filter(|c| c.state == self.supply && self.supply != Supply::None).map(|c| c.t);
		let mut s = Snapshot { supply: self.supply, since, timeline, polls: self.ups.polls(), ..Default::default() };
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
		s.scalars.insert(address("status.power"), values::source_scalar("status.power", V::Text(self.supply.as_str().into()), None));
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
		let name = match s.item.get("name") {
			Some(V::Text(n)) => n.clone(),
			_ => self.id.clone(),
		};
		s.actions = self.actions(&name);
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
		item.insert("power", V::Text(self.supply.as_str().into()));
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
			self.say_as(e, "");
		}
	}

	fn say_as(&self, e: &Event, after: &str) {
		let code = e.code();
		if ["power.", "battery.low", "upsd.", "data."].iter().any(|p| code.starts_with(p)) {
			eprintln!("nut {}: {code}: {e}{after}", self.id);
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
		values.timelines.insert(id.clone(), snap.timeline.clone());
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
	/// Its settings, as they change without it starting again.
	retune: watch::Sender<NutSource>,
	/// Its actions go to its task, one at a time.
	acts: mpsc::Sender<Act>,
	one: Arc<tokio::sync::Mutex<()>>,
	_stop: oneshot::Sender<()>,
	task: tokio::task::JoinHandle<()>,
}

/// Every source of the settings, each read by a poller of its own.
#[derive(Default)]
pub struct Runtime {
	inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
	/// Where the sources' tasks run, once the service has started them, and the history they keep.
	handle: Option<(tokio::runtime::Handle, Arc<Store>)>,
	sources: BTreeMap<String, Running>,
}

impl Runtime {
	/// Starts the sources of `config`, their tasks on `handle`, their timelines and their last
	/// readings kept in `store`.
	pub fn start(&self, handle: tokio::runtime::Handle, config: &Config, store: Arc<Store>) {
		self.lock().handle = Some((handle, store));
		self.apply(config);
	}

	fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
		self.inner.lock().unwrap_or_else(|p| p.into_inner())
	}

	/// Takes newly read settings: a source taken out of them, or changed as `restarts` says, is
	/// stopped; one new or so changed starts. Another change is taken as the source runs. Before
	/// `start`, nothing runs.
	pub fn apply(&self, config: &Config) {
		let mut inner = self.lock();
		let Some((handle, store)) = inner.handle.clone() else { return };
		inner.sources.retain(|id, r| {
			let Some(src) = config.sources.get(id).filter(|s| !restarts(&r.src, s)) else {
				eprintln!("nut {id}: stopped");
				return false;
			};
			if *src != r.src {
				r.src = src.clone();
				r.retune.send_replace(src.clone());
			}
			true
		});
		for (id, src) in &config.sources {
			if !inner.sources.contains_key(id) {
				match spawn(&handle, &store, id, src) {
					Ok(r) => {
						inner.sources.insert(id.clone(), r);
					}
					Err(e) => eprintln!("nut {id}: can't start reading upsd: {e}"),
				}
			}
		}
	}

	/// Runs `action` on the station of source `id` as `creds`' user once any other action of the
	/// source has ended, and waits up to `timeout::NUT_ACTION` for the station's answer. `Err` says
	/// why there is none: none in time, or the source stopped meanwhile.
	pub async fn act(&self, id: &str, action: Action, creds: Creds) -> Result<Result<String, Failure>, &'static str> {
		let (acts, one) = {
			let inner = self.lock();
			let r = inner.sources.get(id).ok_or("the source isn't read")?;
			(r.acts.clone(), r.one.clone())
		};
		let _one = one.lock().await;
		let (reply, answer) = oneshot::channel();
		acts.send(Act { action, creds, reply }).await.map_err(|_| "the source stopped")?;
		match tokio::time::timeout(timeout::NUT_ACTION, answer).await {
			Ok(Ok(result)) => Ok(result),
			Ok(Err(_)) => Err("the source stopped"),
			Err(_) => Err(crate::errors::reason::TIMED_OUT),
		}
	}

	/// Each source's last snapshot.
	pub fn snapshots(&self) -> BTreeMap<String, Arc<Snapshot>> {
		let inner = self.lock();
		inner.sources.iter().map(|(id, r)| (id.clone(), r.snapshot.lock().unwrap_or_else(|p| p.into_inner()).clone())).collect()
	}

	/// Stops every source, its timeline ending now, and waits a moment for their tasks to end: as
	/// the service stops.
	pub async fn stop(&self) {
		let sources = std::mem::take(&mut self.lock().sources);
		// Each told to stop as the rest of it goes, before any is waited for.
		let tasks: Vec<_> = sources.into_values().map(|r| r.task).collect();
		for task in tasks {
			let _ = tokio::time::timeout(Duration::from_secs(1), task).await;
		}
	}

	/// How often each source is to be asked now: `live` while a client shows a source's values,
	/// `tracked` what graphs and sparks show, `timelines` the sources timelines show.
	pub fn set_demand(&self, live: bool, tracked: &BTreeSet<String>, timelines: &BTreeSet<String>) {
		let inner = self.lock();
		for (id, r) in &inner.sources {
			let graphed = timelines.contains(id) || tracked.iter().any(|a| a.split('.').next() == Some(id.as_str()));
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

/// A poller of `src`'s upsd, and where its updates come.
fn poller(src: &NutSource) -> std::io::Result<(Poller, mpsc::UnboundedReceiver<Update>)> {
	let (tx, updates) = mpsc::unbounded_channel();
	let mut cfg = poll::Config::new(Server { host: src.host.clone(), port: src.port });
	cfg.names = Some(vec![src.ups.clone()]);
	cfg.interval = Duration::from_millis(src.background());
	cfg.timeout = timeout::NUT;
	cfg.meta = offers_wanted(src);
	Ok((Poller::spawn(cfg, move |u| tx.send(u).is_ok())?, updates))
}

/// Whether upsd is asked what the station offers: for a source with commands or settings.
fn offers_wanted(src: &NutSource) -> bool {
	!src.commands.is_empty() || !src.settings.is_empty()
}

/// Whether a source whose settings go from `old` to `new` starts again: when what it reads, how
/// often, its recorder, its history or its battery change, or upsd is to be asked what the station
/// offers or no longer. Its commands, settings, role, outlets and NUT user are taken as it runs.
fn restarts(old: &NutSource, new: &NutSource) -> bool {
	let kept = NutSource {
		commands: old.commands.clone(),
		settings: old.settings.clone(),
		role: old.role,
		outlets: old.outlets.clone(),
		user: old.user.clone(),
		password_file: old.password_file.clone(),
		..new.clone()
	};
	kept != *old || offers_wanted(old) != offers_wanted(new)
}

/// A poller for `src`, and the task that takes in what it reports.
fn spawn(handle: &tokio::runtime::Handle, store: &Arc<Store>, id: &str, src: &NutSource) -> std::io::Result<Running> {
	let (poller, updates) = poller(src)?;
	let timeline = Timeline::load(&store.dir().join(crate::history::timeline::DIR), id, store.last_seen(id));
	let mut station = Station::keeping(id, src, timeline);
	let snapshot = Arc::new(Mutex::new(Arc::new(station.snapshot(SystemTime::now()))));
	let (demand, wanted) = watch::channel(Demand::Background);
	let (retune, retuned) = watch::channel(src.clone());
	let (stop, stopped) = oneshot::channel();
	let (filler, filled) = mpsc::channel(1);
	let (acts, asked) = mpsc::channel(1);
	let task = Task {
		id: id.to_string(),
		src: src.clone(),
		poller,
		updates,
		wanted,
		retuned,
		stopped,
		out: snapshot.clone(),
		store: store.clone(),
		filled,
		filler,
		cancel: Arc::new(AtomicBool::new(false)),
		asked,
		pending: Vec::new(),
	};
	let task = handle.spawn(task.run(station));
	Ok(Running { src: src.clone(), snapshot, demand, retune, acts, one: Arc::new(tokio::sync::Mutex::new(())), _stop: stop, task })
}

/// What a source's task works with, besides its station.
struct Task {
	id: String,
	src: NutSource,
	poller: Poller,
	updates: mpsc::UnboundedReceiver<Update>,
	wanted: watch::Receiver<Demand>,
	/// Its settings, as they change without it starting again: what it may run and who may.
	retuned: watch::Receiver<NutSource>,
	stopped: oneshot::Receiver<()>,
	/// Where its snapshot is left.
	out: Arc<Mutex<Arc<Snapshot>>>,
	store: Arc<Store>,
	/// What its recorder's journal made of its timeline, once its history is filled in from it.
	filled: mpsc::Receiver<Filled>,
	filler: mpsc::Sender<Filled>,
	/// Set as the task ends: filling in its history stops.
	cancel: Arc<AtomicBool>,
	/// The actions asked of the source, and those given to upsd that the station hasn't answered.
	asked: mpsc::Receiver<Act>,
	pending: Vec<Pending>,
}

/// An action for a source's station, and where its answer goes.
struct Act {
	action: Action,
	creds: Creds,
	reply: oneshot::Sender<Result<String, Failure>>,
}

/// An action given to upsd, waiting for the station's answer.
struct Pending {
	action: Action,
	reply: oneshot::Sender<Result<String, Failure>>,
	since: Instant,
}

/// An action whose asker has stopped waiting is forgotten after this, if its answer never comes.
const PENDING_KEPT: Duration = Duration::from_secs(120);

/// A source's history filled in from the recorder's journal `label`, from `from` to `until` (Unix
/// seconds): its timeline then, or why not.
struct Filled {
	label: String,
	from: u64,
	until: u64,
	result: Result<Vec<Change>, String>,
}

impl Task {
	/// What the poller reports becomes the source's snapshot, and the poller asks as often as
	/// wanted, until the source is stopped; then its timeline ends. With a recorder, the last hour
	/// of its journal comes first, and the history is filled in from it meanwhile.
	async fn run(mut self, mut station: Station) {
		let mut asking = Asking { interval: self.src.background(), paused: false };
		asking.set(&self.poller, &self.src, *self.wanted.borrow_and_update());
		// When the source was last read is kept, for a timeline left on line or battery to end then.
		// Read, its history is whole to then too, which the next start's recorder fills in from:
		// with a recorder, only once that has filled in what came before this start. A recorder
		// that can't be reached leaves it where it was, for one that can be to fill in from.
		let mut whole = self.src.recorder.is_none();
		if let Some(addr) = self.src.recorder.clone() {
			let recalled = recall(&self.id, &self.src, &addr, &self.store, &mut station, self.filler.clone(), self.cancel.clone());
			tokio::select! {
				_ = &mut self.stopped => {
					station.close(SystemTime::now());
					return;
				}
				_ = recalled => {}
			}
		}
		let mut at = SystemTime::now();
		// upsd had no data for the station since it last said what the station offers.
		let mut stale_offer = false;
		loop {
			tokio::select! {
				_ = &mut self.stopped => break,
				changed = self.wanted.changed() => {
					if changed.is_err() {
						break;
					}
					asking.set(&self.poller, &self.src, *self.wanted.borrow_and_update());
				}
				changed = self.retuned.changed() => {
					if changed.is_err() {
						break;
					}
					self.src = self.retuned.borrow_and_update().clone();
					station.src = self.src.clone();
					// What its settings allow now that upsd wasn't asked about.
					for var in station.undescribed() {
						self.poller.describe(&self.src.ups, &var);
					}
					let s = Arc::new(station.snapshot(at));
					*self.out.lock().unwrap_or_else(|p| p.into_inner()) = s;
				}
				Some(f) = self.filled.recv() => {
					whole = self.filled_in(&mut station, f);
					let s = Arc::new(station.snapshot(at));
					*self.out.lock().unwrap_or_else(|p| p.into_inner()) = s;
				}
				Some(act) = self.asked.recv() => {
					// Only one is asked at a time (`Running::one`); one whose asker gave up may still be answered.
					self.pending.retain(|p| !p.reply.is_closed() || p.since.elapsed() < PENDING_KEPT);
					self.poller.run(&self.src.ups, act.action.clone(), act.creds);
					self.pending.push(Pending { action: act.action, reply: act.reply, since: Instant::now() });
				}
				u = self.updates.recv() => {
					let Some(u) = u else { break };
					match &u {
						Update::Meta { ups, .. } if *ups == self.src.ups => stale_offer = false,
						Update::NoData { ups, .. } if *ups == self.src.ups => stale_offer = true,
						_ => {}
					}
					let u = match self.offered(&mut station, u) {
						Some(u) => u,
						None => {
							let s = Arc::new(station.snapshot(at));
							*self.out.lock().unwrap_or_else(|p| p.into_inner()) = s;
							continue;
						}
					};
					let read = matches!(&u, Update::Vars { ups, .. } if *ups == self.src.ups);
					if let Some(t) = station.update(&self.src, u) {
						at = t;
						if read {
							let t = upst_core::unix(at) as u64;
							self.store.seen(&self.id, t);
							if whole {
								self.store.filled(&self.id, t);
							}
						}
						if read && std::mem::take(&mut stale_offer) && offers_wanted(&self.src) {
							self.renew(&mut asking);
						} else if read {
							for var in station.undescribed() {
								self.poller.describe(&self.src.ups, &var);
							}
						}
						let s = Arc::new(station.snapshot(at));
						*self.out.lock().unwrap_or_else(|p| p.into_inner()) = s;
					}
				}
			}
		}
		self.cancel.store(true, Ordering::Relaxed);
		station.close(SystemTime::now());
		// Dropped, the poller tells its thread to end, which it does once a call in flight returns.
		drop(self.poller);
	}

	/// Starts the poller anew, for upsd to say again what the station offers, which it says only as
	/// the poller connects: one that connected while upsd had no data for the station (nutflow
	/// before it has reached the station) has nothing of it. The old poller's thread ends.
	fn renew(&mut self, asking: &mut Asking) {
		match poller(&self.src) {
			Ok((poller, updates)) => {
				self.poller = poller;
				self.updates = updates;
				*asking = Asking { interval: self.src.background(), paused: false };
				asking.set(&self.poller, &self.src, *self.wanted.borrow());
			}
			Err(e) => eprintln!("nut {}: can't start asking upsd anew ({e}): the station's commands and settings may be missing", self.id),
		}
	}

	/// Takes in what upsd says the station offers, and how the station answered an action (which
	/// goes to the first that waits for an answer to it); any other update is given back.
	fn offered(&mut self, station: &mut Station, u: Update) -> Option<Update> {
		match u {
			Update::Meta { ups, cmds, rw } if ups == self.src.ups => {
				for var in station.offers(cmds, rw) {
					self.poller.describe(&ups, &var);
				}
			}
			Update::Info { ups, var, info } if ups == self.src.ups => station.described(var, info),
			Update::Done { ups, action, result } if ups == self.src.ups => {
				if let Some(i) = self.pending.iter().position(|p| p.action == action) {
					let _ = self.pending.remove(i).reply.send(result);
				}
				// What it changed is read at once.
				self.poller.refresh();
			}
			other => return Some(other),
		}
		None
	}

	/// The timeline the recorder's journal made takes the place of the station's then, and the
	/// history is whole to then; or the journal says why not. Whether it is.
	fn filled_in(&self, station: &mut Station, f: Filled) -> bool {
		match f.result {
			Ok(window) => {
				station.recalled(f.from, f.until, window);
				self.store.filled(&self.id, f.until);
				let before = match f.from {
					0 => "all it had".to_string(),
					from => format!("the {}", format::format(format::Shown::Num(f.until.saturating_sub(from) as f64), "duration", "s")),
				};
				recorder::say(&self.id, format!("history filled in from the recorder's {}: {before} before this start", f.label));
				true
			}
			Err(e) => {
				let addr = self.src.recorder.as_deref().unwrap_or_default();
				recorder::say(&self.id, format!("recorder {addr}: {e}; no history from before elamond"));
				false
			}
		}
	}
}

/// The last hour of the source's journal at the recorder `addr`, taken in by its station before
/// anything read now; then its history filled in from the journal in the background, from where
/// it was last whole, what that made of its timeline sent to `filled`. A recorder that can't be
/// reached, or hasn't the UPS, is a line in the journal.
async fn recall(id: &str, src: &NutSource, addr: &str, store: &Arc<Store>, station: &mut Station, filled: mpsc::Sender<Filled>, cancel: Arc<AtomicBool>) {
	let since = store.last_seen(id);
	let until = crate::store::now();
	let (s, a) = (src.clone(), addr.to_string());
	let job = tokio::task::spawn_blocking(move || recorder::recent(&a, &s, until));
	let got = match tokio::time::timeout(timeout::RECORDER_START, job).await {
		Ok(Ok(got)) => got,
		Ok(Err(e)) => Err(e.to_string()),
		Err(_) => Err(format!("no answer in {} s", timeout::RECORDER_START.as_secs())),
	};
	let recorded = match got {
		Ok(r) => r,
		Err(e) => {
			recorder::say(id, format!("recorder {addr}: {e}; no history from before elamond"));
			return;
		}
	};
	station.warm(&recorded.recent, upst_core::from_unix(until as f64), since);
	let from = store.filled_until(id).unwrap_or(0);
	let (id, addr, store, history, label) = (id.to_string(), addr.to_string(), store.clone(), src.history, recorded.label);
	tokio::task::spawn_blocking(move || {
		let result = recorder::backfill(&addr, &label, &id, history, from, until, &store, &cancel);
		let _ = filled.blocking_send(Filled { label, from, until, result });
	});
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
