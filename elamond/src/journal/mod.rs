//! The systemd journal of `log` widgets (docs/config.md, The journal): a `journalctl` following
//! each journal a stream shows, with the last lines it printed kept in a tail, and pages of older
//! lines for `GET /api/v1/log` (`api`). A journalctl runs in a process group of its own, only while
//! some stream holds its tail: the last to let go kills it. A slow or stuck journalctl holds up
//! nothing but its own logs.

pub mod api;

use std::collections::{HashMap, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use serde_json::Value;
use tokio::time::Instant;

use crate::consts::{cadence, limit, timeout};
use crate::control::{read_some, Group};
use crate::settings::{Config, Widget};
use crate::ui::{Dashboard, Obj, Put, Ui, UiWidget};

/// What journalctl prints of each line: the rest the service has no use for.
const FIELDS: &str = "--output-fields=MESSAGE,PRIORITY,_SYSTEMD_UNIT,_SYSTEMD_USER_UNIT,SYSLOG_IDENTIFIER,_COMM";

/// The note of a log of the system journal while the service's user may read only its own of it.
pub const ONLY_OWN: &str = "Only this user's journal: add the user to systemd-journal to read the system's.";
/// The note of a log on a computer without journalctl.
pub const NO_JOURNALCTL: &str = "journalctl isn't there: the journal needs systemd's journalctl.";

/// Which journal: the system's (as far as the service's user may read it), that user's own, or
/// one unit's, of the system or of the user.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
	System,
	User,
	Unit { name: String, user: bool },
}

impl Key {
	/// The journal a `log` shows. A unit's is a link's unit, the link (the first by id that has it)
	/// saying whether the system's or the user's; none for a unit no link has.
	pub fn of(w: &Widget, config: &Config) -> Option<Key> {
		match w.scope.as_deref().unwrap_or("system") {
			"system" => Some(Key::System),
			"user" => Some(Key::User),
			"unit" => {
				let name = w.unit.as_deref().filter(|u| !u.is_empty())?;
				let link = config.links.values().find(|l| l.unit.as_deref() == Some(name))?;
				Some(Key::Unit { name: name.to_string(), user: link.scope() == "user" })
			}
			_ => None,
		}
	}

	/// journalctl's arguments that choose it.
	fn args(&self) -> Vec<String> {
		match self {
			Key::System => Vec::new(),
			Key::User => vec!["--user".into()],
			Key::Unit { name, user } => [user.then(|| "--user".to_string()), Some("-u".into()), Some(name.clone())].into_iter().flatten().collect(),
		}
	}

	/// Whether it is the system's journal or part of it, which the service's user may not read all of.
	fn of_system(&self) -> bool {
		matches!(self, Key::System | Key::Unit { user: false, .. })
	}

	/// What the service's own journal calls it.
	fn words(&self) -> String {
		match self {
			Key::System => "the system journal".into(),
			Key::User => "the user's journal".into(),
			Key::Unit { name, .. } => format!("{name}'s journal"),
		}
	}
}

/// The highest priority (the least important) a level shows: `warning` shows 0–4.
pub fn priority_of(level: &str) -> Option<u8> {
	Some(match level {
		"critical" => 2,
		"error" => 3,
		"warning" => 4,
		"notice" => 5,
		"info" => 6,
		"debug" => 7,
		_ => return None,
	})
}

/// The level of a priority: 0–2 `critical`, 3 `error`, 4 `warning`, 5 `notice`, 6 `info`, 7 `debug`.
fn level_of(priority: u8) -> &'static str {
	match priority {
		0..=2 => "critical",
		3 => "error",
		4 => "warning",
		5 => "notice",
		6 => "info",
		_ => "debug",
	}
}

/// The priority of a line without one.
const INFO: u8 = 6;

// ---- lines ----------------------------------------------------------------------------------

