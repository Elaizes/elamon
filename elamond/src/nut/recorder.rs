//! A source's past from `upst record` (docs/config.md, The long history). As the source starts, the
//! last hour of the recorder's journal of it warms its station up (`Ups::replay`), so estimates and
//! events go on from there; then, in the background, the journal since the service last read the
//! source becomes its long history and its timeline. That is read a line at a time and walked with
//! `journal::State`: a year of journal is gigabytes as records, too much to replay at once.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use upst_core::fmt::parse_num;
use upst_core::history::recordable;
use upst_core::journal::{Feed, Kind, Record, State, GAP};
use upst_core::recorder::{Addr, Client, Listed, Stream};
use upst_core::target::Server;
use upst_core::Status;

use crate::consts::{cadence, limit, timeout};
use crate::history::disk::{Made, Point, Tier};
use crate::history::timeline::{Change, Supply};
use crate::history::Store;
use crate::settings::NutSource;

/// Writes `line` in the journal, of the source `id`.
pub fn say(id: &str, line: String) {
	eprintln!("nut {id}: {line}");
	#[cfg(test)]
	tests::SAID.lock().unwrap_or_else(|p| p.into_inner()).push((id.to_string(), line));
}

/// What a source's start took from its recorder: its journal's label, and the last hour of it.
pub struct Recorded {
	pub label: String,
	pub recent: Vec<Record>,
}

/// The journal of `src` among those a recorder lists: `<ups>@<host>[:<port>]` as the source reads
/// the UPS, else the only one of that UPS name.
pub fn label(listed: &[Listed], src: &NutSource) -> Result<String, String> {
	let server = Server { host: src.host.clone(), port: src.port }.to_string();
	let named = [format!("{}@{server}", src.ups), format!("{}@{}:{}", src.ups, src.host, src.port), format!("{}@{}", src.ups, src.host)];
	if let Some(l) = listed.iter().find(|l| named.contains(&l.label)) {
		return Ok(l.label.clone());
	}
	let prefix = format!("{}@", src.ups);
	let same: Vec<&str> = listed.iter().map(|l| l.label.as_str()).filter(|l| l.starts_with(&prefix)).collect();
	match same.as_slice() {
		[one] => Ok(one.to_string()),
		[] => {
			let all: Vec<&str> = listed.iter().map(|l| l.label.as_str()).collect();
			Err(format!("it has no journal of {}@{server} (it has {})", src.ups, if all.is_empty() { "none".into() } else { all.join(", ") }))
		}
		more => Err(format!("it has more than one journal of {} ({}) and none of {}@{server}", src.ups, more.join(", "), src.ups)),
	}
}

/// The last `cadence::WARM_UP` seconds before `until` (Unix seconds) of the journal of `src` at the
/// recorder `addr`.
pub fn recent(addr: &str, src: &NutSource, until: u64) -> Result<Recorded, String> {
	let addr: Addr = addr.parse()?;
	let mut c = Client::connect(&addr, timeout::RECORDER).map_err(|e| e.to_string())?;
	let label = label(&c.list().map_err(|e| e.to_string())?, src)?;
	let recent = c.get(&label, until.saturating_sub(cadence::WARM_UP) as f64).map_err(|e| e.to_string())?;
	c.quit();
	Ok(Recorded { label, recent })
}

