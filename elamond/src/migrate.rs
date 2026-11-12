//! What the service writes beside the settings itself: the starter elamond.toml where there is none,
//! defaults.toml for reference, and, once, an elamond.toml of 0.6.0 (no `schema`, read over what was
//! built in) moved to a whole file of schema 3 that gives the same settings (docs/config.md).

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item, Table};

use crate::settings::{self, DEFAULTS};
use crate::store::{self, Place};

/// The settings elamond starts with, beside the settings, for reference; and the version before's.
pub const DEFAULTS_FILE: &str = "defaults.toml";
pub const DEFAULTS_OLD: &str = "defaults.toml.old";
/// What a moved elamond.toml of 0.6.0 is kept as, beside the new one.
pub const KEPT: &str = "elamond.toml.0.6.0";

/// What the service does with the settings files as it starts, before it reads them: the starter
/// where there is no elamond.toml, a 0.6.0 one moved to schema 3, and defaults.toml beside them.
/// Each said in the journal.
pub fn at_start(place: &Place) {
	match &place.named {
		None => {
			let main = place.main();
			if fs::symlink_metadata(&main).is_err() {
				match write_starter(&main) {
					Ok(()) => eprintln!("settings: wrote {}, the settings elamond starts with (control is off)", store::shown(&main)),
					Err(e) => eprintln!("settings: can't write {}, the settings to start from: {e}", store::shown(&main)),
				}
			} else {
				tell(&main, &move_old(place, store::now()), true);
			}
		}
		Some(named) => {
			if let Ok(text) = fs::read_to_string(named)
				&& text.parse::<toml::Table>().is_ok()
				&& !settings::has_schema(&text)
			{
				eprintln!(
					"settings: {} has no schema, a file of elamond 0.6.0, and is read over the defaults as 0.6.0 read it; \
					 a file ELAMON_CONFIG names isn't moved to schema 3",
					named.display()
				);
			}
			if let Some(note) = not_read(place) {
				eprintln!("settings: {note}");
			}
		}
	}
	if let Some(note) = write_defaults(&place.dir, env!("CARGO_PKG_VERSION")) {
		eprintln!("settings: {note}");
	}
}

/// After the settings files changed, a 0.6.0 elamond.toml mended since is moved to schema 3; said
/// in the journal when it is, or when it couldn't be.
pub fn reread(place: &Place) {
	if place.named.is_none() {
		tell(&place.main(), &move_old(place, store::now()), false);
	}
}

/// Why the user's own elamond.toml isn't read, when ELAMON_CONFIG names another and it is a file
/// of theirs (not a link NixOS or Home Manager made).
pub fn not_read(place: &Place) -> Option<String> {
	let named = place.named.as_ref()?;
	let own = place.dir.join(store::CONFIG_FILE);
	(fs::symlink_metadata(&own).is_ok() && !store::from_nix_store(&own) && *named != own).then(|| {
		format!(
			"{} isn't read: ELAMON_CONFIG names {}. Put what is yours in a drop-in, {}/*.toml",
			store::shown(&own),
			named.display(),
			store::shown(&place.dropins())
		)
	})
}

/// Writes the starter, the settings elamond starts with, as the main file `main` where there is
/// none: never over one, and only its owner's to read.
pub fn write_starter(main: &Path) -> io::Result<()> {
	if let Some(dir) = main.parent().filter(|d| !d.as_os_str().is_empty()) {
		fs::create_dir_all(dir)?;
	}
	let mut f = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(main)?;
	f.write_all(DEFAULTS.as_bytes())
}

// ---- defaults.toml ------------------------------------------------------------------------

/// What defaults.toml begins with, its first line naming the version that wrote it.
pub fn defaults_header(version: &str) -> String {
	format!(
		"# elamond {version}: the settings it starts with, for reference — read-only and not read as settings\n\
		 # (elamond.toml and elamond.d/ are). Each new version rewrites it and keeps the one before as\n\
		 # defaults.toml.old; elamond defaults --diff compares.\n\n"
	)
}

