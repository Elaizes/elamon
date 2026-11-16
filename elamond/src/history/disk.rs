//! The long history on disk, in `<data dir>/history/`: for each address recorded and each tier
//! (`10s`, `5m`, `1h`) a file of 8-byte records, a `u32` LE slot (Unix seconds over the tier's
//! step) and an `f32` LE mean of the readings in it. Points are made in memory as readings come,
//! and appended every `cadence::HISTORY_FLUSH` and as the service stops, never synced: a crash
//! loses those minutes at most. A partial record at a file's end is taken as not there, a slot
//! written twice as its later record, a file that can't be read as empty; a 10-second or 5-minute
//! file is rewritten with its window alone once it holds twice as much.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{self, Seek, SeekFrom, Write};
use std::ops::Range;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::consts::cadence;

/// The folder in the data directory.
pub const DIR: &str = "history";

/// A step the history keeps points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
	/// Every 10 seconds, for a day.
	S10,
	/// Every 5 minutes, for 30 days.
	M5,
	/// Every hour, for good or `[history] keep` days.
	H1,
}

impl Tier {
	pub const ALL: [Tier; 3] = [Tier::S10, Tier::M5, Tier::H1];

	fn index(self) -> usize {
		self as usize
	}

	/// Its folder.
	pub fn name(self) -> &'static str {
		match self {
			Tier::S10 => "10s",
			Tier::M5 => "5m",
			Tier::H1 => "1h",
		}
	}

	/// The seconds a point stands for.
	pub fn step(self) -> u64 {
		match self {
			Tier::S10 => 10,
			Tier::M5 => 300,
			Tier::H1 => 3600,
		}
	}

	/// The slots it keeps; none for the hourly one, whose `keep` the settings say.
	pub fn window(self) -> Option<u64> {
		match self {
			Tier::S10 | Tier::M5 => Some(8640),
			Tier::H1 => None,
		}
	}

	/// The slot `secs` (Unix seconds) is in.
	pub fn slot(self, secs: u64) -> u32 {
		(secs / self.step()).min(u64::from(u32::MAX)) as u32
	}
}

/// A slot and the mean of the readings in it.
pub type Point = (u32, f32);

/// Bytes a point takes in a file.
const RECORD: u64 = 8;
/// Points further ahead of the clock than this (a clock set back) go as a file is rewritten.
const AHEAD_SECS: u64 = 86_400;
/// Points before 2000 are none of the history's: what a damaged file reads as, mostly.
pub const EARLIEST_SECS: u64 = 946_684_800;
/// File names longer than this end in a digest of the address instead.
const NAME_MAX: usize = 200;

/// The long history: the points made and not written yet, and the files.
pub struct Store {
	dir: PathBuf,
	/// Days the hourly points are kept; 0 for all of them.
	keep_days: AtomicU64,
	made: Mutex<Made>,
	/// Held through a flush, so one writes at a time.
	writer: Mutex<Writer>,
	/// Files that couldn't be read, said in the journal once each.
	unreadable: Mutex<HashSet<PathBuf>>,
	/// When each source was last read live.
	seen: Mutex<Seen>,
}

/// What the readings made in memory: the slots being summed, and the points of those that ended.
#[derive(Default)]
pub struct Made {
	/// For each address, the slot of each tier being summed.
	open: HashMap<String, [Option<Sum>; 3]>,
	/// For each tier, the points of each address not written yet, in the order they were made.
	pending: [BTreeMap<String, Vec<Point>>; 3],
}

/// The readings of a slot so far.
#[derive(Clone, Copy, Debug)]
struct Sum {
	slot: u32,
	sum: f64,
	count: u32,
}

impl Sum {
	fn mean(&self) -> f32 {
		(self.sum / f64::from(self.count)) as f32
	}
}

