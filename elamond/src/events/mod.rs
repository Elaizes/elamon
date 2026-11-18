//! Events: what the rules of the settings make of the computer's values and of what a UPS says
//! (`rules`), what `elamond notify` adds, the file they are kept in, the lines a `log` of scope
//! `events` shows, and the ntfy topics they are sent on to (`ntfy`). What a rule is, how many
//! events are kept, where they go and how they read is all the settings' (`def`).

pub mod cli;
pub mod def;
pub mod ntfy;
pub mod rules;

use std::collections::VecDeque;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

use crate::journal::{Buffer, Filter, Line};
use crate::settings::Config;
use def::Lvl;

/// The events, in the data directory, one JSON object a line: an event, or how sending one went.
pub const FILE: &str = "events.jsonl";
/// What `elamond notify` leaves for the service, and the lock both take around it.
pub const SPOOL_FILE: &str = "notify.jsonl";
const SPOOL_LOCK: &str = "notify.lock";
/// A line of events.jsonl or of the spool longer than this isn't one of ours.
const LINE_MAX: usize = 16 * 1024;
/// The spool is read up to this: a script that fills it faster than the service takes it loses the rest.
const SPOOL_MAX: u64 = 256 * 1024;

/// Whether an event begins a trouble, ends one, or stands alone.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
	Start,
	End,
	Once,
}

/// An event as it is kept.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Event {
	/// Counts the events, from the first kept.
	pub n: u64,
	/// Unix ms.
	pub t: u64,
	pub level: Lvl,
	/// The level of the trouble it is of: its own, or, for an end, its start's. What a target's
	/// `level` is held against.
	pub of: Lvl,
	/// The rule's id.
	pub rule: String,
	pub code: String,
	pub phase: Phase,
	/// With names, and without.
	pub text: String,
	pub plain: String,
	/// `elamond notify --title`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub title: Option<String>,
}

/// An event a rule or `elamond notify` made, before it is kept; with the priority and tags its
/// rule gives it over a target's.
#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
	pub t: u64,
	pub level: Lvl,
	pub of: Lvl,
	pub rule: String,
	pub code: String,
	pub phase: Phase,
	pub text: String,
	pub plain: String,
	pub title: Option<String>,
	pub priority: Option<u8>,
	pub tags: Option<Vec<String>>,
}

/// How sending the event `sent` to a target went: "sent", or why not.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Delivery {
	pub sent: u64,
	pub t: u64,
	pub target: String,
	pub result: String,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum Kept {
	Event(Event),
	Delivery(Delivery),
}

/// Text fit for a line: no control or unseen characters, `max` characters at most.
pub fn clean(s: &str, max: usize) -> String {
	let s: String = s.chars().filter(|c| c.is_control() || !crate::store::unseen(*c)).map(|c| if c.is_control() { ' ' } else { c }).take(max).collect();
	s.trim().to_string()
}

/// The events kept, in memory and in their file.
struct Log {
	path: PathBuf,
	entries: VecDeque<(Event, Vec<Delivery>)>,
	/// Lines in the file, to know when to write it anew.
	lines: usize,
	next: u64,
	told: bool,
}

impl Log {
	fn load(dir: &Path) -> Log {
		let path = dir.join(FILE);
		let mut log = Log { path, entries: VecDeque::new(), lines: 0, next: 1, told: false };
		let text = match fs::read_to_string(&log.path) {
			Ok(t) => t,
			Err(e) => {
				if e.kind() != io::ErrorKind::NotFound {
					eprintln!("events: can't read {}: {e}; starting anew", log.path.display());
				}
				return log;
			}
		};
		// A line that doesn't parse is skipped, and gone when the file is next written anew.
		for line in text.lines().filter(|l| !l.trim().is_empty()) {
			log.lines += 1;
			match serde_json::from_str::<Kept>(line).ok().filter(|_| line.len() <= LINE_MAX) {
				Some(Kept::Event(e)) => {
					log.next = log.next.max(e.n + 1);
					log.entries.push_back((e, Vec::new()));
				}
				Some(Kept::Delivery(d)) => {
					if let Some((_, sent)) = log.entries.iter_mut().rev().find(|(e, _)| e.n == d.sent) {
						sent.push(d);
					}
				}
				None => {}
			}
		}
		log
	}

	/// Lets go of what is past `keep` events or `days` days; whether anything went.
	fn trim(&mut self, keep: usize, days: u64, now: u64) -> bool {
		let oldest = now.saturating_sub(days.saturating_mul(86_400_000));
		let before = self.entries.len();
		while self.entries.len() > keep || self.entries.front().is_some_and(|(e, _)| e.t < oldest) {
			self.entries.pop_front();
		}
		self.entries.len() != before
	}

