//! The power timeline of each UPS source: when it was on line, on battery, or nothing was known. A
//! list of changes, in memory and in `<data dir>/history/timeline/<source>`, a line each
//! (`<Unix seconds> <line|battery|none> <code|->`, the code of the event that made it), appended as
//! it happens and never synced: a line a crash cut short is taken as not there. A `timeline`
//! widget's `segments` are made of it over a range.

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;
use upst_core::{Power, Status};

use crate::consts::limit;
use crate::ui::{Obj, Put};

/// The folder of the timelines in the history's.
pub const DIR: &str = "timeline";

/// Where a station's power comes from: on line (or on bypass), on battery, or not known.
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

	fn parse(s: &str) -> Option<Supply> {
		[Supply::Line, Supply::Battery, Supply::None].into_iter().find(|x| x.as_str() == s)
	}

	/// From `ups.status`: on battery with `OB`; on line with `OL`, or on bypass.
	pub fn of(status: &Status) -> Supply {
		if status.has("OB") {
			Supply::Battery
		} else if status.has("OL") || status.power() == Power::Bypass {
			Supply::Line
		} else {
			Supply::None
		}
	}

	/// Of segments too short to tell apart made one, the state that wins: battery if any was, else
	/// none if any was, else line.
	fn weight(self) -> u8 {
		match self {
			Supply::Line => 0,
			Supply::None => 1,
			Supply::Battery => 2,
		}
	}
}

/// The power is `state` from `t` (Unix seconds) on, as the event `code` said (`-` for none).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
	pub t: u64,
	pub state: Supply,
	pub code: String,
}

impl Change {
	pub fn new(t: u64, state: Supply, code: &str) -> Change {
		Change { t, state, code: code.to_string() }
	}

	fn line(&self) -> String {
		format!("{} {} {}\n", self.t, self.state.as_str(), self.code)
	}

	fn parse(line: &str) -> Option<Change> {
		let mut w = line.split(' ');
		let t = w.next()?.parse().ok()?;
		let state = Supply::parse(w.next()?)?;
		let code = w.next().filter(|c| !c.is_empty() && c.bytes().all(|b| b.is_ascii_graphic())).unwrap_or("-");
		Some(Change::new(t, state, code))
	}
}

/// A source's timeline.
#[derive(Debug, Default)]
pub struct Timeline {
	/// Its file; none for one kept in memory alone.
	file: Option<PathBuf>,
	changes: Arc<Vec<Change>>,
	/// What the source says now: what a recorder's past gives way to after it.
	live: Supply,
	/// The source stopped: nothing more is taken.
	closed: bool,
	/// Its file couldn't be written, which was said.
	failing: bool,
}

impl Timeline {
	/// One kept in memory alone.
	pub fn memory() -> Timeline {
		Timeline::default()
	}

	/// The timeline of the source `id` in `dir`, as its file has it; one that doesn't end in `none`
	/// gets a change to `none` at `last` (when the source was last read, or its last change if that
	/// was later): it wasn't watched since. A file that can't be read is taken as empty.
	pub fn load(dir: &Path, id: &str, last: Option<u64>) -> Timeline {
		let file = dir.join(super::disk::file_name(id));
		let changes = match read(&file) {
			Ok(c) => c,
			Err(e) => {
				eprintln!("history: can't read {} ({e}); taken as empty", file.display());
				Vec::new()
			}
		};
		let over = changes.len() > limit::TIMELINE_CHANGES;
		let mut t = Timeline { file: Some(file), changes: Arc::new(changes), ..Default::default() };
		if over {
			t.trim();
		}
		if let Some(c) = t.changes.last().filter(|c| c.state != Supply::None) {
			let at = last.unwrap_or(c.t).max(c.t);
			t.set(Supply::None, at, "-");
		}
		t
	}

	/// Its changes, oldest first.
	pub fn changes(&self) -> Arc<Vec<Change>> {
		self.changes.clone()
	}