impl Made {
	/// Readings at `secs` (Unix seconds), each summed into its slot of each tier. A slot that time
	/// has moved on from becomes a point, for every address, whether it has a reading now or not.
	pub fn push<'a>(&mut self, secs: u64, readings: impl IntoIterator<Item = (&'a str, f64)>) {
		for (address, x) in readings {
			if address.is_empty() || !x.is_finite() {
				continue;
			}
			if !self.open.contains_key(address) {
				self.open.insert(address.to_string(), [None; 3]);
			}
			let Made { open, pending } = self;
			let sums = open.get_mut(address).expect("just put there");
			for tier in Tier::ALL {
				let slot = tier.slot(secs);
				let sum = &mut sums[tier.index()];
				match sum {
					Some(s) if s.slot == slot => {
						s.sum += x;
						s.count += 1;
					}
					_ => {
						if let Some(s) = sum.take() {
							pending[tier.index()].entry(address.to_string()).or_default().push((s.slot, s.mean()));
						}
						*sum = Some(Sum { slot, sum: x, count: 1 });
					}
				}
			}
		}
		self.close(|tier, slot| slot != tier.slot(secs));
	}

	/// The points made (with `all`, of the slots still being summed too), each tier's in `slots`:
	/// taken away.
	pub fn take(&mut self, all: bool, slots: impl Fn(Tier) -> Range<u32>) -> Vec<(Tier, String, Vec<Point>)> {
		if all {
			self.close(|_, _| true);
		}
		let mut out = Vec::new();
		for tier in Tier::ALL {
			let range = slots(tier);
			for (address, mut points) in std::mem::take(&mut self.pending[tier.index()]) {
				points.retain(|(s, _)| range.contains(s));
				if !points.is_empty() {
					out.push((tier, address, points));
				}
			}
		}
		out
	}

	/// The points made and not taken.
	pub fn len(&self) -> usize {
		self.pending.iter().flat_map(|p| p.values()).map(Vec::len).sum()
	}

	/// The slots summed that `done` says are over become points.
	fn close(&mut self, done: impl Fn(Tier, u32) -> bool) {
		let Made { open, pending } = self;
		open.retain(|address, sums| {
			for tier in Tier::ALL {
				if let Some(s) = sums[tier.index()].filter(|s| done(tier, s.slot)) {
					pending[tier.index()].entry(address.clone()).or_default().push((s.slot, s.mean()));
					sums[tier.index()] = None;
				}
			}
			sums.iter().any(Option::is_some)
		});
	}
}

#[derive(Default)]
struct Writer {
	/// When the files were last trimmed (Unix seconds); 0 before the first time.
	trimmed: u64,
	/// The last flush couldn't write: said once, until one can.
	failing: bool,
}

/// When each source was last read live, as `state.json` keeps it beside the tiers:
/// `{"version":1,"sources":{"eco":{"last":1700000000}}}`, Unix seconds. A recorder fills in the
/// history from then, and a timeline left on line or battery ends then.
#[derive(Default)]
struct Seen {
	/// Its file was read.
	read: bool,
	last: BTreeMap<String, u64>,
	/// Changed since it was written.
	changed: bool,
}