	fn write(&mut self, line: &Kept, keep: usize) {
		// Written anew once it holds three times what is kept: deliveries are lines too.
		let written = if self.lines + 1 > keep.max(self.entries.len()).saturating_mul(3) {
			let mut text = Vec::new();
			let mut lines = 0;
			for (e, sent) in &self.entries {
				for k in std::iter::once(Kept::Event(e.clone())).chain(sent.iter().cloned().map(Kept::Delivery)) {
					text.extend(serde_json::to_vec(&k).unwrap_or_default());
					text.push(b'\n');
					lines += 1;
				}
			}
			crate::store::write_private(&self.path, &text).map(|()| self.lines = lines)
		} else {
			append(&self.path, line).map(|()| self.lines += 1)
		};
		match written {
			Ok(()) => self.told = false,
			Err(e) if !self.told => {
				self.told = true;
				eprintln!("events: can't write {}: {e}; they are kept in memory meanwhile", self.path.display());
			}
			Err(_) => {}
		}
	}
}

/// One line more in a file only its owner reads, in one write.
fn append(path: &Path, line: &impl Serialize) -> io::Result<()> {
	if let Some(dir) = path.parent() {
		crate::store::private_dir(dir)?;
	}
	let mut bytes = serde_json::to_vec(line).map_err(io::Error::other)?;
	bytes.push(b'\n');
	fs::OpenOptions::new().append(true).create(true).mode(0o600).open(path)?.write_all(&bytes)
}

/// The priority of the journal (`journal::priority_of`) an event's level shows as in a `log`.
fn priority(level: Lvl) -> u8 {
	crate::journal::priority_of(level.as_str()).unwrap_or(6)
}

/// An event as a `log` shows it: the rule in the program's place, the text with names.
fn line(e: &Event) -> Line {
	Line { seq: 0, us: e.t * 1000 + e.n % 1000, priority: priority(e.level), unit: None, id: Some(e.rule.clone()), text: e.text.clone() }
}

/// The service's events: kept, shown and handed to the targets.
pub struct Events {
	log: Mutex<Log>,
	/// What a `log` of scope `events` shows: the last events, as lines.
	buffer: Arc<Mutex<Buffer>>,
	dir: PathBuf,
	/// The settings as the targets go by them, and the events for them.
	settings: tokio::sync::watch::Sender<Arc<Config>>,
	out: Mutex<Option<tokio::sync::mpsc::UnboundedSender<Outgoing>>>,
}

/// An event kept, with what its rule says of sending it.
pub struct Outgoing {
	pub event: Event,
	pub priority: Option<u8>,
	pub tags: Option<Vec<String>>,
}

impl Events {
	/// The events kept in `dir`, as `config` keeps them.
	pub fn load(dir: &Path, config: Arc<Config>) -> Events {
		let mut log = Log::load(dir);
		log.trim(config.events.keep, config.events.keep_days, crate::activity::now_ms());
		let mut buffer = Buffer::default();
		for (e, _) in &log.entries {
			buffer.push(line(e));
		}
		buffer.ready();
		Events { log: Mutex::new(log), buffer: Arc::new(Mutex::new(buffer)), dir: dir.to_path_buf(), settings: tokio::sync::watch::channel(config).0, out: Mutex::new(None) }
	}