/// The version a defaults.toml says it is of, on its first line.
pub fn version_of(text: &str) -> Option<&str> {
	text.lines().next()?.strip_prefix("# elamond ")?.split_once(':').map(|(v, _)| v)
}

/// Writes defaults.toml into the config directory `dir` when it isn't there or is another version's,
/// that one kept as defaults.toml.old: read-only, the settings `version` starts with. A link there
/// is left alone. What was done, when anything was, or why not.
pub fn write_defaults(dir: &Path, version: &str) -> Option<String> {
	let path = dir.join(DEFAULTS_FILE);
	let shown = store::shown(&path);
	match fs::symlink_metadata(&path) {
		Ok(m) if m.file_type().is_symlink() => return Some(format!("{shown} is a link, which the service leaves alone")),
		Ok(_) => {
			let before = fs::read_to_string(&path).unwrap_or_default();
			if version_of(&before) == Some(version) {
				return None;
			}
			if let Err(e) = fs::rename(&path, dir.join(DEFAULTS_OLD)) {
				return Some(format!("can't keep {shown} as {DEFAULTS_OLD} ({e}), so it stays as it was"));
			}
		}
		Err(e) if e.kind() == io::ErrorKind::NotFound => {}
		Err(e) => return Some(format!("can't look at {shown}: {e}")),
	}
	let written = fs::create_dir_all(dir).and_then(|()| store::replace_as(&path, (defaults_header(version) + DEFAULTS).as_bytes(), 0o444));
	Some(match written {
		Ok(()) => format!("wrote {shown}, the settings elamond {version} starts with, for reference"),
		Err(e) => format!("can't write {shown}: {e}"),
	})
}

// ---- a 0.6.0 elamond.toml moved to schema 3 -------------------------------------------------

/// How moving elamond.toml to schema 3 went.
#[derive(Debug)]
pub enum Moved {
	/// Not one of 0.6.0: it says its schema, or isn't there.
	NotOld,
	/// One of 0.6.0 the service doesn't write: why.
	Kept(String),
	/// One of 0.6.0 whose settings have mistakes, which reading them reports.
	Unsound,
	/// Written whole, it wouldn't give the same settings, or couldn't be written: why.
	Failed(String),
	/// Moved: the old file kept as `kept`, and how the settings differ from the defaults.
	Done { kept: PathBuf, changes: Vec<String> },
}

/// Says in the journal how moving the main file `main` went; at a reread, only what was done or
/// went wrong.
fn tell(main: &Path, moved: &Moved, starting: bool) {
	let main = store::shown(main);
	match moved {
		Moved::NotOld => {}
		Moved::Kept(why) if starting => {
			eprintln!("settings: {main} has no schema, a file of elamond 0.6.0, and is read over the defaults as 0.6.0 read it; it isn't moved to schema 3: {why}")
		}
		Moved::Unsound if starting => eprintln!("settings: {main} is a file of elamond 0.6.0; it moves to schema 3 at the first start after its mistakes are mended"),
		Moved::Kept(_) | Moved::Unsound => {}
		Moved::Failed(why) => eprintln!("settings: {main} stays a file of elamond 0.6.0, read over the defaults as 0.6.0 read it: {why}"),
		Moved::Done { kept, changes } => {
			eprintln!(
				"settings: {main} was a file of elamond 0.6.0 (no schema), read over what was built in; it is now a whole file of settings schema 3, \
				 the settings elamond starts with and yours in their place: the same settings. The file before is kept as {}",
				store::shown(kept)
			);
			if !changes.is_empty() {
				eprintln!("settings: where yours differ from the defaults: {}", changes.join("; "));
			}
		}
	}
}