/// The file of `Seen`.
pub const STATE: &str = "state.json";

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
	m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Store {
	/// The history in `dir`, nothing read yet: files are read when asked for.
	pub fn new(dir: PathBuf) -> Store {
		Store {
			dir,
			keep_days: AtomicU64::new(0),
			made: Mutex::new(Made::default()),
			writer: Mutex::new(Writer::default()),
			unreadable: Mutex::new(HashSet::new()),
			seen: Mutex::new(Seen::default()),
		}
	}

	/// How long the hourly points are kept: `days`, or for good.
	pub fn set_keep(&self, days: Option<u64>) {
		self.keep_days.store(days.unwrap_or(0), Ordering::Relaxed);
	}

	/// Readings at `t_ms` (Unix ms), each summed into its slot of each tier. A slot that time has
	/// moved on from becomes a point, for every address, whether it has a reading now or not.
	pub fn push<'a>(&self, t_ms: u64, readings: impl IntoIterator<Item = (&'a str, f64)>) {
		lock(&self.made).push(t_ms / 1000, readings);
	}

	/// Writes the points made since the last flush (with `all`, as the service stops, the slots
	/// still being summed too), then rewrites what outgrew its window; once a day, but not as the
	/// service stops, it trims every file. Never fails: what can't be written is said in the
	/// journal and is lost.
	pub fn flush(&self, now: u64, all: bool) {
		let mut writer = lock(&self.writer);
		let batch: Vec<(Tier, String, Vec<Point>)> = {
			let mut made = lock(&self.made);
			if all {
				made.close(|_, _| true);
			}
			Tier::ALL
				.iter()
				.flat_map(|t| made.pending[t.index()].iter().map(|(a, p)| (*t, a.clone(), p.clone())))
				.collect()
		};
		self.write_batch(&mut writer, &batch, now, "their points of the last minutes are lost");
		// Forgotten once written, or once given up on; those made meanwhile stay.
		{
			let mut made = lock(&self.made);
			for (tier, address, points) in &batch {
				let pending = &mut made.pending[tier.index()];
				if let Some(p) = pending.get_mut(address) {
					p.drain(..points.len().min(p.len()));
					if p.is_empty() {
						pending.remove(address);
					}
				}
			}
		}
		self.save_seen();
		if !all && now >= writer.trimmed.saturating_add(cadence::HISTORY_TRIM.as_secs()) {
			self.trim_files(now);
			writer.trimmed = now;
		}
	}

	/// Appends points made elsewhere (from a recorder's journal) to their files, as a flush does,
	/// and between flushes. Those outside what their tier keeps at `now` are left out.
	pub fn write(&self, batch: Vec<(Tier, String, Vec<Point>)>, now: u64) {
		let mut writer = lock(&self.writer);
		let batch: Vec<_> = batch
			.into_iter()
			.filter_map(|(tier, address, mut points)| {
				let kept = self.kept(tier, now);
				points.retain(|(s, _)| kept.contains(s));
				(!points.is_empty()).then_some((tier, address, points))
			})
			.collect();
		self.write_batch(&mut writer, &batch, now, "those points are lost");
	}

	/// Appends each of `batch` to its file; what can't be written is said once, until it can be.
	fn write_batch(&self, writer: &mut Writer, batch: &[(Tier, String, Vec<Point>)], now: u64, lost: &str) {
		for tier in Tier::ALL.into_iter().filter(|t| batch.iter().any(|(b, ..)| b == t)) {
			// Where it can't be made, each file of the tier says so below.
			let _ = fs::DirBuilder::new().recursive(true).mode(0o700).create(self.dir.join(tier.name()));
		}
		let (mut failures, mut first) = (0, None);
		for (tier, address, points) in batch {
			if let Err(e) = self.append(*tier, address, points, now) {
				failures += 1;
				first.get_or_insert(e);
			}
		}
		match first {
			Some(e) if !writer.failing => {
				eprintln!("history: can't write {failures} of {} files in {} ({e}); {lost}", batch.len(), self.dir.display());
				writer.failing = true;
			}
			None if writer.failing && !batch.is_empty() => {
				eprintln!("history: written again");
				writer.failing = false;
			}
			_ => {}
		}
	}

	/// When the source `id` was last read live, as the history last kept it.
	pub fn last_seen(&self, id: &str) -> Option<u64> {
		let mut seen = lock(&self.seen);
		self.read_seen(&mut seen);
		seen.last.get(id).copied()
	}

	/// The source `id` was read live at `t` (Unix seconds): kept from the next flush.
	pub fn seen(&self, id: &str, t: u64) {
		let mut seen = lock(&self.seen);
		self.read_seen(&mut seen);
		match seen.last.get_mut(id) {
			Some(last) if *last == t => return,
			Some(last) => *last = t,
			None => {
				seen.last.insert(id.to_string(), t);
			}
		}
		seen.changed = true;
	}

	fn read_seen(&self, seen: &mut Seen) {
		if std::mem::replace(&mut seen.read, true) {
			return;
		}
		let path = self.dir.join(STATE);
		let v: serde_json::Value = match fs::read(&path) {
			Ok(b) => serde_json::from_slice(&b).unwrap_or_default(),
			Err(e) if e.kind() == io::ErrorKind::NotFound => return,
			Err(e) => {
				eprintln!("history: can't read {} ({e}); a recorder fills in the history from its start", path.display());
				return;
			}
		};
		for (id, s) in v["sources"].as_object().into_iter().flatten() {
			if let Some(last) = s["last"].as_u64() {
				seen.last.insert(id.clone(), last);
			}
		}
	}

	/// Writes `state.json` when it changed: a new file renamed over it, not synced.
	fn save_seen(&self) {
		let mut seen = lock(&self.seen);
		if !seen.changed {
			return;
		}
		let sources: serde_json::Map<String, serde_json::Value> = seen.last.iter().map(|(id, t)| (id.clone(), serde_json::json!({ "last": t }))).collect();
		let text = serde_json::json!({ "version": 1, "sources": sources }).to_string();
		let path = self.dir.join(STATE);
		let tmp = self.dir.join(format!(".{STATE}.tmp"));
		let written = fs::DirBuilder::new()
			.recursive(true)
			.mode(0o700)
			.create(&self.dir)
			.and_then(|()| fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp))
			.and_then(|mut f| f.write_all(text.as_bytes()))
			.and_then(|()| fs::rename(&tmp, &path));
		match written {
			Ok(()) => seen.changed = false,
			Err(e) => {
				let _ = fs::remove_file(&tmp);
				eprintln!("history: can't write {} ({e})", path.display());
			}
		}
	}

	/// Appends `points` to the file of `address` at `tier`, after cutting off a partial record a
	/// crash left at its end; a 10-second or 5-minute file that comes to hold twice its window is
	/// rewritten with its window alone.
	fn append(&self, tier: Tier, address: &str, points: &[Point], now: u64) -> io::Result<()> {
		let path = self.path(tier, address);
		let mut f = fs::OpenOptions::new().write(true).create(true).truncate(false).mode(0o600).open(&path)?;
		let len = f.metadata()?.len();
		let whole = len - len % RECORD;
		if whole != len {
			f.set_len(whole)?;
		}
		f.seek(SeekFrom::Start(whole))?;
		f.write_all(&encode(points))?;
		drop(f);
		let size = whole + points.len() as u64 * RECORD;
		if let Some(window) = tier.window()
			&& size > 2 * window * RECORD
			&& let Some(records) = self.records(&path)
			&& let Err(e) = rewrite(&path, &tidy(records, self.kept(tier, now)))
		{
			eprintln!("history: can't rewrite {} with its window ({e}); it grows meanwhile", path.display());
		}
		Ok(())
	}

	/// Trims every file to what its tier keeps at `now`, taking away those left with nothing and
	/// what a rewrite cut short left behind.
	#[cfg(test)]
	pub fn trim(&self, now: u64) {
		let _writer = lock(&self.writer);
		self.trim_files(now);
	}

	fn trim_files(&self, now: u64) {
		for tier in Tier::ALL {
			let Ok(entries) = fs::read_dir(self.dir.join(tier.name())) else { continue };
			let kept = self.kept(tier, now);
			for e in entries.flatten() {
				let path = e.path();
				if e.file_name().as_encoded_bytes().starts_with(b".") {
					let _ = fs::remove_file(&path);
					continue;
				}
				let Ok(meta) = e.metadata() else { continue };
				if !meta.is_file() {
					continue;
				}
				let Some(records) = self.records(&path) else { continue };
				let tidied = tidy(records, kept.clone());
				let done = if tidied.is_empty() {
					fs::remove_file(&path)
				} else if tidied.len() as u64 * RECORD != meta.len() {
					rewrite(&path, &tidied)
				} else {
					Ok(())
				};
				if let Err(e) = done {
					eprintln!("history: can't trim {} ({e})", path.display());
				}
			}
		}
	}

	/// The slots `tier` keeps at `now`: its window, or `keep` days, or all since 2000; none
	/// further ahead than a day.
	fn kept(&self, tier: Tier, now: u64) -> Range<u32> {
		let days = self.keep_days.load(Ordering::Relaxed);
		let span = match tier.window() {
			Some(w) => Some(w * tier.step()),
			None => (days > 0).then(|| days.saturating_mul(86_400)),
		};
		let from = span.map_or(EARLIEST_SECS, |s| now.saturating_sub(s)).max(EARLIEST_SECS);
		tier.slot(from)..tier.slot(now.saturating_add(AHEAD_SECS)).saturating_add(1)
	}

	/// The points of `address` at `tier` in `slots`, oldest first, a slot once: those written and
	/// those not yet, and a slot still being summed that ended before `slots` do.
	pub fn points(&self, tier: Tier, address: &str, slots: Range<u32>) -> Vec<Point> {
		// Memory first: a flush writes before it forgets, so each point is in one or the other.
		let mine: Vec<Point> = {
			let made = lock(&self.made);
			let mut p = made.pending[tier.index()].get(address).cloned().unwrap_or_default();
			if let Some(s) = made.open.get(address).and_then(|o| o[tier.index()]).filter(|s| s.slot < slots.end) {
				p.push((s.slot, s.mean()));
			}
			p
		};
		let mut all = self.records(&self.path(tier, address)).unwrap_or_default();
		all.extend(mine);
		tidy(all, slots)
	}

	/// Whether the history can be read at all: its folder is there and open to the service, or not
	/// made yet.
	pub fn readable(&self) -> io::Result<()> {
		match fs::read_dir(&self.dir) {
			Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
			_ => Ok(()),
		}
	}

	/// Where it is.
	pub fn dir(&self) -> &Path {
		&self.dir
	}

	fn path(&self, tier: Tier, address: &str) -> PathBuf {
		self.dir.join(tier.name()).join(file_name(address))
	}

	/// A file's points, as many as are whole and finite, in the order written: none for a file
	/// that isn't there, `None` for one that can't be read (said once).
	fn records(&self, path: &Path) -> Option<Vec<Point>> {
		match fs::read(path) {
			Ok(b) => Some(decode(&b)),
			Err(e) if e.kind() == io::ErrorKind::NotFound => Some(Vec::new()),
			Err(e) => {
				if lock(&self.unreadable).insert(path.to_path_buf()) {
					eprintln!("history: can't read {} ({e}); taken as empty", path.display());
				}
				None
			}
		}
	}
}

