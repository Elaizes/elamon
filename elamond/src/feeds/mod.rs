//! The sources of kind `command`, `http` and `file` (docs/config.md, Sources): what a program
//! prints, a web service answers or a file holds. Each is read by a task of its own every
//! `interval` while it is needed — a client has on screen a dashboard that names one of its values,
//! a graph or a spark of the settings shows one, or its history is kept — and not at all
//! otherwise. Each read leaves a snapshot that the collector takes at its tick: a slow command
//! never holds up a reading, a stream or another source.

pub mod cli;
mod parse;
mod read;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::watch;

use crate::consts::{limit, timeout};
use crate::settings::{Config, Feed};
use crate::values::{Scalar, Values, V};

use read::{read, Outcome};

/// Why a read failed: `why`, which the journal says too, and `detail`, which only clients get (a
/// command's first line of stderr, curl's words).
#[derive(Clone, Debug, PartialEq)]
pub struct Failure {
	pub why: String,
	pub detail: Option<String>,
}

impl Failure {
	pub fn new(why: impl Into<String>) -> Failure {
		Failure { why: why.into(), detail: None }
	}

	pub fn with(why: impl Into<String>, detail: impl Into<String>) -> Failure {
		Failure { why: why.into(), detail: Some(detail.into()).filter(|d| !d.is_empty()) }
	}

	/// What `<id>.error` says.
	pub fn text(&self) -> String {
		match &self.detail {
			Some(d) => format!("{}: {d}", self.why),
			None => self.why.clone(),
		}
	}
}

/// A value as a source's: no unit, its format by what it is.
fn scalar(v: V) -> Scalar {
	let format = match v {
		V::Int(_) | V::Float(_) => "number",
		V::Text(_) => "text",
		V::Bool(_) => "bool",
	};
	Scalar { v, unit: "", format, limits: None, max: None, level: None }
}

/// What a source has, at its last read: what the collector puts among the values.
#[derive(Clone, Debug, Default)]
pub struct FeedSnapshot {
	/// The values of its last read, by address (`app.queue.length`), while it reads well.
	pub scalars: BTreeMap<String, Scalar>,
	/// Every address it has had since it started, with the format it had.
	pub known: BTreeMap<String, &'static str>,
	/// Its numbers, for the long history.
	pub kept: Vec<String>,
	/// When it last read well.
	pub ok_at: Option<SystemTime>,
	/// Why its last read failed, while it fails.
	pub error: Option<String>,
	/// Its command's exit code, after a run that ended by itself.
	pub exit: Option<i32>,
	/// Reads made, which the tests count.
	#[cfg_attr(not(test), allow(dead_code))]
	pub reads: u64,
}

/// A source's reads as they come: its snapshot, and what the journal was told.
struct Keeper {
	id: String,
	snap: FeedSnapshot,
	/// Its values among `known`, besides its own `.age`, `.exit` and `.error`.
	known_values: usize,
	failing: bool,
	/// The journal said values were left out.
	cut_said: bool,
}

impl Keeper {
	fn new(id: &str) -> Keeper {
		Keeper { id: id.to_string(), snap: FeedSnapshot::default(), known_values: 0, failing: false, cut_said: false }
	}

	/// Takes in a read that ended at `at`; what the journal is to say of it: that the source
	/// started failing, or reads again, or left values out, once each.
	fn took(&mut self, o: Outcome, at: SystemTime) -> Vec<String> {
		let id = self.id.clone();
		let mut say = Vec::new();
		let s = &mut self.snap;
		s.reads += 1;
		s.exit = o.exit;
		if o.cut && !self.cut_said {
			self.cut_said = true;
			let (most, deep) = (limit::FEED_VALUES_MAX, limit::FEED_DEPTH_MAX);
			say.push(format!("source {id}: more than {most} values, or some more than {deep} levels deep; those are left out"));
		}
		match o.result {
			Ok(values) => {
				if std::mem::take(&mut self.failing) {
					say.push(format!("source {id}: reads again"));
				}
				s.error = None;
				s.ok_at = Some(at);
				s.scalars = values.into_iter().map(|(path, v)| (format!("{id}.{path}"), scalar(v))).collect();
				s.kept = s.scalars.iter().filter(|(_, x)| x.v.num().is_some()).map(|(a, _)| a.clone()).collect();
			}
			Err(f) => {
				if !std::mem::replace(&mut self.failing, true) {
					say.push(format!("source {id}: {}", f.why));
				}
				s.error = Some(f.text());
				s.scalars.clear();
				s.kept.clear();
			}
		}
		for (a, x) in &s.scalars {
			if !s.known.contains_key(a) && self.known_values < limit::FEED_KNOWN_MAX {
				self.known_values += 1;
				s.known.insert(a.clone(), x.format);
			} else if let Some(f) = s.known.get_mut(a) {
				*f = x.format;
			}
		}
		for (own, format, had) in [("age", "duration", s.ok_at.is_some()), ("exit", "number", s.exit.is_some()), ("error", "text", s.error.is_some())] {
			if had {
				s.known.insert(format!("{id}.{own}"), format);
			}
		}
		say
	}
}