	/// The state of its last change; `none` before any.
	fn state(&self) -> Supply {
		self.changes.last().map_or(Supply::None, |c| c.state)
	}

	/// The power is `s` from `at` (Unix seconds) on, as the event `code` says: a change, written at
	/// once, unless it was so already.
	pub fn set(&mut self, s: Supply, at: u64, code: &str) {
		if self.closed {
			return;
		}
		self.live = s;
		if self.state() == s {
			return;
		}
		// A clock set back doesn't reorder it.
		let at = self.changes.last().map_or(at, |c| at.max(c.t));
		let change = Change::new(at, s, code);
		let line = change.line();
		// Of two changes in one second, the later is all there was, as the file reads.
		let changes = Arc::make_mut(&mut self.changes);
		if changes.last().is_some_and(|c| c.t == at) {
			changes.pop();
		}
		if changes.last().map_or(Supply::None, |c| c.state) != s {
			changes.push(change);
		}
		if self.changes.len() > 2 * limit::TIMELINE_CHANGES {
			self.trim();
		} else {
			let written = self.file.as_deref().map_or(Ok(()), |f| append(f, &line));
			self.said(written);
		}
	}

	/// The source stops being read at `at`: nothing is known from then on, and nothing more is
	/// taken.
	pub fn close(&mut self, at: u64) {
		self.set(Supply::None, at, "-");
		self.closed = true;
	}

	/// The newest `TIMELINE_CHANGES` alone, its file written anew.
	fn trim(&mut self) {
		let n = self.changes.len().saturating_sub(limit::TIMELINE_CHANGES);
		if n > 0 {
			Arc::make_mut(&mut self.changes).drain(..n);
		}
		self.rewrite();
	}

	fn rewrite(&mut self) {
		let written = match &self.file {
			Some(f) => rewrite(f, &self.changes),
			None => Ok(()),
		};
		self.said(written);
	}

	/// Says once that its file can't be written, and once that it can again.
	fn said(&mut self, written: io::Result<()>) {
		let file = self.file.as_deref().unwrap_or(Path::new("")).display();
		match written {
			Err(e) if !self.failing => {
				eprintln!("history: can't write {file} ({e}); the power's changes are kept in memory meanwhile");
				self.failing = true;
			}
			Ok(()) if self.failing => {
				eprintln!("history: {file} written again");
				self.failing = false;
			}
			_ => {}
		}
	}
}

/// A file's changes in order of time, a time once (its last written), none the same as the one
/// before it; what follows its last line break (a line a crash cut short) is cut off.
fn read(path: &Path) -> io::Result<Vec<Change>> {
	let bytes = match fs::read(path) {
		Ok(b) => b,
		Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
		Err(e) => return Err(e),
	};
	let whole = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
	if whole < bytes.len() {
		fs::OpenOptions::new().write(true).open(path)?.set_len(whole as u64)?;
	}
	let text = String::from_utf8_lossy(&bytes[..whole]);
	Ok(tidy(text.lines().filter_map(Change::parse).collect()))
}

/// `changes` in order of time, a time once (the last of it), none the same as the one before it,
/// nor `none` first: before any, nothing is known.
fn tidy(mut changes: Vec<Change>) -> Vec<Change> {
	// Stable: of the changes at one time, the last stays last.
	changes.sort_by_key(|c| c.t);
	let mut out: Vec<Change> = Vec::with_capacity(changes.len());
	for c in changes {
		if out.last().is_some_and(|l| l.t == c.t) {
			out.pop();
		}
		if out.last().map_or(Supply::None, |l| l.state) != c.state {
			out.push(c);
		}
	}
	out
}

fn append(path: &Path, line: &str) -> io::Result<()> {
	if let Some(dir) = path.parent() {
		fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
	}
	// One write of a short line: appended whole, beside another task's.
	fs::OpenOptions::new().append(true).create(true).mode(0o600).open(path)?.write_all(line.as_bytes())
}