/// Flushes `store` every `cadence::HISTORY_FLUSH`, on a thread of its own.
pub fn spawn_writer(store: Arc<Store>) -> io::Result<()> {
	std::thread::Builder::new().name("history".into()).spawn(move || {
		loop {
			std::thread::sleep(cadence::HISTORY_FLUSH);
			store.flush(crate::store::now(), false);
		}
	})?;
	Ok(())
}

/// The name of the file of `address`: itself, each byte but `a-z`, `0-9`, `.`, `_` and `-` as
/// `%XX`, and a `.` it starts with too, so no name is hidden or a folder's. A long one ends in a
/// digest of the address, after `~`, which no other name has.
pub fn file_name(address: &str) -> String {
	let mut out = String::with_capacity(address.len());
	for (i, b) in address.bytes().enumerate() {
		match b {
			b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' => out.push(b as char),
			b'.' if i > 0 => out.push('.'),
			_ => out.push_str(&format!("%{b:02X}")),
		}
	}
	if out.len() > NAME_MAX {
		let digest = crate::store::sha256_hex(address.as_bytes());
		// All of it ASCII; not cut in the middle of a %XX.
		let mut cut = NAME_MAX - 17;
		if let Some(p) = out[cut - 2..cut].find('%') {
			cut = cut - 2 + p;
		}
		out.truncate(cut);
		out.push('~');
		out.push_str(&digest[..16]);
	}
	out
}