/// The journal `label` at the recorder `addr`, read a line at a time from `since` to `until` (Unix
/// seconds) into the long history of the source `id` (unless `history` is off): returns its
/// timeline then, the first change at `since`. Gives up once `cancel` is set.
#[allow(clippy::too_many_arguments)]
pub fn backfill(addr: &str, label: &str, id: &str, history: bool, since: u64, until: u64, store: &Store, cancel: &AtomicBool) -> Result<Vec<Change>, String> {
	let addr: Addr = addr.parse()?;
	let stream = Stream::connect(&addr, timeout::RECORDER).map_err(|e| e.to_string())?;
	// A long journal may take the recorder a while to start sending.
	stream.timeouts(Some(timeout::RECORDER_LINE)).map_err(|e| e.to_string())?;
	let mut w = stream.try_clone().map_err(|e| e.to_string())?;
	if label.contains(char::is_whitespace) {
		return Err(format!("bad label {label:?}"));
	}
	w.write_all(format!("GET {label} {since}\n").as_bytes()).map_err(|e| e.to_string())?;
	let mut r = BufReader::new(stream);
	let mut line = String::new();
	let mut next = |line: &mut String| -> Result<(), String> {
		line.clear();
		match r.read_line(line) {
			Ok(0) => Err("the recorder closed the connection".into()),
			Ok(_) => Ok(()),
			Err(e) => Err(e.to_string()),
		}
	};
	next(&mut line)?;
	let head = line.trim_end();
	if let Some(e) = head.strip_prefix("ERR ") {
		return Err(format!("the recorder says {e}"));
	}
	if !head.starts_with("BEGIN ") {
		return Err(format!("unexpected reply {head:?}"));
	}
	let mut walk = Walk::new(id, history, since, until);
	let mut read: u64 = 0;
	loop {
		if cancel.load(Ordering::Relaxed) {
			return Err("stopped".into());
		}
		next(&mut line)?;
		let l = line.trim_end_matches(['\n', '\r']);
		if l == "END" {
			break;
		}
		let Some(rec) = Record::parse(l) else { continue };
		// What came after is the service's own.
		if rec.t >= until as f64 {
			break;
		}
		walk.take(&rec);
		read += 1;
		// Counted now and then: that takes a while too.
		if read.is_multiple_of(4096) && walk.made.len() >= limit::BACKFILL_POINTS {
			store.write(walk.points(false), crate::store::now());
		}
	}
	let changes = walk.end();
	store.write(walk.points(true), crate::store::now());
	Ok(changes)
}

/// A journal walked record by record: the numbers of each poll, and where the power came from.
struct Walk {
	id: String,
	history: bool,
	since: u64,
	until: u64,
	state: State,
	/// The time of the records being read, while later ones may still be of their poll.
	poll: Option<f64>,
	/// The time of the last record.
	last: Option<f64>,
	made: Made,
	/// The address of each variable taken, no more than `NUT_VARS_MAX`.
	addresses: BTreeMap<String, String>,
	changes: Vec<Change>,
}

impl Walk {
	fn new(id: &str, history: bool, since: u64, until: u64) -> Walk {
		Walk {
			id: id.to_string(),
			history,
			since,
			until,
			state: State::default(),
			poll: None,
			last: None,
			made: Made::default(),
			addresses: BTreeMap::new(),
			changes: Vec::new(),
		}
	}

	fn take(&mut self, r: &Record) {
		let t = self.last.map_or(r.t, |l| r.t.max(l));
		// A poll is over once a record of a later time comes, or one that ends the polls: the
		// recorder writes whole seconds, so a poll and a lost connection can share one.
		let ends = matches!(r.kind, Kind::Start | Kind::Down(_) | Kind::NoData(_));
		if let Some(p) = self.poll.filter(|p| *p != t || ends) {
			self.polled(p);
			self.poll = None;
		}
		if let Some(l) = self.last {
			if t - l > GAP || matches!(r.kind, Kind::Start) {
				// Nothing is known of a silence, nor of a time the recorder didn't run.
				self.power(l, Supply::None);
			} else {
				self.between(l, t);
			}
		}
		self.state.apply(r);
		self.poll = Some(t);
		self.last = Some(t);
	}

	/// What the poll at `t` read, all its records taken: the numbers, while upsd was read, and where
	/// the power came from.
	fn polled(&mut self, t: f64) {
		let up = self.state.feed == Feed::Up;
		let supply = match up {
			true => Supply::of(&Status::parse(self.state.vars.get("ups.status").map_or("", String::as_str))),
			false => Supply::None,
		};
		self.power(t, supply);
		if up {
			self.reading(t);
		}
	}