fn rewrite(path: &Path, changes: &[Change]) -> io::Result<()> {
	if let Some(dir) = path.parent() {
		fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
	}
	let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
	let tmp = path.with_file_name(format!(".{name}.tmp"));
	let text: String = changes.iter().map(Change::line).collect();
	let written = fs::OpenOptions::new()
		.write(true)
		.create(true)
		.truncate(true)
		.mode(0o600)
		.open(&tmp)
		.and_then(|mut f| f.write_all(text.as_bytes()))
		.and_then(|()| fs::rename(&tmp, path));
	if written.is_err() {
		let _ = fs::remove_file(&tmp);
	}
	written
}

/// A stretch of one state, from and to Unix ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
	pub from: u64,
	pub to: u64,
	pub state: Supply,
}

/// The segments of `changes` from `from_ms` to `to_ms`, oldest first, each from where the one
/// before ends, `none` where nothing is known. Segments shorter than a 400th of that time that come
/// one after another are made one, `battery` if any was, else `none` if any was, else `line`; a
/// short one alone stays. Neighbours of one state are one. None when `to_ms` isn't after `from_ms`.
pub fn segments(changes: &[Change], from_ms: u64, to_ms: u64) -> Vec<Segment> {
	if to_ms <= from_ms {
		return Vec::new();
	}
	let ms = |c: &Change| c.t.saturating_mul(1000);
	let first = changes.iter().rev().find(|c| ms(c) <= from_ms).map_or(Supply::None, |c| c.state);
	let mut raw = vec![Segment { from: from_ms, to: to_ms, state: first }];
	for c in changes.iter().filter(|c| ms(c) > from_ms && ms(c) < to_ms) {
		let last = raw.last_mut().expect("one at least");
		if last.state != c.state {
			last.to = ms(c);
			raw.push(Segment { from: ms(c), to: to_ms, state: c.state });
		}
	}
	let short = |s: &Segment| s.to - s.from < (to_ms - from_ms) / 400;
	let mut merged: Vec<Segment> = Vec::with_capacity(raw.len());
	let mut i = 0;
	while i < raw.len() {
		let run = raw[i..].iter().take_while(|s| short(s)).count();
		let seg = if run >= 2 {
			let state = raw[i..i + run].iter().map(|s| s.state).max_by_key(|s| s.weight()).unwrap_or_default();
			let s = Segment { from: raw[i].from, to: raw[i + run - 1].to, state };
			i += run;
			s
		} else {
			i += 1;
			raw[i - 1]
		};
		match merged.last_mut() {
			Some(last) if last.state == seg.state => last.to = seg.to,
			_ => merged.push(seg),
		}
	}
	merged
}