/// A line of the journal, as a client is sent it.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
	/// Counts the lines a tail took in, from its start.
	pub seq: u64,
	/// When journald took it in, Unix µs (`__REALTIME_TIMESTAMP`).
	pub us: u64,
	/// 0–7.
	pub priority: u8,
	/// The systemd unit it came from: the user's unit, else the system's.
	pub unit: Option<String>,
	/// The program's name: `SYSLOG_IDENTIFIER`, else the command.
	pub id: Option<String>,
	pub text: String,
}

impl Line {
	/// The line as a state's `lines` and `/log` have it.
	pub fn json(&self) -> Obj {
		let mut o = Obj::new();
		o.put("c", self.us);
		o.put("t", self.us / 1000);
		o.put("level", level_of(self.priority));
		o.opt("unit", self.unit.as_deref());
		o.opt("id", self.id.as_deref());
		o.put("text", self.text.as_str());
		o
	}

	/// The bytes it takes in a state.
	fn size(&self) -> usize {
		serde_json::to_string(&Value::Object(self.json())).map_or(0, |s| s.len())
	}
}

/// A line `journalctl -o json` printed, its `seq` 0; none for one that isn't JSON or has no time.
pub fn parse(bytes: &[u8]) -> Option<Line> {
	let o: serde_json::Map<String, Value> = serde_json::from_slice(bytes).ok()?;
	let field = |name: &str| field(o.get(name)?).filter(|s| !s.is_empty());
	Some(Line {
		seq: 0,
		us: field("__REALTIME_TIMESTAMP")?.trim().parse().ok()?,
		priority: field("PRIORITY").and_then(|p| p.trim().parse().ok()).filter(|p| *p <= 7).unwrap_or(INFO),
		unit: field("_SYSTEMD_USER_UNIT").or_else(|| field("_SYSTEMD_UNIT")),
		id: field("SYSLOG_IDENTIFIER").or_else(|| field("_COMM")),
		text: clean(&field("MESSAGE").unwrap_or_default()),
	})
}

/// A field's value as text: journalctl prints one that isn't UTF-8 as an array of its bytes, and a
/// field a line has more than once as an array of its values (the first is taken).
fn field(v: &Value) -> Option<String> {
	match v {
		Value::String(s) => Some(s.clone()),
		Value::Array(a) if !a.is_empty() && a.iter().all(Value::is_u64) => {
			let bytes: Vec<u8> = a.iter().filter_map(Value::as_u64).map(|b| b as u8).collect();
			Some(String::from_utf8_lossy(&bytes).into_owned())
		}
		Value::Array(a) => a.first().and_then(field),
		_ => None,
	}
}

/// A message as a client shows it: terminal colours (ANSI CSI sequences) and control characters
/// taken out but line breaks, a tab a space, at most `limit::LOG_TEXT_MAX` characters (cut with `…`).
pub fn clean(text: &str) -> String {
	let mut out = String::new();
	let mut chars = text.chars().peekable();
	while let Some(c) = chars.next() {
		match c {
			'\u{1b}' if chars.peek() == Some(&'[') => {
				chars.next();
				// Its parameters and intermediates, then the byte that ends it.
				for d in chars.by_ref() {
					if ('\u{40}'..='\u{7e}').contains(&d) {
						break;
					}
				}
			}
			'\t' => out.push(' '),
			'\n' => out.push('\n'),
			c if c.is_control() => {}
			c => out.push(c),
		}
	}
	let out = out.trim_end();
	if out.chars().count() <= limit::LOG_TEXT_MAX {
		return out.to_string();
	}
	out.chars().take(limit::LOG_TEXT_MAX - 1).chain(['…']).collect()
}

/// Lines out of what a pipe gives, however it comes: one longer than `limit::JOURNAL_LINE_MAX`
/// is skipped.
#[derive(Default)]
struct Splitter {
	line: Vec<u8>,
	long: bool,
}

impl Splitter {
	/// Each line `bytes` ends, without its line break.
	fn feed(&mut self, bytes: &[u8], mut each: impl FnMut(&[u8])) {
		for part in bytes.split_inclusive(|b| *b == b'\n') {
			let (body, ends) = match part.strip_suffix(b"\n") {
				Some(body) => (body, true),
				None => (part, false),
			};
			if !self.long {
				if self.line.len() + body.len() > limit::JOURNAL_LINE_MAX {
					self.long = true;
					self.line.clear();
				} else {
					self.line.extend_from_slice(body);
				}
			}
			if ends {
				if !self.long {
					each(&self.line);
				}
				self.line.clear();
				self.long = false;
			}
		}
	}
}