fn encode(points: &[Point]) -> Vec<u8> {
	let mut b = Vec::with_capacity(points.len() * RECORD as usize);
	for (slot, x) in points {
		b.extend_from_slice(&slot.to_le_bytes());
		b.extend_from_slice(&x.to_le_bytes());
	}
	b
}

fn decode(b: &[u8]) -> Vec<Point> {
	b.as_chunks::<{ RECORD as usize }>()
		.0
		.iter()
		.filter_map(|r| {
			let slot = u32::from_le_bytes([r[0], r[1], r[2], r[3]]);
			let x = f32::from_le_bytes([r[4], r[5], r[6], r[7]]);
			x.is_finite().then_some((slot, x))
		})
		.collect()
}

/// `records` in `slots`, oldest first, each slot once: the last written of it.
fn tidy(mut records: Vec<Point>, slots: Range<u32>) -> Vec<Point> {
	records.retain(|(s, _)| slots.contains(s));
	// Stable, so of the records of a slot the last written stays last.
	records.sort_by_key(|(s, _)| *s);
	let mut out: Vec<Point> = Vec::with_capacity(records.len());
	for r in records {
		match out.last_mut() {
			Some(last) if last.0 == r.0 => *last = r,
			_ => out.push(r),
		}
	}
	out
}

/// `path` holding `records` alone: a new file beside it renamed over it, not synced.
fn rewrite(path: &Path, records: &[Point]) -> io::Result<()> {
	let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
	let tmp = path.with_file_name(format!(".{name}.tmp"));
	let written = fs::OpenOptions::new()
		.write(true)
		.create(true)
		.truncate(true)
		.mode(0o600)
		.open(&tmp)
		.and_then(|mut f| f.write_all(&encode(records)))
		.and_then(|()| fs::rename(&tmp, path));
	if written.is_err() {
		let _ = fs::remove_file(&tmp);
	}
	written
}

/// What `elamond check` says of the history in `dir`: how much there is, in how many files, and
/// the day of its oldest point.
pub fn summary(dir: &Path) -> String {
	let (mut bytes, mut files) = (0u64, 0usize);
	let mut walk = vec![dir.to_path_buf()];
	while let Some(d) = walk.pop() {
		for e in fs::read_dir(&d).into_iter().flatten().flatten() {
			match e.metadata() {
				Ok(m) if m.is_dir() && d == dir => walk.push(e.path()),
				Ok(m) if m.is_file() && !e.file_name().as_encoded_bytes().starts_with(b".") => {
					bytes += m.len();
					files += 1;
				}
				_ => {}
			}
		}
	}
	if files == 0 {
		return "history: none yet, not in backups".into();
	}
	// The hourly points reach back furthest; in the first hour there are none yet.
	let oldest = Tier::ALL.iter().rev().find_map(|t| {
		let entries = fs::read_dir(dir.join(t.name())).ok()?;
		entries
			.flatten()
			.filter_map(|e| fs::read(e.path()).ok())
			.flat_map(|b| decode(&b))
			.map(|(slot, _)| u64::from(slot) * t.step())
			.filter(|s| *s >= EARLIEST_SECS)
			.min()
	});
	let oldest = oldest.map_or_else(|| "none".to_string(), crate::store::format_date);
	let size = match bytes {
		0..1_048_576 => format!("{} KiB", bytes.div_ceil(1024)),
		_ => format!("{:.1} MiB", bytes as f64 / 1_048_576.0),
	};
	format!("history: {size} in {files} files (oldest {oldest}), not in backups")
}

#[cfg(test)]
pub mod tests {
	use super::*;

	/// A folder of its own for a test's history.
	pub fn scratch(name: &str) -> PathBuf {
		let dir = std::env::temp_dir().join(format!("elamond-history-{name}-{}", data_encoding::HEXLOWER.encode(&crate::store::random_bytes::<6>())));
		fs::create_dir_all(&dir).unwrap();
		dir
	}

	/// The points of a whole tier for an address, written or not.
	fn all(store: &Store, tier: Tier, address: &str) -> Vec<Point> {
		store.points(tier, address, 0..u32::MAX)
	}

	const NOW: u64 = 1_700_000_000;