	/// Readings after `from` and before `to`, as the last poll had it, the journal writing only what
	/// changes: as often as the finest tier that keeps that time has points (every 10 seconds the
	/// last day before `until`, every 5 minutes the 30 days before, every hour before that).
	fn between(&mut self, from: f64, to: f64) {
		if !self.history || self.state.feed != Feed::Up {
			return;
		}
		let mut at = from as u64;
		loop {
			let kept = |t: &Tier| t.window().is_none_or(|w| at + w * t.step() >= self.until);
			let step = Tier::ALL.into_iter().find(kept).map_or(3600, Tier::step);
			at = (at / step + 1) * step;
			if at as f64 >= to {
				return;
			}
			self.reading(at as f64);
		}
	}

	/// The numbers the state has, as read at `t`, summed into the history's slots.
	fn reading(&mut self, t: f64) {
		if !self.history {
			return;
		}
		let Walk { id, state, addresses, made, .. } = self;
		for (var, v) in &state.vars {
			if addresses.len() < limit::NUT_VARS_MAX && !addresses.contains_key(var) && recordable(var) && parse_num(v).is_some() {
				addresses.insert(var.clone(), format!("{id}.{var}"));
			}
		}
		let readings = state.vars.iter().filter_map(|(var, v)| Some((addresses.get(var)?.as_str(), parse_num(v)?)));
		made.push(t as u64, readings);
	}

	/// The power came from `s` from `t` on; before `since`, as from `since`.
	fn power(&mut self, t: f64, s: Supply) {
		let t = (t as u64).max(self.since);
		if self.changes.last().is_some_and(|c| c.t == t) {
			self.changes.pop();
		}
		if self.changes.last().map(|c| c.state) != Some(s) {
			self.changes.push(Change::new(t, s, "-"));
		}
	}

	/// The last poll and the time after it, to `until`; the timeline from `since`, its first change
	/// then.
	fn end(&mut self) -> Vec<Change> {
		if let Some(p) = self.poll.take() {
			self.polled(p);
		}
		if let Some(l) = self.last {
			if self.until as f64 - l > GAP {
				self.power(l, Supply::None);
			} else {
				self.between(l, self.until as f64);
			}
		}
		if self.changes.first().is_none_or(|c| c.t > self.since) {
			self.changes.insert(0, Change::new(self.since, Supply::None, "-"));
		}
		std::mem::take(&mut self.changes)
	}

	/// The points made (with `all`, of the slots still being summed too), each tier's of the slots
	/// wholly from `since` to `until`: before, the service read the source itself, and after.
	fn points(&mut self, all: bool) -> Vec<(Tier, String, Vec<Point>)> {
		let (since, until) = (self.since, self.until);
		self.made.take(all, |tier| tier.slot(since.div_ceil(tier.step()) * tier.step())..tier.slot(until))
	}
}

#[cfg(test)]
pub mod tests {
	use std::io::{BufRead, BufReader, Write};
	use std::net::{SocketAddr, TcpListener, TcpStream};
	use std::path::PathBuf;
	use std::sync::atomic::{AtomicBool, Ordering};
	use std::sync::{Arc, Mutex};

	use upst_core::journal;

	use super::*;

	/// What `say` wrote, by source.
	pub static SAID: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

	/// What `say` wrote of the source `id`.
	pub fn said(id: &str) -> Vec<String> {
		SAID.lock().unwrap_or_else(|p| p.into_inner()).iter().filter(|(i, _)| i == id).map(|(_, l)| l.clone()).collect()
	}

	/// A recorder serving journal files over TCP on 127.0.0.1, as `upst record` does: `VER`, `LIST`,
	/// `GET <label> [<since>]`, `QUIT`. It stops when dropped.
	pub struct Recorder {
		pub addr: SocketAddr,
		stop: Arc<AtomicBool>,
	}