/// Which lines a client sees: of a priority up to `priority`, and, with a search, those whose
/// text, unit or program's name holds it, whatever the case.
#[derive(Clone, Debug, PartialEq)]
pub struct Filter {
	pub priority: u8,
	/// Lowercase.
	search: Option<String>,
}

impl Filter {
	pub fn new(priority: u8, search: Option<&str>) -> Filter {
		Filter { priority, search: search.filter(|s| !s.is_empty()).map(str::to_lowercase) }
	}

	pub fn matches(&self, l: &Line) -> bool {
		l.priority <= self.priority
			&& self.search.as_deref().is_none_or(|s| [Some(&l.text), l.unit.as_ref(), l.id.as_ref()].into_iter().flatten().any(|t| t.to_lowercase().contains(s)))
	}
}

// ---- a tail's lines, and a stream's states of them ------------------------------------------

/// What a tail has of the journal: the last lines its journalctl printed, and how that runs.
#[derive(Debug, Default)]
pub struct Buffer {
	lines: VecDeque<Line>,
	/// The `seq` of the next line.
	next: u64,
	/// Counts the journalctls started: a stream that saw another starts anew.
	run: u64,
	/// The journalctl has printed the journal's last lines, or has ended: the tail's logs have a state.
	ready: bool,
	/// Why there may be more lines than these, or none.
	note: Option<String>,
}

/// What a stream sent of a tail's lines: of which run, and the `seq` of the next it hasn't seen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sent {
	run: u64,
	next: u64,
}

impl Buffer {
	/// A journalctl started: what the one before left goes, and what it prints is new.
	pub fn start(&mut self, note: Option<String>) {
		self.run += 1;
		self.lines.clear();
		self.ready = false;
		self.note = note;
	}

	/// The journal's last lines are all in.
	pub fn ready(&mut self) {
		self.ready = true;
	}

	/// journalctl ended, or couldn't start, for `why`.
	pub fn ended(&mut self, why: String) {
		self.ready = true;
		self.note = Some(why);
	}

	/// A line journalctl printed; beyond `limit::JOURNAL_KEPT`, the oldest goes.
	pub fn push(&mut self, mut line: Line) {
		line.seq = self.next;
		self.next += 1;
		self.lines.push_back(line);
		while self.lines.len() > limit::JOURNAL_KEPT {
			self.lines.pop_front();
		}
	}

	/// The state of a log of these lines for a stream that sent `sent` of them (none: its first
	/// state, or one after its filter changed), as `filter` lets them through, a client keeping
	/// `keep` of them; `sent` then says what this one sends. None until the journal's last lines are in.
	pub fn state(&self, sent: &mut Option<Sent>, filter: &Filter, keep: usize) -> Option<Obj> {
		if !self.ready {
			return None;
		}
		let mut o = Obj::new();
		match sent.filter(|s| s.run == self.run) {
			// What the client has gives way to the newest lines that match.
			None => {
				let lines = newest(self.lines.iter(), filter, keep, limit::LOG_RESET_BYTES).0;
				o.put("lines", lines);
				o.put("reset", true);
			}
			// What came since, the newest that fit; those that didn't, and those that left the buffer
			// before they were sent, are counted.
			Some(s) => {
				let kept_from = self.lines.front().map_or(self.next, |l| l.seq);
				let gone = kept_from.saturating_sub(s.next);
				let (lines, skipped) = newest(self.lines.iter().filter(|l| l.seq >= s.next), filter, limit::LOG_NEW_LINES, limit::LOG_NEW_BYTES);
				o.put("lines", lines);
				let more = gone + skipped;
				if more > 0 {
					o.put("more", more);
				}
			}
		}
		o.opt("note", self.note.as_deref());
		*sent = Some(Sent { run: self.run, next: self.next });
		Some(o)
	}
}