	#[test]
	fn each_tier_takes_the_mean_of_the_readings_in_its_slots() {
		let dir = scratch("tiers");
		let store = Store::new(dir.clone());
		// A reading a second for two hours and a bit, from the start of an hour.
		let start = NOW / 3600 * 3600;
		for t in start..start + 2 * 3600 + 5 {
			store.push(t * 1000, [("cpu.usage", (t - start) as f64)]);
		}
		let s10 = all(&store, Tier::S10, "cpu.usage");
		// 720 slots of 10 s that ended, and the one that is still going.
		assert_eq!(s10.len(), 721);
		assert_eq!(s10[0], ((start / 10) as u32, 4.5));
		assert_eq!(s10[719], ((start / 10) as u32 + 719, 7194.5));
		let m5 = all(&store, Tier::M5, "cpu.usage");
		assert_eq!((m5.len(), m5[0], m5[1].1), (25, ((start / 300) as u32, 149.5), 449.5));
		let h1 = all(&store, Tier::H1, "cpu.usage");
		let hour = (start / 3600) as u32;
		assert_eq!(h1, [(hour, 1799.5), (hour + 1, 5399.5), (hour + 2, 7202.0)]);
		// A slot still going comes only once it has ended, though no reading closed it.
		let going = (start + 7200) / 10;
		assert!(store.points(Tier::S10, "cpu.usage", 0..going as u32).iter().all(|p| p.0 < going as u32));
		assert_eq!(store.points(Tier::S10, "cpu.usage", going as u32..going as u32 + 1), [(going as u32, 7202.0)]);
		// Nothing on disk until a flush.
		assert!(!dir.join("10s").exists());
		fs::remove_dir_all(&dir).unwrap();
	}

	#[test]
	fn a_value_that_stops_still_has_its_last_slot() {
		let dir = scratch("stops");
		let store = Store::new(dir.clone());
		for t in NOW..NOW + 25 {
			let gpu = (t < NOW + 12).then_some(("gpu.usage", 50.0));
			store.push(t * 1000, [("cpu.usage", 1.0)].into_iter().chain(gpu));
		}
		let slot = (NOW / 10) as u32;
		// The GPU's slot of 10 s that has two readings ended when the next began.
		assert_eq!(store.points(Tier::S10, "gpu.usage", 0..slot + 2), [(slot, 50.0), (slot + 1, 50.0)]);
		// Its 5-minute and hourly slots go on until time moves past them.
		store.push((NOW + 3600) * 1000, [("cpu.usage", 1.0)]);
		assert_eq!(all(&store, Tier::H1, "gpu.usage"), [((NOW / 3600) as u32, 50.0)]);
		assert!(store.lock_made_open_is_without("gpu.usage"));
		fs::remove_dir_all(&dir).unwrap();
	}

	impl Store {
		fn lock_made_open_is_without(&self, address: &str) -> bool {
			!lock(&self.made).open.contains_key(address)
		}

		fn pending_points(&self) -> usize {
			lock(&self.made).pending.iter().flat_map(|p| p.values()).map(Vec::len).sum()
		}
	}

	#[test]
	fn a_flush_writes_and_a_new_store_reads_on_from_it() {
		let dir = scratch("restart");
		let store = Store::new(dir.clone());
		for t in NOW..NOW + 600 {
			store.push(t * 1000, [("cpu.usage", 10.0), ("disk.read.{nvme0n1}", 2.0)]);
		}
		store.flush(NOW + 600, false);
		assert_eq!(store.pending_points(), 0);
		let file = dir.join("10s").join("cpu.usage");
		// Each slot of 10 s that ended, 8 bytes each; the 5-minute one that ended too.
		assert_eq!(fs::metadata(&file).unwrap().len(), 59 * 8);
		assert_eq!(fs::metadata(dir.join("5m").join("cpu.usage")).unwrap().len(), 2 * 8);
		assert!(dir.join("10s").join("disk.read.%7Bnvme0n1%7D").is_file());
		assert!(!dir.join("1h").join("cpu.usage").exists(), "its hour goes on");
		// The service stops halfway through a slot: what it had of every tier is written.
		for t in NOW + 600..NOW + 605 {
			store.push(t * 1000, [("cpu.usage", 20.0)]);
		}
		store.flush(NOW + 605, true);
		let slot = (NOW / 10) as u32 + 60;
		assert_eq!(all(&store, Tier::S10, "cpu.usage").last(), Some(&(slot, 20.0)));
		drop(store);
		// Started again within the same slot: the slot comes out once, the later reading's.
		let store = Store::new(dir.clone());
		assert_eq!(all(&store, Tier::S10, "cpu.usage").len(), 61);
		assert_eq!(all(&store, Tier::H1, "cpu.usage"), [((NOW / 3600) as u32, (6100.0f64 / 605.0) as f32)]);
		for t in NOW + 607..NOW + 615 {
			store.push(t * 1000, [("cpu.usage", 30.0)]);
		}
		assert_eq!(all(&store, Tier::S10, "cpu.usage").last(), Some(&(slot + 1, 30.0)));
		store.flush(NOW + 615, true);
		let s10 = all(&Store::new(dir.clone()), Tier::S10, "cpu.usage");
		assert_eq!((s10.len(), s10[60], s10[61]), (62, (slot, 30.0), (slot + 1, 30.0)));
		assert_eq!(fs::metadata(&file).unwrap().len(), 63 * 8, "appended, the slot twice");
		fs::remove_dir_all(&dir).unwrap();
	}