	fn log(&self) -> MutexGuard<'_, Log> {
		self.log.lock().unwrap_or_else(|p| p.into_inner())
	}

	/// Starts sending events on to the targets of the settings, on `handle`'s runtime; `computer`
	/// is the computer's name, for a title with names.
	pub fn start(self: &Arc<Self>, handle: &tokio::runtime::Handle, computer: &str) {
		let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
		*self.out.lock().unwrap_or_else(|p| p.into_inner()) = Some(tx);
		handle.spawn(ntfy::run(rx, self.settings.subscribe(), Arc::downgrade(self), computer.to_string(), "curl".into()));
	}

	/// Newly read settings: what is kept, and where events go from now on.
	pub fn apply(&self, config: Arc<Config>) {
		self.settings.send_replace(config);
	}

	/// Keeps `draft`, shows it and hands it to the targets.
	pub fn record(&self, draft: Draft, config: &Config) -> Event {
		let mut log = self.log();
		let event = Event {
			n: log.next,
			t: draft.t,
			level: draft.level,
			of: draft.of,
			rule: draft.rule,
			code: draft.code,
			phase: draft.phase,
			text: clean(&draft.text, crate::consts::limit::LOG_TEXT_MAX),
			plain: clean(&draft.plain, crate::consts::limit::LOG_TEXT_MAX),
			title: draft.title.map(|t| clean(&t, crate::consts::limit::NAME_MAX)),
		};
		log.next += 1;
		log.entries.push_back((event.clone(), Vec::new()));
		log.trim(config.events.keep, config.events.keep_days, event.t);
		log.write(&Kept::Event(event.clone()), config.events.keep);
		drop(log);
		self.buffer.lock().unwrap_or_else(|p| p.into_inner()).push(line(&event));
		eprintln!("event {}: {} {}: {}", event.n, event.level.as_str(), event.code, event.text);
		if let Some(out) = self.out.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
			let _ = out.send(Outgoing { event: event.clone(), priority: draft.priority, tags: draft.tags });
		}
		event
	}

	/// How sending the event `n` to `target` went.
	pub fn delivered(&self, n: u64, target: &str, result: &str) {
		let d = Delivery { sent: n, t: crate::activity::now_ms(), target: target.to_string(), result: clean(result, crate::consts::limit::WHAT_MAX) };
		let keep = self.settings.borrow().events.keep;
		let mut log = self.log();
		let Some((_, sent)) = log.entries.iter_mut().rev().find(|(e, _)| e.n == n) else { return };
		sent.push(d.clone());
		log.write(&Kept::Delivery(d), keep);
	}

	/// The last `n` events, newest first, each with how sending it went.
	pub fn newest(&self, n: usize) -> Vec<(Event, Vec<Delivery>)> {
		self.log().entries.iter().rev().take(n).cloned().collect()
	}

	/// The tail a `log` of scope `events` holds: these events' lines, no journalctl behind them.
	pub fn tail(&self) -> Arc<crate::journal::Tail> {
		Arc::new(crate::journal::Tail::of(self.buffer.clone()))
	}

	/// Events before `before` (a line's `c`; the newest without) that `filter` lets through,
	/// `count` at most, oldest first, and the `c` to go on from, none once there are no more.
	pub fn page(&self, before: Option<u64>, filter: &Filter, count: usize) -> (Vec<Line>, Option<u64>) {
		let log = self.log();
		let mut lines = Vec::new();
		let mut more = None;
		for l in log.entries.iter().rev().map(|(e, _)| line(e)).filter(|l| before.is_none_or(|b| l.us < b)).filter(|l| filter.matches(l)) {
			if lines.len() == count {
				more = lines.last().map(|l: &Line| l.us);
				break;
			}
			lines.push(l);
		}
		lines.reverse();
		(lines, more)
	}

	/// What `elamond notify` left since the last look, as events, as `[events.notify]` takes them.
	pub fn take_notified(&self, config: &Config, window: &mut VecDeque<u64>) {
		let now = crate::activity::now_ms();
		let def = &config.events.notify;
		let mut dropped = 0;
		for n in take_spool(&self.dir) {
			while window.front().is_some_and(|t| now.saturating_sub(*t) >= def.per * 1000) {
				window.pop_front();
			}
			if window.len() >= def.burst as usize {
				dropped += 1;
				continue;
			}
			window.push_back(now);
			let level = n.level.unwrap_or(def.level);
			let text = clean(&n.text, def.length);
			self.record(
				Draft { t: n.t, level, of: level, rule: def.rule.clone(), code: def.code.clone(), phase: Phase::Once, plain: text.clone(), text, title: n.title, priority: None, tags: None },
				config,
			);
		}
		if dropped > 0 {
			eprintln!("events: {dropped} of `elamond notify` dropped: more than {} in {} s ([events.notify])", def.burst, def.per);
		}
	}
}

// ---- `elamond notify`'s spool ---------------------------------------------------------------

/// What `elamond notify` asks for.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Notified {
	pub t: u64,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub level: Option<Lvl>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub title: Option<String>,
	pub text: String,
}

fn spool_lock(dir: &Path) -> io::Result<fs::File> {
	crate::store::private_dir(dir)?;
	let f = fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).mode(0o600).open(dir.join(SPOOL_LOCK))?;
	f.lock()?;
	Ok(f)
}

/// Leaves `n` for the service that uses `dir`.
pub fn spool(dir: &Path, n: &Notified) -> io::Result<()> {
	let _lock = spool_lock(dir)?;
	append(&dir.join(SPOOL_FILE), n)
}

/// What was left in `dir`, taken out of it. A line that isn't one of `elamond notify`'s is dropped.
fn take_spool(dir: &Path) -> Vec<Notified> {
	let path = dir.join(SPOOL_FILE);
	// Looked at every tick: nothing more than this while there is nothing.
	if !fs::metadata(&path).is_ok_and(|m| m.len() > 0) {
		return Vec::new();
	}
	let Ok(_lock) = spool_lock(dir) else { return Vec::new() };
	let text = fs::File::open(&path).and_then(|f| {
		use std::io::Read;
		let mut text = String::new();
		f.take(SPOOL_MAX).read_to_string(&mut text).map(|_| text)
	});
	let _ = fs::OpenOptions::new().write(true).truncate(true).open(&path);
	text.unwrap_or_default().lines().filter(|l| l.len() <= LINE_MAX).filter_map(|l| serde_json::from_str(l).ok()).collect()
}

#[cfg(test)]
mod tests;