/// Of `lines` (oldest first), the newest that `filter` lets through, `count` and `bytes` at most,
/// as JSON oldest first; and how many older ones matched.
fn newest<'a>(lines: impl DoubleEndedIterator<Item = &'a Line>, filter: &Filter, count: usize, bytes: usize) -> (Value, u64) {
	let (mut out, mut size, mut more) = (Vec::new(), 0, 0u64);
	for l in lines.rev().filter(|l| filter.matches(l)) {
		let s = l.size();
		if more == 0 && out.len() < count && size + s <= bytes {
			size += s;
			out.push(Value::Object(l.json()));
		} else {
			more += 1;
		}
	}
	out.reverse();
	(Value::Array(out), more)
}

// ---- tails ----------------------------------------------------------------------------------

/// A journalctl following one journal, while some stream holds it: dropped, it is killed.
pub struct Tail {
	buffer: Arc<Mutex<Buffer>>,
	task: Option<tokio::task::JoinHandle<()>>,
}

impl Tail {
	pub fn buffer(&self) -> MutexGuard<'_, Buffer> {
		self.buffer.lock().unwrap_or_else(|p| p.into_inner())
	}

	/// A tail without a journalctl, its lines put in by hand: the fixtures'.
	#[cfg(test)]
	pub fn idle() -> Tail {
		Tail { buffer: Arc::default(), task: None }
	}
}

impl Drop for Tail {
	fn drop(&mut self) {
		if let Some(t) = &self.task {
			t.abort();
		}
	}
}

/// The journals followed now, one tail each, and the pages of older lines asked for.
pub struct Journal {
	tails: Mutex<HashMap<Key, Weak<Tail>>>,
	/// journalctl: as PATH finds it (in the tests, `ELAMON_JOURNALCTL`, a script of theirs).
	pub program: PathBuf,
	/// Whether the service's user may read the system journal, asked as each journalctl starts.
	pub readable: fn() -> bool,
	/// Taken by each `GET /api/v1/log` while its journalctl runs.
	pages: tokio::sync::Semaphore,
}

impl Default for Journal {
	fn default() -> Journal {
		#[cfg(test)]
		let program = std::env::var_os("ELAMON_JOURNALCTL").map_or_else(|| PathBuf::from("journalctl"), PathBuf::from);
		#[cfg(not(test))]
		let program = PathBuf::from("journalctl");
		Journal { tails: Mutex::default(), program, readable: system_readable, pages: tokio::sync::Semaphore::new(limit::LOG_PAGES_AT_ONCE) }
	}
}

impl Journal {
	/// The tail of `key`'s journal: the one running, or one started now, on the runtime this is called on.
	pub fn tail(&self, key: &Key) -> Arc<Tail> {
		let mut tails = self.tails.lock().unwrap_or_else(|p| p.into_inner());
		tails.retain(|_, t| t.strong_count() > 0);
		if let Some(t) = tails.get(key).and_then(Weak::upgrade) {
			return t;
		}
		let buffer = Arc::new(Mutex::new(Buffer::default()));
		let task = tokio::spawn(run(key.clone(), self.program.clone(), self.readable, buffer.clone()));
		let tail = Arc::new(Tail { buffer, task: Some(task) });
		tails.insert(key.clone(), Arc::downgrade(&tail));
		tail
	}

	/// Journals followed now.
	#[cfg(test)]
	pub fn followed(&self) -> usize {
		self.tails.lock().unwrap().values().filter(|t| t.strong_count() > 0).count()
	}
}

fn lock(buffer: &Mutex<Buffer>) -> MutexGuard<'_, Buffer> {
	buffer.lock().unwrap_or_else(|p| p.into_inner())
}