	impl Recorder {
		/// Serving `files`, each a label and its journal.
		pub fn serve(files: Vec<(String, PathBuf)>) -> Recorder {
			let listener = TcpListener::bind("127.0.0.1:0").unwrap();
			let addr = listener.local_addr().unwrap();
			let stop = Arc::new(AtomicBool::new(false));
			let stopped = stop.clone();
			std::thread::spawn(move || {
				for conn in listener.incoming() {
					if stopped.load(Ordering::SeqCst) {
						return;
					}
					let Ok(conn) = conn else { continue };
					let files = files.clone();
					std::thread::spawn(move || answer(conn, &files));
				}
			});
			Recorder { addr, stop }
		}
	}

	impl Drop for Recorder {
		fn drop(&mut self) {
			self.stop.store(true, Ordering::SeqCst);
			let _ = TcpStream::connect(self.addr);
		}
	}

	fn answer(conn: TcpStream, files: &[(String, PathBuf)]) {
		let mut w = conn.try_clone().unwrap();
		let mut r = BufReader::new(conn);
		let mut line = String::new();
		loop {
			line.clear();
			if r.read_line(&mut line).unwrap_or(0) == 0 {
				return;
			}
			let words: Vec<&str> = line.trim_end().split(' ').collect();
			let mut out = String::new();
			match words.as_slice() {
				["VER"] => out.push_str("upst 0.0.0-test journal 1\n"),
				["LIST"] => {
					out.push_str("BEGIN LIST\n");
					for (label, path) in files {
						let recs = journal::read(path).unwrap();
						out.push_str(&format!("UPS {label} {} {}\n", recs[0].t, recs[recs.len() - 1].t));
					}
					out.push_str("END LIST\n");
				}
				["GET", label, rest @ ..] => match files.iter().find(|(l, _)| l == label) {
					None => out.push_str("ERR UNKNOWN-UPS\n"),
					Some((_, path)) => {
						let since: f64 = rest.first().and_then(|s| s.parse().ok()).unwrap_or(0.0);
						out.push_str(&format!("BEGIN {label}\n"));
						for rec in journal::since(journal::read(path).unwrap(), since) {
							out.push_str(&format!("{rec}\n"));
						}
						out.push_str("END\n");
					}
				},
				["QUIT"] => {
					let _ = w.write_all(b"OK Goodbye\n");
					return;
				}
				_ => out.push_str("ERR UNKNOWN-COMMAND\n"),
			}
			if w.write_all(out.as_bytes()).is_err() {
				return;
			}
		}
	}

	fn src(text: &str) -> NutSource {
		let config = crate::settings::read_sources(&[crate::settings::Source::new("a.toml", format!("[sources.s]\nkind = \"nut\"\n{text}"))]).unwrap();
		config.sources["s"].clone()
	}

	#[test]
	fn the_journal_of_a_source_is_the_one_of_its_ups_and_server() {
		let listed = |labels: &[&str]| -> Vec<Listed> { labels.iter().map(|l| Listed { label: l.to_string(), first: 0.0, last: 1.0 }).collect() };
		let eco = src("ups = \"eco\"\n");
		assert_eq!(label(&listed(&["ups@localhost", "eco@localhost"]), &eco).unwrap(), "eco@localhost");
		assert_eq!(label(&listed(&["eco@localhost:3493"]), &eco).unwrap(), "eco@localhost:3493");
		// The only one of its name, wherever the recorder reads it.
		assert_eq!(label(&listed(&["eco@127.0.0.1", "ups@nas"]), &eco).unwrap(), "eco@127.0.0.1");
		let other = src("ups = \"eco\"\nhost = \"192.0.2.7\"\nport = 3500\n");
		assert_eq!(label(&listed(&["eco@192.0.2.7:3500", "eco@localhost"]), &other).unwrap(), "eco@192.0.2.7:3500");
		assert_eq!(label(&listed(&["eco@a", "eco@b"]), &other).unwrap_err(), "it has more than one journal of eco (eco@a, eco@b) and none of eco@192.0.2.7:3500");
		assert_eq!(label(&listed(&["ups@nas"]), &eco).unwrap_err(), "it has no journal of eco@localhost (it has ups@nas)");
		assert_eq!(label(&[], &eco).unwrap_err(), "it has no journal of eco@localhost (it has none)");
	}