/// Moves the main file of `place` to schema 3 when it is one of 0.6.0 (no `schema`): only one of
/// the user's own (no ELAMON_CONFIG, not a link, writable), and only when its settings are sound
/// and the new file gives exactly the same ones, the drop-ins read after either. The old file is
/// kept beside it, never over another; the new one replaces it in one step, as private as it was.
pub fn move_old(place: &Place, now: u64) -> Moved {
	let main = place.main();
	let Ok(text) = fs::read_to_string(&main) else { return Moved::NotOld };
	if settings::has_schema(&text) {
		return Moved::NotOld;
	}
	if text.parse::<toml::Table>().is_err() {
		return Moved::Unsound;
	}
	if let Err(why) = movable(place) {
		return Moved::Kept(why);
	}
	let dir = main.parent().unwrap_or(Path::new("."));
	let files = place.files();
	let Ok(sources) = settings::read_files(&files) else { return Moved::Unsound };
	let Ok((_, before)) = settings::read_merged(&sources, true) else { return Moved::Unsound };
	let kept = free(dir, KEPT);
	let kept_name = kept.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
	let new = match compose(&text, &store::format_date(now), &kept_name) {
		Ok(t) => t,
		Err(e) => return Moved::Failed(e),
	};
	let mut moved = sources.clone();
	moved[0].text = new.clone();
	let after = match settings::read_merged(&moved, true) {
		Ok((_, m)) => m,
		Err(m) => return Moved::Failed(format!("written whole, it would have mistakes: {}", settings::join_mistakes(&m).replace('\n', "; "))),
	};
	if after != before {
		let differs = crate::commands::diff(&after, &before).into_iter().map(|c| c.to_string()).collect::<Vec<_>>().join("; ");
		return Moved::Failed(format!("written whole, it wouldn't be the same settings ({differs})"));
	}
	if let Err(e) = keep_and_replace(&main, &kept, &text, &new) {
		return Moved::Failed(e);
	}
	let defaults = settings::read_merged(&[settings::Source::new(settings::DEFAULTS_NAME, DEFAULTS)], true).map(|(_, m)| m).unwrap_or_default();
	let changes = crate::commands::diff(&after, &defaults).into_iter().map(|c| c.to_string()).collect();
	Moved::Done { kept, changes }
}

/// Whether the main file of `place` is one the service may move to schema 3: the user's own (not
/// one ELAMON_CONFIG names, not a link), and writable, in a folder it may write to; why not.
pub fn movable(place: &Place) -> Result<(), String> {
	if place.named.is_some() {
		return Err("ELAMON_CONFIG names it".into());
	}
	let main = place.main();
	let meta = fs::symlink_metadata(&main).map_err(|e| format!("can't look at it: {e}"))?;
	if meta.file_type().is_symlink() {
		return Err(if store::from_nix_store(&main) { "it is a link into the Nix store".into() } else { "it is a link".into() });
	}
	if !meta.is_file() {
		return Err("it isn't a file".into());
	}
	let dir = main.parent().unwrap_or(Path::new("."));
	if !writable(&main) || !writable(dir) {
		return Err("it, or its folder, is read-only".into());
	}
	Ok(())
}

/// Whether this process may write `path`.
fn writable(path: &Path) -> bool {
	use std::os::unix::ffi::OsStrExt;
	let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) else { return false };
	// SAFETY: access reads the NUL-terminated path it is given and nothing else.
	unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
}

/// `dir/name`, or `dir/name-2`… when that is taken: never a file that is there.
fn free(dir: &Path, name: &str) -> PathBuf {
	let mut path = dir.join(name);
	let mut n = 2;
	while fs::symlink_metadata(&path).is_ok() {
		path = dir.join(format!("{name}-{n}"));
		n += 1;
	}
	path
}

/// Keeps `main`, whose text is `old`, as `kept`, and writes `new` in its place in one step with its
/// permissions.
fn keep_and_replace(main: &Path, kept: &Path, old: &str, new: &str) -> Result<(), String> {
	let mode = store::mode_of(main, 0o600);
	// The same file under a second name; where links can't be made, a copy as private.
	if fs::hard_link(main, kept).is_err() {
		let copied = fs::OpenOptions::new().write(true).create_new(true).mode(mode).open(kept).and_then(|mut f| f.write_all(old.as_bytes()));
		if let Err(e) = copied {
			let _ = fs::remove_file(kept);
			return Err(format!("can't keep it as {}: {e}", store::shown(kept)));
		}
	}
	store::replace_as(main, new.as_bytes(), mode).map_err(|e| {
		let _ = fs::remove_file(kept);
		format!("can't write {}: {e}", store::shown(main))
	})
}