/// Follows `key`'s journal into `buffer` for as long as the tail is held: a journalctl that ends is
/// started again `cadence::JOURNAL_RESTART` after it last started.
async fn run(key: Key, program: PathBuf, readable: fn() -> bool, buffer: Arc<Mutex<Buffer>>) {
	let mut said = None;
	loop {
		let started = Instant::now();
		let why = follow(&key, &program, readable, &buffer).await;
		// Said once, not every time it is started again for the same.
		if said.as_ref() != Some(&why) {
			eprintln!("journal: {}: {why}", key.words());
			said = Some(why.clone());
		}
		lock(&buffer).ended(why);
		tokio::time::sleep_until(started + cadence::JOURNAL_RESTART).await;
	}
}

/// journalctl with `args`, in a process group of its own, killed as it is dropped.
fn spawn(program: &Path, args: &[String]) -> io::Result<tokio::process::Child> {
	tokio::process::Command::new(program)
		.args(args)
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.process_group(0)
		.kill_on_drop(true)
		.spawn()
}

/// Why journalctl couldn't start.
fn cant_start(e: &io::Error) -> String {
	if e.kind() == io::ErrorKind::NotFound {
		return NO_JOURNALCTL.into();
	}
	let s = e.to_string();
	format!("journalctl can't start: {}", s.split(" (os error").next().unwrap_or(&s))
}

/// Why journalctl failed: the first line of its error (not a hint it gives as it starts), or how it
/// exited.
fn why_failed(err: &[u8], status: Option<io::Result<ExitStatus>>) -> String {
	let text = String::from_utf8_lossy(err);
	match text.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with("Hint:")) {
		Some(line) => line.to_string(),
		None => match status.and_then(Result::ok).and_then(|s| s.code()) {
			Some(code) => format!("exit code {code}"),
			None => "killed".into(),
		},
	}
}

/// Runs a journalctl following `key`'s journal, its lines into `buffer`, until it ends; why it did.
async fn follow(key: &Key, program: &Path, readable: fn() -> bool, buffer: &Mutex<Buffer>) -> String {
	let mut args = key.args();
	args.extend(["-f", "-o", "json", "-n", &limit::JOURNAL_KEPT.to_string(), "--no-pager", FIELDS].map(String::from));
	let mut child = match spawn(program, &args) {
		Ok(c) => c,
		Err(e) => return cant_start(&e),
	};
	let mut group = Group(child.id().map(|id| id as i32));
	lock(buffer).start((key.of_system() && !readable()).then(|| ONLY_OWN.to_string()));
	let (mut out, mut err) = (child.stdout.take(), child.stderr.take());
	let (mut out_buf, mut err_buf) = (vec![0u8; 64 * 1024], [0u8; 4096]);
	let mut splitter = Splitter::default();
	let mut errors = Vec::new();
	let mut status = None;
	// The journal's last lines are in once journalctl has printed as many, or has said nothing for a
	// moment, or has run for a while.
	let begun = Instant::now();
	let mut heard = begun;
	let (mut read, mut ready) = (0, false);
	while status.is_none() || out.is_some() || err.is_some() {
		let ready_at = (heard + timeout::JOURNAL_QUIET).min(begun + timeout::JOURNAL_START);
		tokio::select! {
			n = read_some(&mut out, &mut out_buf) => {
				let mut b = lock(buffer);
				splitter.feed(&out_buf[..n], |l| {
					read += 1;
					if let Some(line) = parse(l) {
						b.push(line);
					}
				});
				heard = Instant::now();
				if !ready && read >= limit::JOURNAL_KEPT {
					ready = true;
					b.ready();
				}
			}
			n = read_some(&mut err, &mut err_buf) => {
				let room = limit::FEED_STDERR_MAX.saturating_sub(errors.len()).min(n);
				errors.extend_from_slice(&err_buf[..room]);
			}
			_ = tokio::time::sleep_until(ready_at), if !ready => {
				ready = true;
				lock(buffer).ready();
			}
			s = child.wait(), if status.is_none() => {
				status = Some(s);
				// What it started goes with it; what it wrote is in the pipes still.
				drop(std::mem::replace(&mut group, Group(None)));
			}
		}
	}
	format!("journalctl ended: {}", why_failed(&errors, status))
}