	fn rec(t: u64, kind: Kind) -> Record {
		Record::new(t as f64, kind)
	}

	fn set(t: u64, k: &str, v: &str) -> Record {
		rec(t, Kind::Set(k.into(), v.into()))
	}

	#[test]
	fn a_walk_reads_the_polls_and_the_power_of_a_journal() {
		let mut w = Walk::new("eco", true, 1000, 2000);
		// The state before `since`, as a recorder's answer opens with; then a poll every 10 s, the
		// journal writing what changed; upsd gone; a silence; the recorder started again.
		let journal = [
			rec(990, Kind::Start),
			rec(990, Kind::Up),
			set(990, "ups.status", "OL"),
			set(990, "ups.realpower", "100"),
			set(990, "device.serial", "0000"),
			set(1010, "ups.realpower", "110"),
			set(1040, "ups.status", "OB DISCHRG"),
			set(1040, "ups.realpower", "120"),
			rec(1060, Kind::Down("refused".into())),
			rec(1100, Kind::Up),
			set(1100, "ups.status", "OL"),
			rec(1500, Kind::Start),
			rec(1500, Kind::Up),
			rec(1510, Kind::Tick),
		];
		for r in &journal {
			w.take(r);
		}
		let changes: Vec<(u64, &str)> = w.end().iter().map(|c| (c.t, c.state.as_str())).collect();
		// On line from `since`, on battery, upsd lost; the poll at 1100 is all that is known before the
		// silence, which is none from it; the recorder started again, then nothing to `until`.
		assert_eq!(changes, [(1000, "line"), (1040, "battery"), (1060, "none"), (1500, "line"), (1510, "none")]);
		let points = w.points(true);
		let s10: BTreeMap<u32, f32> = points.iter().find(|(t, a, _)| *t == Tier::S10 && a == "eco.ups.realpower").unwrap().2.iter().copied().collect();
		// From the slot that starts at `since`: a reading at each poll, and every 10 s between.
		assert_eq!(s10.iter().map(|(s, x)| (s * 10, *x)).collect::<Vec<_>>(), [(1000, 100.0), (1010, 110.0), (1020, 110.0), (1030, 110.0), (1040, 120.0), (1050, 120.0), (1100, 120.0), (1500, 120.0), (1510, 120.0)]);
		assert!(!points.iter().any(|(_, a, _)| a.contains("serial")), "not recordable");
	}

	#[test]
	fn between_polls_a_walk_reads_as_often_as_the_history_keeps_points_of_that_time() {
		let until = 40 * 86_400;
		// A minute of a station that doesn't change, two days and an hour before `until`, then a
		// minute in its last day.
		let journal = |t: u64| [rec(t, Kind::Start), rec(t, Kind::Up), set(t, "ups.realpower", "100"), rec(t + 60, Kind::Tick)];
		let mut w = Walk::new("eco", true, 0, until);
		for r in journal(until - 2 * 86_400 - 3600).iter().chain(&journal(until - 600)) {
			w.take(r);
		}
		w.end();
		let points = w.points(true);
		let s10: Vec<u32> = points.iter().find(|(t, a, _)| *t == Tier::S10 && a == "eco.ups.realpower").unwrap().2.iter().map(|p| p.0).collect();
		// Two days back: at the polls alone, the minute being in no 5-minute slot's start. In the
		// last day: every 10 seconds.
		let old = Tier::S10.slot(until - 2 * 86_400 - 3600);
		let new = Tier::S10.slot(until - 600);
		assert_eq!(s10, [old, old + 6, new, new + 1, new + 2, new + 3, new + 4, new + 5, new + 6]);
	}
}