	#[test]
	fn a_partial_record_at_the_end_is_not_there_and_is_cut_off() {
		let dir = scratch("torn");
		let store = Store::new(dir.clone());
		for t in NOW..NOW + 30 {
			store.push(t * 1000, [("cpu.usage", 5.0)]);
		}
		store.flush(NOW + 30, true);
		let file = dir.join("10s").join("cpu.usage");
		// A crash in the middle of a write.
		let mut f = fs::OpenOptions::new().append(true).open(&file).unwrap();
		f.write_all(&encode(&[((NOW / 10) as u32 + 3, 9.0)])[..5]).unwrap();
		drop(f);
		let store = Store::new(dir.clone());
		assert_eq!(all(&store, Tier::S10, "cpu.usage").len(), 3);
		for t in NOW + 30..NOW + 40 {
			store.push(t * 1000, [("cpu.usage", 6.0)]);
		}
		store.flush(NOW + 40, true);
		assert_eq!(fs::metadata(&file).unwrap().len(), 4 * 8);
		assert_eq!(all(&store, Tier::S10, "cpu.usage")[3], ((NOW / 10) as u32 + 3, 6.0));
		fs::remove_dir_all(&dir).unwrap();
	}

	#[test]
	fn a_damaged_or_unreadable_file_is_empty_and_never_stops_the_service() {
		let dir = scratch("garbage");
		for t in Tier::ALL {
			fs::create_dir_all(dir.join(t.name())).unwrap();
		}
		// Words where points should be; random bytes; a folder where a file should be.
		fs::write(dir.join("10s").join("cpu.usage"), b"not a history file\n").unwrap();
		let noise: Vec<u8> = (0..4096u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
		fs::write(dir.join("1h").join("cpu.usage"), &noise).unwrap();
		fs::create_dir_all(dir.join("5m").join("cpu.usage")).unwrap();
		let store = Store::new(dir.clone());
		let near = |t: Tier| t.slot(NOW - 400 * 86_400)..t.slot(NOW);
		for t in Tier::ALL {
			assert!(store.points(t, "cpu.usage", near(t)).is_empty(), "{t:?}");
		}
		for t in NOW..NOW + 20 {
			store.push(t * 1000, [("cpu.usage", 1.0)]);
		}
		store.flush(NOW + 20, true);
		// Points written after the words line up, and read.
		let slot = (NOW / 10) as u32;
		assert_eq!(store.points(Tier::S10, "cpu.usage", near(Tier::S10).start..slot + 2), [(slot, 1.0), (slot + 1, 1.0)]);
		assert_eq!(store.points(Tier::H1, "cpu.usage", near(Tier::H1).start..Tier::H1.slot(NOW) + 1), [(Tier::H1.slot(NOW), 1.0)]);
		// The trim keeps what is sound; the folder stays as it is, never read.
		store.trim(NOW + 20);
		assert_eq!(fs::metadata(dir.join("10s").join("cpu.usage")).unwrap().len(), 2 * 8);
		assert!(dir.join("5m").join("cpu.usage").is_dir());
		assert_eq!(all(&store, Tier::H1, "cpu.usage"), [(Tier::H1.slot(NOW), 1.0)]);
		fs::remove_dir_all(&dir).unwrap();
	}

	#[test]
	fn a_file_never_grows_past_twice_its_window() {
		let dir = scratch("bound");
		let store = Store::new(dir.clone());
		let file = dir.join("10s").join("cpu.usage");
		let (window, bound) = (8640u64, 2 * 8640 * 8);
		// Three days of readings every 10 s, flushed every 5 minutes as the service does.
		let mut biggest = 0;
		let start = NOW;
		for t in (start..start + 3 * 86_400).step_by(10) {
			store.push(t * 1000, [("cpu.usage", (t % 7) as f64)]);
			if t % 300 == 0 {
				store.flush(t, false);
				biggest = biggest.max(fs::metadata(&file).map_or(0, |m| m.len()));
			}
		}
		// Within a flush of the bound, at the most.
		assert!(biggest <= bound && biggest >= bound - 300 / 10 * 8, "{biggest}");
		let end = start + 3 * 86_400;
		let kept = all(&store, Tier::S10, "cpu.usage");
		assert!(kept.len() as u64 >= window && kept.len() as u64 <= 2 * window + 30, "{}", kept.len());
		assert!(kept.windows(2).all(|w| w[1].0 == w[0].0 + 1), "every slot once, in order");
		assert_eq!(kept.last().unwrap().0, ((end - 10) / 10) as u32);
		// The 5-minute file holds 30 days at most, the same way: three days are well inside.
		assert_eq!(fs::metadata(dir.join("5m").join("cpu.usage")).unwrap().len(), 3 * 288 * 8);
		fs::remove_dir_all(&dir).unwrap();
	}

	#[test]
	fn the_daily_trim_keeps_each_tiers_window_and_keep_days_of_hours() {
		let dir = scratch("keep");
		let store = Store::new(dir.clone());
		let day = 86_400;
		let then = NOW - 100 * day;
		// Readings an hour apart from 100 days ago, and a value that stopped 40 days ago.
		for t in (then..NOW).step_by(3600) {
			let old = (t < NOW - 40 * day).then_some(("gpu.usage", 1.0));
			store.push(t * 1000, [("cpu.usage", 2.0)].into_iter().chain(old));
		}
		store.flush(NOW, true);
		// A point a year ahead, from a clock that was wrong.
		let mut f = fs::OpenOptions::new().append(true).open(dir.join("1h").join("cpu.usage")).unwrap();
		f.write_all(&encode(&[(Tier::H1.slot(NOW + 365 * day), 3.0)])).unwrap();
		drop(f);
		store.trim(NOW);
		let h1 = all(&store, Tier::H1, "cpu.usage");
		assert_eq!((h1.len(), h1[0].0), (2400, Tier::H1.slot(then)), "all of it, without the year ahead");
		let m5 = all(&store, Tier::M5, "cpu.usage");
		assert_eq!((m5.len(), m5[0].0), (720, Tier::M5.slot(NOW - 30 * day)), "30 days");
		assert_eq!(all(&store, Tier::S10, "cpu.usage").len(), 24);
		// The value gone 40 days: its 5-minute and 10-second files go, its hours stay.
		assert!(!dir.join("5m").join("gpu.usage").exists() && !dir.join("10s").join("gpu.usage").exists());
		assert_eq!(all(&store, Tier::H1, "gpu.usage").len(), 60 * 24);
		// keep = 30: hours older than 30 days go, and with them the value gone 40 days.
		store.set_keep(Some(30));
		store.trim(NOW);
		let h1 = all(&store, Tier::H1, "cpu.usage");
		assert_eq!((h1.len(), h1[0].0), (30 * 24, Tier::H1.slot(NOW - 30 * day)));
		assert!(!dir.join("1h").join("gpu.usage").exists());
		// A rewrite cut short left its file behind: the trim takes it away.
		fs::write(dir.join("1h").join(".cpu.usage.tmp"), b"half").unwrap();
		store.trim(NOW);
		assert!(!dir.join("1h").join(".cpu.usage.tmp").exists());
		fs::remove_dir_all(&dir).unwrap();
	}

	#[test]
	fn file_names_are_the_address_with_anything_else_escaped() {
		assert_eq!(file_name("cpu.usage"), "cpu.usage");
		assert_eq!(file_name("temp.{nvme Composite}"), "temp.%7Bnvme%20%43omposite%7D");
		assert_eq!(file_name("eco.outlet.1.realpower"), "eco.outlet.1.realpower");
		assert_eq!(file_name("Fan/2"), "%46an%2F2");
		assert_eq!(file_name(".."), "%2E.");
		let long = file_name(&format!("x.{}", "{".repeat(100)));
		let (head, digest) = long.split_once('~').unwrap();
		assert!(long.len() <= NAME_MAX && head.ends_with("%7B") && digest.len() == 16, "{long}");
		assert_ne!(long, file_name(&format!("x.{}}}", "{".repeat(100))));
	}

	#[test]
	fn check_says_how_much_there_is() {
		let dir = scratch("summary");
		assert_eq!(summary(&dir.join("history")), "history: none yet, not in backups");
		let store = Store::new(dir.join("history"));
		for t in (NOW - 2 * 86_400..NOW).step_by(10) {
			store.push(t * 1000, [("cpu.usage", 1.0), ("mem.used", 2.0)]);
		}
		store.flush(NOW, true);
		assert_eq!(summary(&dir.join("history")), "history: 280 KiB in 6 files (oldest 2023-11-12), not in backups");
		for t in (NOW..NOW + 10 * 86_400).step_by(10) {
			store.push(t * 1000, ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"].map(|a| (a, 1.0)));
		}
		store.flush(NOW + 10 * 86_400, true);
		assert!(summary(&dir.join("history")).starts_with("history: 1.2 MiB in 36 files (oldest 2023-11-12)"), "{}", summary(&dir.join("history")));
		fs::remove_dir_all(&dir).unwrap();
	}
}