/// Whether the service's user may read the system journal: the system's own file of it, where
/// journald keeps it on disk or in memory, as `systemd-journal`, `adm` or `wheel` (by ACL) let it.
pub fn system_readable() -> bool {
	let Ok(id) = std::fs::read_to_string("/etc/machine-id") else { return false };
	["/var/log/journal", "/run/log/journal"].iter().any(|dir| {
		let Ok(path) = std::ffi::CString::new(format!("{dir}/{}/system.journal", id.trim())) else { return false };
		// SAFETY: access(2) of a path we own, which it only reads.
		unsafe { libc::access(path.as_ptr(), libc::R_OK) == 0 }
	})
}

// ---- a stream's logs ------------------------------------------------------------------------

/// The `log`s of `dashboard` and of `ui`'s header, in order, each with its journal.
pub fn on_screen<'a>(ui: &'a Ui, dashboard: &'a Dashboard, config: &Config) -> Vec<(&'a UiWidget, Key)> {
	let header = ui.header.as_deref().filter(|h| *h != dashboard.id).and_then(|h| ui.dashboard(h));
	let widgets = std::iter::once(dashboard).chain(header).flat_map(|d| d.all_widgets()).filter(|w| w.kind == "log");
	widgets.filter_map(|w| Some((w, Key::of(&w.def, config)?))).collect()
}

/// A filter a client asked a log for: a level of its own (else the widget's) and a search.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Asked {
	pub priority: Option<u8>,
	pub search: Option<String>,
}

/// A `log` on a stream's screen, with the tail of its journal.
struct Shown {
	id: String,
	key: Key,
	/// The widget's own level, as a priority, and the lines a client keeps of it.
	own: u8,
	keep: usize,
	tail: Arc<Tail>,
	/// What the stream sent of it; none before its first state, or once it is to start anew.
	sent: Option<Sent>,
}

/// The `log`s a stream shows, and the filters its client asked for, by widget id (kept, as a
/// table's order is, from one dashboard to another).
#[derive(Default)]
pub struct Logs {
	on: Vec<Shown>,
	asked: HashMap<String, Asked>,
}

impl Logs {
	/// Takes the `log`s on screen now, in order, with their journals: one shown before keeps its tail
	/// and what was sent of it, a new one gets its tail of `tail`, and one gone lets go of its own.
	pub fn show(&mut self, widgets: Vec<(&UiWidget, Key)>, tail: impl Fn(&Key) -> Arc<Tail>) {
		let mut before = std::mem::take(&mut self.on);
		for (w, key) in widgets {
			let own = w.def.level.as_deref().and_then(priority_of).unwrap_or(INFO);
			let keep = w.def.lines.unwrap_or(limit::LOG_LINES) as usize;
			match before.iter().position(|s| s.id == w.id && s.key == key && s.own == own && s.keep == keep) {
				Some(i) => self.on.push(before.swap_remove(i)),
				None => self.on.push(Shown { id: w.id.clone(), tail: tail(&key), key, own, keep, sent: None }),
			}
		}
	}

	/// The filter a client asked the log `id` for; `None`, its own level and no search, back. Its
	/// next state starts anew.
	pub fn ask(&mut self, id: &str, asked: Option<Asked>) {
		match asked {
			Some(a) => self.asked.insert(id.to_string(), a),
			None => self.asked.remove(id),
		};
		if let Some(s) = self.on.iter_mut().find(|s| s.id == id) {
			s.sent = None;
		}
	}

	/// Adds the states of the logs on screen to the `data` line `data`, after the reading's; none
	/// of a log whose journal's last lines aren't in yet.
	pub fn put(&mut self, data: &mut Obj) {
		let Some(Value::Object(widgets)) = data.get_mut("widgets") else { return };
		for s in &mut self.on {
			let asked = self.asked.get(&s.id);
			let filter = Filter::new(asked.and_then(|a| a.priority).unwrap_or(s.own), asked.and_then(|a| a.search.as_deref()));
			if let Some(state) = s.tail.buffer().state(&mut s.sent, &filter, s.keep) {
				widgets.insert(s.id.clone(), Value::Object(state));
			}
		}
	}
}

#[cfg(test)]
pub mod tests;