/// The sources' values, `<id>.age` as of the reading's `t`, the addresses they have had and the
/// numbers the history keeps; and the role of each source of `config`, from the start.
pub fn put(values: &mut Values, config: &Config, snapshots: &BTreeMap<String, Arc<FeedSnapshot>>) {
	for (id, feed) in &config.feeds {
		values.feeds.insert(id.clone(), feed.role);
		let Some(s) = snapshots.get(id) else { continue };
		values.scalars.extend(s.scalars.iter().map(|(a, x)| (a.clone(), x.clone())));
		if let Some(code) = s.exit {
			values.scalars.insert(format!("{id}.exit"), scalar(V::Int(code.into())));
		}
		if let Some(e) = &s.error {
			values.scalars.insert(format!("{id}.error"), scalar(V::Text(e.clone())));
		}
		if let Some(ok) = s.ok_at {
			let ms = ok.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
			let age = V::Int((values.t.saturating_sub(ms) / 1000).try_into().unwrap_or(i64::MAX));
			values.scalars.insert(format!("{id}.age"), Scalar { unit: "s", format: "duration", ..scalar(age) });
		}
		values.known.extend(s.known.iter().map(|(a, f)| (a.clone(), *f)));
		if feed.history {
			values.kept.extend(s.kept.iter().cloned());
		}
	}
}

// ---- the sources, running -------------------------------------------------------------------

/// One source, running: its settings, its last snapshot and whether it is needed. Dropped, its
/// task is aborted, and a read in flight with it: its command or curl and what they started are
/// killed.
struct Running {
	feed: Feed,
	snapshot: Arc<Mutex<Arc<FeedSnapshot>>>,
	wanted: watch::Sender<bool>,
	task: tokio::task::JoinHandle<()>,
}

impl Drop for Running {
	fn drop(&mut self) {
		self.task.abort();
	}
}

/// Every `command`, `http` and `file` source of the settings, each read by a task of its own.
#[derive(Default)]
pub struct Runtime {
	inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
	/// Where the sources' tasks run, once the service has started them.
	handle: Option<tokio::runtime::Handle>,
	feeds: BTreeMap<String, Running>,
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

	/// Takes newly read settings: a source taken out of them, or changed in any way, is stopped;
	/// one new or changed starts, read once, then as `set_demand` says. Before `start`, nothing runs.
	pub fn apply(&self, config: &Config) {
		let mut inner = self.lock();
		let Some(handle) = inner.handle.clone() else { return };
		inner.feeds.retain(|id, r| config.feeds.get(id) == Some(&r.feed));
		for (id, feed) in &config.feeds {
			if !inner.feeds.contains_key(id) {
				inner.feeds.insert(id.clone(), spawn(&handle, id, feed));
			}
		}
	}

	/// Each source's last snapshot.
	pub fn snapshots(&self) -> BTreeMap<String, Arc<FeedSnapshot>> {
		let inner = self.lock();
		inner.feeds.iter().map(|(id, r)| (id.clone(), r.snapshot.lock().unwrap_or_else(|p| p.into_inner()).clone())).collect()
	}

	/// Stops every source, a read in flight killed, and waits a moment for their tasks to end: as
	/// the service stops.
	pub async fn stop(&self) {
		let feeds = std::mem::take(&mut self.lock().feeds);
		for mut r in feeds.into_values() {
			r.task.abort();
			let _ = tokio::time::timeout(timeout::FEED_STOP, &mut r.task).await;
		}
	}

	/// Which sources are needed now, to be read every `interval` (besides the one read each has as
	/// it starts): those whose ids `heads` has (what the clients have on screen,
	/// `Hub::wanted_heads`), those a graph or a spark tracks one of the values of (`tracked`), and
	/// those whose history is kept.
	pub fn set_demand(&self, heads: &BTreeSet<String>, tracked: &BTreeSet<String>) {
		let inner = self.lock();
		for (id, r) in &inner.feeds {
			let on = r.feed.history || heads.contains(id) || tracked.iter().any(|a| a.split('.').next() == Some(id.as_str()));
			r.wanted.send_if_modified(|w| std::mem::replace(w, on) != on);
		}
	}
}

/// The task of source `id`.
fn spawn(handle: &tokio::runtime::Handle, id: &str, feed: &Feed) -> Running {
	let snapshot = Arc::new(Mutex::new(Arc::new(FeedSnapshot::default())));
	let (wanted, rx) = watch::channel(false);
	let task = handle.spawn(run(id.to_string(), feed.clone(), snapshot.clone(), rx));
	Running { feed: feed.clone(), snapshot, wanted, task }
}

/// Reads the source once as it starts, then every `interval` while it is wanted, one read at a
/// time, each leaving its snapshot in `out`; wanted again, at once if its last read is older than
/// that. The first read, needed or not, is what the source has: a `when` or a list's row that
/// names one of its values holds from then on, so a dashboard of them has its shape before anyone
/// looks at it, and can be looked at.
async fn run(id: String, feed: Feed, out: Arc<Mutex<Arc<FeedSnapshot>>>, mut wanted: watch::Receiver<bool>) {
	let every = Duration::from_secs(feed.interval);
	let mut keeper = Keeper::new(&id);
	let mut stuck = None;
	let mut last: Option<tokio::time::Instant> = None;
	loop {
		if last.is_some() && !*wanted.borrow_and_update() {
			if wanted.changed().await.is_err() {
				return;
			}
			continue;
		}
		if let Some(due) = last.map(|l| l + every).filter(|d| *d > tokio::time::Instant::now()) {
			tokio::select! {
				_ = tokio::time::sleep_until(due) => {}
				changed = wanted.changed() => {
					if changed.is_err() {
						return;
					}
					continue;
				}
			}
		}
		last = Some(tokio::time::Instant::now());
		let outcome = read(&feed, &mut stuck).await;
		for line in keeper.took(outcome, SystemTime::now()) {
			eprintln!("{line}");
		}
		*out.lock().unwrap_or_else(|p| p.into_inner()) = Arc::new(keeper.snap.clone());
	}
}

#[cfg(test)]
mod tests;