/// The new elamond.toml for one of 0.6.0, `old`, moved on `date` and kept as `kept`: the settings
/// elamond starts with, in their own words and comments, every key of `old` in place of the one it
/// overrides (a table merging key by key, anything else, a list of tables too, replacing), with its
/// comments, and every table of its own after them, in its order. A key the defaults show as an
/// example in a comment (`# names = …`) takes that comment's place.
pub fn compose(old: &str, date: &str, kept: &str) -> Result<String, String> {
	let mut doc: DocumentMut = DEFAULTS.parse().map_err(|e| format!("the defaults don't parse: {e}"))?;
	let mut from: DocumentMut = old.parse().map_err(|e| format!("it doesn't parse: {e}"))?;
	// Room between the defaults' tables for what replaces theirs, and their own tables after all.
	let mut last = 0;
	spread(doc.as_item_mut(), &mut last);
	let end = (last + 1) * ROOM;
	// The old file's own header, above its first key or table, goes to the top of the new one.
	let header = take_header(&mut from);
	let note = format!(
		"# Moved here on {date} from the settings of elamond 0.6.0, which are kept beside this file\n\
		 # as {kept}: the settings elamond starts with, each key of those in place of the one\n\
		 # it changed, and their own tables at the end.\n"
	);
	let mut top = format!("\n{note}");
	if header.contains('#') {
		top.push('\n');
		top.push_str(header.trim_end_matches('\n'));
		top.push('\n');
	}
	if let Some((mut key, _)) = doc.as_table_mut().get_key_value_mut("schema") {
		let before = text_of(key.leaf_decor().prefix());
		let (head, rest) = before.split_at(before.rfind("\n\n").map_or(before.len(), |i| i + 1));
		key.leaf_decor_mut().set_prefix(format!("{head}{top}{rest}"));
	}
	uncomment_examples(&mut doc, &mut from);
	apply(doc.as_table_mut(), false, from.as_table(), end);
	let trailing = text_of(Some(from.trailing()));
	if trailing.contains('#') {
		let mine = text_of(Some(doc.trailing()));
		doc.set_trailing(format!("{mine}\n{}", trailing.trim_start_matches('\n')));
	}
	Ok(doc.to_string())
}

/// How far apart the defaults' tables are spread, for what replaces theirs to fit between.
const ROOM: isize = 1 << 20;

fn text_of(r: Option<&toml_edit::RawString>) -> String {
	r.and_then(|r| r.as_str()).unwrap_or("").to_string()
}

/// Spreads the positions of the tables in `item` `ROOM` apart, noting the last.
fn spread(item: &mut Item, last: &mut isize) {
	fn each(t: &mut Table, last: &mut isize) {
		if let Some(p) = t.position() {
			*last = (*last).max(p);
			t.set_position(Some(p * ROOM));
		}
		for (_, i) in t.iter_mut() {
			spread(i, last);
		}
	}
	match item {
		Item::Table(t) => each(t, last),
		Item::ArrayOfTables(a) => a.iter_mut().for_each(|t| each(t, last)),
		_ => {}
	}
}

/// The positions of the tables in `item`.
fn positions(item: &Item, out: &mut Vec<isize>) {
	match item {
		Item::Table(t) => {
			out.extend(t.position());
			for (_, i) in t.iter() {
				positions(i, out);
			}
		}
		Item::ArrayOfTables(a) => {
			for t in a.iter() {
				out.extend(t.position());
				for (_, i) in t.iter() {
					positions(i, out);
				}
			}
		}
		_ => {}
	}
}

/// Moves the tables in `item` to `at` on, in their order.
fn place_at(item: &mut Item, at: isize) {
	let mut ps = Vec::new();
	positions(item, &mut ps);
	let first = ps.into_iter().min().unwrap_or(0);
	shift(item, at - first);
}

