//! The activity log: every acting request a phone made, let through or refused, with how it
//! ended. The last `limit::ACTIVITY_MAX` are kept in memory and in a private file in the data
//! directory, one JSON object per line, so they survive restarts.

use std::collections::VecDeque;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::consts::limit;

/// Its name in the data directory.
pub const FILE: &str = "activity.jsonl";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Entry {
	/// Unix time in milliseconds.
	pub t: u64,
	/// The phone's name, as it was then.
	pub phone: String,
	pub client_id: String,
	/// What was asked, like "power reboot" or "unit restart nginx.service".
	pub what: String,
	/// "ok", or the error code it was answered with.
	pub result: String,
	/// More about the result: what failed, or how an action exited.
	#[serde(default)]
	pub message: Option<String>,
	/// For a phone that got a key of its own with another's token: the entry that key came from
	/// (`store::family_of`). The phones of one family share `limit::ACTIVITY_PER_FAMILY` entries.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub family: Option<String>,
}

impl Entry {
	/// The family whose share of the log this entry is in.
	fn family(&self) -> &str {
		self.family.as_deref().unwrap_or(&self.client_id)
	}
}

pub struct Activity {
	path: PathBuf,
	state: Mutex<State>,
}

struct State {
	entries: VecDeque<Entry>,
	/// Lines in the file, to know when to rewrite it shorter.
	lines: usize,
	/// A failed write was reported, so the next ones aren't.
	told: bool,
}