/// `segments` as the protocol has them.
pub fn json(segments: &[Segment]) -> Value {
	Value::Array(
		segments
			.iter()
			.map(|s| {
				let mut o = Obj::new();
				o.put("from", s.from);
				o.put("to", s.to);
				o.put("state", s.state.as_str());
				Value::Object(o)
			})
			.collect(),
	)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::history::disk::tests::scratch;

	fn changes(list: &[(u64, Supply)]) -> Vec<Change> {
		list.iter().map(|(t, s)| Change::new(*t, *s, "-")).collect()
	}

	fn states(t: &Timeline) -> Vec<(u64, Supply)> {
		t.changes().iter().map(|c| (c.t, c.state)).collect()
	}

	fn spans(segs: &[Segment]) -> Vec<(u64, u64, &'static str)> {
		segs.iter().map(|s| (s.from, s.to, s.state.as_str())).collect()
	}

	use Supply::{Battery as B, Line as L, None as N};

	#[test]
	fn segments_cover_the_range_and_short_ones_side_by_side_are_made_one() {
		let c = changes(&[(100, L), (200, B), (300, N), (400, L)]);
		// Over all of it, and before and after: nothing known before the first change.
		assert_eq!(spans(&segments(&c, 0, 500_000)), [(0, 100_000, "none"), (100_000, 200_000, "line"), (200_000, 300_000, "battery"), (300_000, 400_000, "none"), (400_000, 500_000, "line")]);
		// From inside a segment; a change at the very start says what it starts with.
		assert_eq!(spans(&segments(&c, 250_000, 450_000)), [(250_000, 300_000, "battery"), (300_000, 400_000, "none"), (400_000, 450_000, "line")]);
		assert_eq!(spans(&segments(&c, 300_000, 350_000)), [(300_000, 350_000, "none")]);
		assert_eq!(spans(&segments(&[], 1, 2)), [(1, 2, "none")]);
		assert!(segments(&c, 5, 5).is_empty());
		// A day: a 400th is 216 s. A flapping link of short stretches is one `none`, but for the
		// battery among them; a short stretch alone stays.
		let day = 86_400;
		let mut flaps = vec![(1000, L)];
		for k in 0..10 {
			flaps.push((5000 + k * 20, N));
			flaps.push((5010 + k * 20, L));
		}
		flaps.extend([(20_000, B), (20_100, L), (40_000, N), (40_050, L), (40_100, B), (40_150, L)]);
		let segs = segments(&changes(&flaps), 0, day * 1000);
		assert_eq!(
			spans(&segs),
			[
				(0, 1_000_000, "none"),
				(1_000_000, 5_000_000, "line"),
				(5_000_000, 5_190_000, "none"),
				(5_190_000, 20_000_000, "line"),
				(20_000_000, 20_100_000, "battery"),
				(20_100_000, 40_000_000, "line"),
				(40_000_000, 40_150_000, "battery"),
				(40_150_000, 86_400_000, "line"),
			]
		);
		assert_eq!(
			json(&segs[..1]),
			serde_json::json!([{ "from": 0, "to": 1_000_000, "state": "none" }])
		);
	}

	#[test]
	fn a_timeline_is_kept_across_restarts_and_ends_in_none_when_it_was_left() {
		let dir = scratch("timeline");
		let mut t = Timeline::load(&dir, "eco", None);
		assert!(t.changes().is_empty());
		t.set(L, 1000, "-");
		t.set(L, 1010, "-");
		t.set(B, 2000, "power.lost");
		let file = dir.join("eco");
		assert_eq!(fs::read_to_string(&file).unwrap(), "1000 line -\n2000 battery power.lost\n");
		drop(t);
		// The service went away on battery; it was last read at 2500.
		let mut t = Timeline::load(&dir, "eco", Some(2500));
		assert_eq!(states(&t), [(1000, L), (2000, B), (2500, N)]);
		t.set(B, 3000, "power.started_on_battery");
		t.close(3600);
		t.set(L, 3700, "-");
		assert_eq!(t.changes().last(), Some(&Change::new(3600, N, "-")));
		// A line cut short and a line of nonsense are left out; the file is cut to what is whole.
		let mut f = fs::OpenOptions::new().append(true).open(&file).unwrap();
		f.write_all(b"junk\n4000 bat").unwrap();
		drop(f);
		let t = Timeline::load(&dir, "eco", Some(100));
		assert_eq!(states(&t), [(1000, L), (2000, B), (2500, N), (3000, B), (3600, N)]);
		assert_eq!(t.changes()[1].code, "power.lost");
		assert!(fs::read_to_string(&file).unwrap().ends_with("3600 none -\njunk\n"));
		fs::remove_dir_all(&dir).unwrap();
	}

	#[test]
	fn a_timeline_keeps_its_newest_changes() {
		let dir = scratch("bound");
		let mut t = Timeline::load(&dir, "eco", None);
		let n = limit::TIMELINE_CHANGES as u64;
		for k in 0..2 * n + 1 {
			t.set(if k % 2 == 0 { L } else { B }, 1000 + k, "-");
		}
		assert_eq!(t.changes().len(), limit::TIMELINE_CHANGES);
		assert_eq!(t.changes().last().unwrap().t, 1000 + 2 * n);
		let lines = fs::read_to_string(dir.join("eco")).unwrap().lines().count();
		assert_eq!(lines, limit::TIMELINE_CHANGES);
		fs::remove_dir_all(&dir).unwrap();
	}
}