fn shift(item: &mut Item, by: isize) {
	match item {
		Item::Table(t) => {
			if let Some(p) = t.position() {
				t.set_position(Some(p + by));
			}
			for (_, i) in t.iter_mut() {
				shift(i, by);
			}
		}
		Item::ArrayOfTables(a) => {
			for t in a.iter_mut() {
				if let Some(p) = t.position() {
					t.set_position(Some(p + by));
				}
				for (_, i) in t.iter_mut() {
					shift(i, by);
				}
			}
		}
		_ => {}
	}
}

/// The comment lines of `prefix`, from its first comment on; empty when it has none.
fn comment(prefix: &str) -> &str {
	match prefix.find('#') {
		Some(i) => &prefix[prefix[..i].rfind('\n').map_or(0, |n| n + 1)..],
		None => "",
	}
}

/// `into` with the comment of `from` added after it, directly above what they are of.
fn joined(into: &str, from: &str) -> String {
	let c = comment(from);
	if c.is_empty() { into.to_string() } else { format!("{into}{c}") }
}

/// Takes the old file's header, its comment above the first key or table up to the last blank line
/// there, off it.
fn take_header(doc: &mut DocumentMut) -> String {
	fn cut(prefix: &str) -> (String, String) {
		let (head, own) = crate::phones::split_own(prefix);
		if head.contains('#') { (head.to_string(), own.to_string()) } else { (String::new(), prefix.to_string()) }
	}
	let first_key = doc.as_table().iter().find(|(_, i)| i.is_value()).map(|(k, _)| k.to_string());
	if let Some(k) = first_key {
		let Some((mut key, _)) = doc.as_table_mut().get_key_value_mut(&k) else { return String::new() };
		let (head, own) = cut(&text_of(key.leaf_decor().prefix()));
		key.leaf_decor_mut().set_prefix(own);
		return head;
	}
	// No key before the tables: the header is above the first of them.
	let mut ps = Vec::new();
	for (_, i) in doc.as_table().iter() {
		positions(i, &mut ps);
	}
	let Some(first) = ps.into_iter().min() else { return String::new() };
	let Some(t) = doc.as_table_mut().iter_mut().find_map(|(_, i)| table_at(i, first)) else { return String::new() };
	let (head, own) = cut(&text_of(t.decor().prefix()));
	t.decor_mut().set_prefix(own);
	head
}

/// The table at `position` in `item`, or below it.
fn table_at(item: &mut Item, position: isize) -> Option<&mut Table> {
	fn within(t: &mut Table, position: isize) -> Option<&mut Table> {
		if t.position() == Some(position) {
			return Some(t);
		}
		t.iter_mut().find_map(|(_, i)| table_at(i, position))
	}
	match item {
		Item::Table(t) => within(t, position),
		Item::ArrayOfTables(a) => a.iter_mut().find_map(|t| within(t, position)),
		_ => None,
	}
}

/// A top-level key of the old file the defaults have only as an example in a comment (`# name =
/// "Desk"`) is written where that example is, with the comment that explains it above.
fn uncomment_examples(doc: &mut DocumentMut, from: &mut DocumentMut) {
	let new: Vec<String> = from.as_table().iter().filter(|(k, i)| i.is_value() && !doc.as_table().contains_key(k)).map(|(k, _)| k.to_string()).collect();
	for k in new {
		let example = format!("# {k} = ");
		let holder = doc.as_table().iter().find(|(h, i)| {
			i.is_value() && doc.as_table().key(h).is_some_and(|key| text_of(key.leaf_decor().prefix()).lines().any(|l| l.starts_with(&example)))
		});
		let Some(holder) = holder.map(|(h, _)| h.to_string()) else { continue };
		let Some((from_key, item)) = from.as_table_mut().remove_entry(&k) else { continue };
		let root = doc.as_table_mut();
		let Some((mut hk, _)) = root.get_key_value_mut(&holder) else { continue };
		let prefix = text_of(hk.leaf_decor().prefix());
		let mut start = prefix.lines().scan(0, |at, l| {
			let here = *at;
			*at += l.len() + 1;
			Some((here, l))
		});
		let Some((at, line)) = start.find(|(_, l)| l.starts_with(&example)) else { continue };
		let (above, below) = (&prefix[..at], &prefix[(at + line.len() + 1).min(prefix.len())..]);
		hk.leaf_decor_mut().set_prefix(below.to_string());
		let mut key = from_key.clone();
		key.leaf_decor_mut().set_prefix(joined(above, &text_of(from_key.leaf_decor().prefix())));
		// Before the key whose comment held the example, the rest in their order.
		let entries: Vec<String> = root.iter().map(|(k, _)| k.to_string()).collect();
		let mut taken: Vec<(toml_edit::Key, Item)> = entries.iter().filter_map(|e| root.remove_entry(e)).collect();
		let at = taken.iter().position(|(k, _)| k.get() == holder).unwrap_or(taken.len());
		taken.insert(at, (key, item));
		for (k, i) in taken {
			root.insert_formatted(&k, i);
		}
	}
}