pub fn now_ms() -> u64 {
	SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Nothing that doesn't show and a sane length: names and pids come from phones and processes.
/// Controls become spaces, format characters (`store::unseen`) go.
pub fn clean(s: &str) -> String {
	let s: String = s
		.chars()
		.filter(|c| c.is_control() || !crate::store::unseen(*c))
		.map(|c| if c.is_control() { ' ' } else { c })
		.take(limit::WHAT_MAX)
		.collect();
	s.trim().to_string()
}

/// Adds `entry` to `entries`, keeping `limit::ACTIVITY_PER_FAMILY` of its family's and
/// `limit::ACTIVITY_MAX` in all: a family past its share loses its own oldest, so one phone that
/// keeps asking can't push the others' out of the log.
fn push(entries: &mut VecDeque<Entry>, entry: Entry) {
	let family = entry.family().to_string();
	if entries.iter().filter(|e| e.family() == family).count() >= limit::ACTIVITY_PER_FAMILY
		&& let Some(oldest) = entries.iter().position(|e| e.family() == family)
	{
		entries.remove(oldest);
	}
	entries.push_back(entry);
	while entries.len() > limit::ACTIVITY_MAX {
		entries.pop_front();
	}
}

impl Activity {
	/// The log kept in `dir`. Lines that don't parse are skipped, and dropped when the file is
	/// next rewritten.
	pub fn load(dir: &Path) -> Self {
		let path = dir.join(FILE);
		let (entries, lines) = match fs::read_to_string(&path) {
			Ok(text) => {
				let mut entries = VecDeque::new();
				let mut lines = 0;
				for line in text.lines().filter(|l| !l.trim().is_empty()) {
					lines += 1;
					if let Ok(e) = serde_json::from_str::<Entry>(line) {
						push(&mut entries, e);
					}
				}
				(entries, lines)
			}
			Err(e) => {
				if e.kind() != io::ErrorKind::NotFound {
					eprintln!("activity: can't read {}: {e}; starting a new log", path.display());
				}
				(VecDeque::new(), 0)
			}
		};
		Activity { path, state: Mutex::new(State { entries, lines, told: false }) }
	}

	pub fn record(&self, entry: Entry) {
		let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
		push(&mut s.entries, entry.clone());
		let written = if s.lines + 1 >= limit::ACTIVITY_FILE_LINES {
			// Rewritten whole, atomically, with what is kept.
			let mut text = Vec::new();
			for e in &s.entries {
				text.extend(serde_json::to_vec(e).unwrap_or_default());
				text.push(b'\n');
			}
			crate::store::write_private(&self.path, &text).map(|()| s.lines = s.entries.len())
		} else {
			self.append(&entry).map(|()| s.lines += 1)
		};
		match written {
			Ok(()) => s.told = false,
			Err(e) if !s.told => {
				s.told = true;
				eprintln!("activity: can't write {}: {e}; the log is kept in memory meanwhile", self.path.display());
			}
			Err(_) => {}
		}
	}

	fn append(&self, entry: &Entry) -> io::Result<()> {
		let dir = self.path.parent().expect("the log lives in a directory");
		if !dir.exists() {
			fs::create_dir_all(dir)?;
			fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
		}
		let mut line = serde_json::to_vec(entry).map_err(io::Error::other)?;
		line.push(b'\n');
		let mut f = fs::OpenOptions::new().append(true).create(true).mode(0o600).open(&self.path)?;
		// One write, so a line is never split by another writer.
		f.write_all(&line)
	}

	/// The last `n` entries, newest first.
	pub fn newest(&self, n: usize) -> Vec<Entry> {
		let s = self.state.lock().unwrap_or_else(|p| p.into_inner());
		s.entries.iter().rev().take(n).cloned().collect()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// One of eight phones' entries, in turn.
	fn entry(i: usize) -> Entry {
		Entry {
			t: i as u64,
			phone: "Phone".into(),
			client_id: format!("phone{}", i % 8),
			what: format!("power lock {i}"),
			result: "ok".into(),
			message: None,
			family: None,
		}
	}

	fn scratch() -> PathBuf {
		let dir = std::env::temp_dir().join(format!("elamond-activity-{}", data_encoding::HEXLOWER.encode(&crate::store::random_bytes::<6>())));
		fs::create_dir_all(&dir).unwrap();
		dir
	}

	#[test]
	fn the_log_survives_a_restart_and_keeps_the_last_entries() {
		let dir = scratch();
		let log = Activity::load(&dir);
		assert!(log.newest(10).is_empty());
		for i in 0..3 {
			log.record(entry(i));
		}
		assert_eq!(log.newest(2).iter().map(|e| e.t).collect::<Vec<_>>(), [2, 1]);
		let path = dir.join(FILE);
		assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);

		// A restart reads it back; a damaged line is skipped.
		let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
		f.write_all(b"{not json\n").unwrap();
		drop(f);
		let log = Activity::load(&dir);
		assert_eq!(log.newest(10), [entry(2), entry(1), entry(0)]);

		// Past the cap, only the newest are kept, and the file is rewritten before it grows past
		// its own.
		for i in 3..(limit::ACTIVITY_FILE_LINES + 7) {
			log.record(entry(i));
		}
		let newest = log.newest(usize::MAX);
		assert_eq!(newest.len(), limit::ACTIVITY_MAX);
		assert_eq!(newest[0].t as usize, limit::ACTIVITY_FILE_LINES + 6);
		let lines = fs::read_to_string(&path).unwrap().lines().count();
		assert!((limit::ACTIVITY_MAX..limit::ACTIVITY_FILE_LINES).contains(&lines), "{lines}");
		assert_eq!(Activity::load(&dir).newest(usize::MAX), newest);
		// No temporary files are left behind.
		assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
		fs::remove_dir_all(dir).unwrap();
	}

	#[test]
	fn one_family_keeps_its_share_and_no_more() {
		let dir = scratch();
		let log = Activity::load(&dir);
		let quiet = |i: usize, phone: &str| Entry { client_id: phone.into(), ..entry(i) };
		log.record(quiet(0, "quiet"));
		log.record(quiet(1, "other"));
		// A phone and the key of its own it asked for, acting as fast as they may, for long.
		for i in 2..2000 {
			let id = if i % 2 == 0 { "busy" } else { "busy-new" };
			log.record(Entry { family: (id != "busy").then(|| "busy".to_string()), ..quiet(i, id) });
		}
		let kept = log.newest(usize::MAX);
		assert_eq!(kept.len(), 2 + limit::ACTIVITY_PER_FAMILY);
		assert!(kept.iter().any(|e| e.client_id == "quiet") && kept.iter().any(|e| e.client_id == "other"));
		// The family's newest are kept, both its phones'.
		assert_eq!(kept[0].t, 1999);
		assert!(kept.iter().any(|e| e.client_id == "busy-new") && kept.iter().any(|e| e.client_id == "busy"));
		// And so it reads back.
		assert_eq!(Activity::load(&dir).newest(usize::MAX), kept);
		let line = fs::read_to_string(dir.join(FILE)).unwrap().lines().last().unwrap().to_string();
		assert!(line.contains("\"family\":\"busy\""), "{line}");
		fs::remove_dir_all(dir).unwrap();
	}

	#[test]
	fn what_is_asked_is_cleaned() {
		assert_eq!(clean(" power\u{1b}[2J reboot\n"), "power [2J reboot");
		assert_eq!(clean(&"x".repeat(1000)).len(), limit::WHAT_MAX);
		// Format characters go: a name can't read backwards or hide a part.
		assert_eq!(clean("link add Web\u{202e}gpj.exe\u{200b}\u{feff}"), "link add Webgpj.exe");
	}
}