/// The old file's `from` applied onto `into`: tables, on both sides, merge key by key; anything
/// else replaces what was there, in its place, with its comment after the one there; what isn't
/// there yet is added, its tables at `end` on. `inline`: `into` is an inline table.
fn apply(into: &mut dyn toml_edit::TableLike, inline: bool, from: &dyn toml_edit::TableLike, end: isize) {
	for (k, item) in from.iter() {
		if item.is_none() {
			continue;
		}
		let from_key = from.key(k).cloned().unwrap_or_else(|| toml_edit::Key::new(k));
		match into.get_key_value_mut(k) {
			Some((mut key, slot)) => {
				if slot.is_table_like() && item.is_table_like() {
					let slot_inline = !slot.is_table();
					if let (Item::Table(a), Item::Table(b)) = (&mut *slot, item) {
						let theirs = text_of(a.decor().prefix());
						a.decor_mut().set_prefix(joined(&theirs, &text_of(b.decor().prefix())));
					}
					if let Some(t) = slot.as_table_like_mut()
						&& let Some(f) = item.as_table_like()
					{
						apply(t, slot_inline, f, end);
					}
					continue;
				}
				let mut new = item.clone();
				let mut was = Vec::new();
				positions(slot, &mut was);
				match was.into_iter().min() {
					Some(at) => place_at(&mut new, at),
					None => shift(&mut new, end),
				}
				if inline {
					// Within braces, the spacing there stays.
					let decor = slot.as_value().map(|v| v.decor().clone());
					let mut value = new.into_value().unwrap_or_else(|_| toml_edit::Value::from(""));
					if let Some(d) = decor {
						*value.decor_mut() = d;
					}
					*slot = Item::Value(value);
				} else {
					let prefix = joined(&text_of(key.leaf_decor().prefix()), &text_of(from_key.leaf_decor().prefix()));
					key.leaf_decor_mut().set_prefix(prefix);
					*slot = new;
				}
			}
			None => {
				let mut new = item.clone();
				shift(&mut new, end);
				if inline {
					into.insert(k, new);
				} else if let toml_edit::Entry::Vacant(v) = into.entry_format(&from_key) {
					v.insert(new);
				}
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn scratch() -> Place {
		let dir = std::env::temp_dir().join(format!("elamond-migrate-{}", data_encoding::HEXLOWER.encode(&store::random_bytes::<6>())));
		fs::create_dir_all(&dir).unwrap();
		Place { dir, named: None }
	}

	/// The settings as merged from `sources`, the main file first.
	fn merged(sources: &[(&str, &str)]) -> serde_json::Map<String, serde_json::Value> {
		let s: Vec<settings::Source> = sources.iter().map(|(n, t)| settings::Source::new(*n, *t)).collect();
		settings::read_merged(&s, true).unwrap_or_else(|m| panic!("{}", settings::join_mistakes(&m))).1
	}

	#[test]
	fn nothing_to_move_is_the_defaults_with_a_note() {
		let text = compose("", "2026-11-12", KEPT).unwrap();
		assert_eq!(merged(&[("a", &text)]), merged(&[("a", DEFAULTS)]));
		// The defaults as they are, the note aside.
		let note = "# Moved here on 2026-11-12 from the settings of elamond 0.6.0, which are kept beside this file\n\
			# as elamond.toml.0.6.0: the settings elamond starts with, each key of those in place of the one\n\
			# it changed, and their own tables at the end.\n\n";
		assert_eq!(text.replacen(note, "", 1), DEFAULTS);
	}

	#[test]
	fn keys_replace_in_place_tables_merge_and_new_ones_come_last() {
		let old = "# My settings.\n\n# On, at last.\ncontrol = true\nname = \"Desk\" # this one\nport = 48000\n\n\
			[profiles.default]\nunlock.grace = \"1m\"\n\n# Mine.\n[profiles.mine]\nuse = \"default\"\n\n\
			[dashboards.cpu]\nicon = \"gauge\"\n\n[[dashboards.cpu.widgets]]\ntype = \"text\"\ntext = \"CPU\"\n\n\
			[dashboards.mine]\ntitle = \"Mine\"\n\n# The end.\n";
		let text = compose(old, "2026-11-12", KEPT).unwrap();
		assert_eq!(merged(&[("a", &text)]), merged(&[("a", old)]));
		// The header on top, a key's comment after the defaults' own, a commented example taken up.
		assert!(text.contains("their own tables at the end.\n\n# My settings.\n\nschema = 3\n"), "{text}");
		assert!(text.contains("kept beside this file\n# as elamond.toml.0.6.0: the settings"), "{text}");
		assert!(text.contains("# below), each as far as its role allows.\n# On, at last.\ncontrol = true\n"), "{text}");
		assert!(text.contains("# The computer's name in the app; its host name when left out.\nname = \"Desk\" # this one\n\n# Names"), "{text}");
		assert!(text.contains("\nport = 48000\n"), "{text}");
		// One key of an inline table; a list of tables replaced where it was.
		assert!(text.contains("unlock = { for = [\"power\", \"signal\", \"actions\", \"units\"], grace = \"1m\" }"), "{text}");
		let cpu = text.find("[dashboards.cpu]").unwrap();
		assert!(text[cpu..].starts_with("[dashboards.cpu]\ntitle = \"CPU\"\nicon = \"gauge\"\n\n[[dashboards.cpu.widgets]]\ntype = \"text\"\ntext = \"CPU\"\n\n[templates.top_processes]"), "{}", &text[cpu..cpu + 300]);
		// Tables of its own after the defaults', in its order, comments and all; its last comment last.
		let mine = text.find("# Mine.\n[profiles.mine]").unwrap();
		assert!(mine > text.find("[dashboards.activity]").unwrap());
		assert!(text.ends_with("[dashboards.mine]\ntitle = \"Mine\"\n\n# The end.\n"), "{text}");
	}

	#[test]
	fn the_fixture_moves_to_the_same_settings() {
		let old = include_str!("../fixtures/settings-0.6.0.toml");
		let text = compose(old, "2026-11-12", KEPT).unwrap();
		assert_eq!(merged(&[("a", &text)]), merged(&[("a", old)]));
		crate::fixtures::check_text("settings-0.6.0-moved.toml", &text);
	}

	#[test]
	fn the_service_moves_its_own_file_once_and_keeps_the_old_one() {
		use std::os::unix::fs::PermissionsExt;
		let place = scratch();
		let main = place.main();
		let old = include_str!("../fixtures/settings-0.6.0.toml");
		fs::write(&main, old).unwrap();
		fs::set_permissions(&main, fs::Permissions::from_mode(0o640)).unwrap();
		// A drop-in keeps adding to it.
		fs::create_dir_all(place.dropins()).unwrap();
		fs::write(place.dropins().join("more.toml"), "[profiles.owner]\ntheme = \"light\"\n").unwrap();
		let before = place.load().unwrap();
		// Someone's elamond.toml.0.6.0 is there already: never written over.
		fs::write(place.dir.join(KEPT), "mine").unwrap();
		let Moved::Done { kept, changes } = move_old(&place, 1_794_484_800) else { panic!("not moved") };
		assert_eq!(kept, place.dir.join("elamond.toml.0.6.0-2"));
		assert_eq!(fs::read_to_string(place.dir.join(KEPT)).unwrap(), "mine");
		assert_eq!(fs::read_to_string(&kept).unwrap(), old);
		assert!(changes.iter().any(|c| c.starts_with("added") && c.contains("profiles.owner")), "{changes:?}");
		let text = fs::read_to_string(&main).unwrap();
		assert!(settings::has_schema(&text) && text.contains("kept beside this file\n# as elamond.toml.0.6.0-2: the settings"));
		assert_eq!(fs::metadata(&main).unwrap().permissions().mode() & 0o777, 0o640);
		let after = place.load().unwrap();
		assert_eq!((after.profile("owner").theme.as_str(), after.dashboards.len()), (before.profile("owner").theme.as_str(), before.dashboards.len()));
		// Once.
		assert!(matches!(move_old(&place, 1_794_484_800), Moved::NotOld));
		fs::remove_dir_all(&place.dir).unwrap();
	}

	#[test]
	fn a_file_with_mistakes_or_one_not_its_own_stays() {
		let place = scratch();
		let main = place.main();
		fs::write(&main, "[dashboards.overview]\ntitle = 5\n").unwrap();
		assert!(matches!(move_old(&place, 0), Moved::Unsound));
		fs::write(&main, "control = true\n").unwrap();
		// Named by ELAMON_CONFIG, or a link.
		let named = Place { named: Some(main.clone()), ..place.clone() };
		assert!(matches!(move_old(&named, 0), Moved::Kept(_)));
		let target = place.dir.join("elsewhere.toml");
		fs::rename(&main, &target).unwrap();
		std::os::unix::fs::symlink(&target, &main).unwrap();
		assert!(matches!(move_old(&place, 0), Moved::Kept(why) if why == "it is a link"));
		assert_eq!(fs::read_to_string(&target).unwrap(), "control = true\n");
		fs::remove_dir_all(&place.dir).unwrap();
	}

	#[test]
	fn defaults_beside_follow_the_version() {
		use std::os::unix::fs::PermissionsExt;
		let place = scratch();
		let path = place.dir.join(DEFAULTS_FILE);
		assert!(write_defaults(&place.dir, "0.6.0").unwrap().starts_with("wrote"));
		assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o444);
		assert_eq!(version_of(&fs::read_to_string(&path).unwrap()), Some("0.6.0"));
		// The same version: left as it is.
		assert_eq!(write_defaults(&place.dir, "0.6.0"), None);
		// Another: the one before kept as .old, replacing an older .old.
		fs::write(place.dir.join(DEFAULTS_OLD), "older").unwrap();
		write_defaults(&place.dir, "0.6.1").unwrap();
		let text = fs::read_to_string(&path).unwrap();
		assert!(text.starts_with("# elamond 0.6.1: the settings it starts with") && text.ends_with(DEFAULTS));
		assert_eq!(version_of(&fs::read_to_string(place.dir.join(DEFAULTS_OLD)).unwrap()), Some("0.6.0"));
		// Not read as settings: the settings are elamond.toml and elamond.d/ alone.
		assert!(place.files().iter().all(|f| !f.ends_with(DEFAULTS_FILE)));
		// A link there is left alone.
		fs::remove_file(&path).unwrap();
		std::os::unix::fs::symlink("/dev/null", &path).unwrap();
		assert!(write_defaults(&place.dir, "0.6.2").unwrap().contains("is a link"));
		assert!(path.is_symlink());
		fs::remove_dir_all(&place.dir).unwrap();
	}

	#[test]
	fn the_starter_is_written_where_there_is_none() {
		use std::os::unix::fs::PermissionsExt;
		let place = scratch();
		let main = place.dir.join("sub").join("elamond.toml");
		write_starter(&main).unwrap();
		assert_eq!(fs::read_to_string(&main).unwrap(), DEFAULTS);
		assert_eq!(fs::metadata(&main).unwrap().permissions().mode() & 0o777, 0o600);
		// Never over one.
		fs::write(&main, "control = true\n").unwrap();
		assert!(write_starter(&main).is_err());
		assert_eq!(fs::read_to_string(&main).unwrap(), "control = true\n");
		fs::remove_dir_all(&place.dir).unwrap();
	}
}
