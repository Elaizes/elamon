//! The commands about the settings: `elamond check`, `edit`, `schema`, `defaults` and `values`.

use std::fmt;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::migrate;
use crate::settings::{self, Mistake, Source, DEFAULTS};
use crate::store::{self, Place};
use crate::{collect, values};

/// The settings files of `place` as the service reads them, with `file` taking its place among
/// them: in place of the one it is, as a drop-in when it is in a folder named elamond.d, else in
/// place of the main file. Its text comes from `text` when given (an edited copy), from the file
/// otherwise. With whether the first is the main file, which may not be there.
fn sources_with(place: &Place, file: Option<&Path>, text: Option<String>) -> Result<(Vec<Source>, bool), Vec<Mistake>> {
	let mut files = place.files();
	let Some(file) = file else {
		let sources = settings::read_files(&files)?;
		let main = settings::main_in(&files, &sources);
		return Ok((sources, main));
	};
	let same = |a: &Path| a == file || fs::canonicalize(a).ok().zip(fs::canonicalize(file).ok()).is_some_and(|(x, y)| x == y);
	let dropin = file.parent().and_then(Path::file_name).is_some_and(|d| d == store::DROPIN_DIR);
	match files.iter().position(|f| same(f)) {
		Some(_) => {}
		None if dropin => {
			// Among the drop-ins by name, in place of one of the same name.
			let name = file.file_name();
			let main = files[0].clone();
			files.retain(|f| *f == main || f.file_name() != name);
			let at = files.iter().skip(1).position(|f| f.file_name() > name).map_or(files.len(), |i| i + 1);
			files.insert(at, file.to_path_buf());
		}
		None => files[0] = file.to_path_buf(),
	}
	let mut sources = settings::read_files(&files)?;
	let name = file.display().to_string();
	match (text, sources.iter_mut().find(|s| s.name == name)) {
		(Some(t), Some(s)) => s.text = t,
		(Some(t), None) => {
			let at = files.iter().position(|f| f == file).unwrap_or(0).min(sources.len());
			sources.insert(at, Source::new(name, t));
		}
		(None, Some(_)) => {}
		(None, None) => {
			return Err(vec![Mistake { file: Some(name), line: None, path: String::new(), message: "there is no such file".into() }]);
		}
	}
	let main = settings::main_in(&files, &sources);
	Ok((sources, main))
}

/// "1 link", "2 links".
fn count(n: usize, what: &str) -> String {
	format!("{n} {what}{}", if n == 1 { "" } else { "s" })
}

/// Prints the mistakes, one to a line.
fn tell(mistakes: &[Mistake]) {
	for m in mistakes {
		eprintln!("{m}");
	}
}

/// `elamond check [FILE]`: the settings as the service would read them; with FILE, that file in
/// its place. Each mistake with its file and line; exit 1 when there are any.
pub fn check(args: &[String]) -> Result<(), String> {
	let file = match args {
		[] => None,
		[f] if !f.starts_with('-') => Some(PathBuf::from(f)),
		_ => return Err("usage: elamond check [FILE]".into()),
	};
	let place = Place::here();
	let (sources, main) = sources_with(&place, file.as_deref(), None).inspect_err(|m| tell(m)).map_err(|_| "the settings can't be read".to_string())?;
	match settings::read_settings(&sources, main) {
		Ok(c) => {
			let read: Vec<&str> = sources.iter().map(|s| s.name.as_str()).collect();
			let old = settings::over_defaults(sources.first().filter(|_| main));
			let from = match (read.is_empty(), old) {
				(true, _) => "no file at all".to_string(),
				(false, true) => format!("the defaults under {}", read.join(", ")),
				(false, false) => read.join(", "),
			};
			println!(
				"The settings are sound: {}, {}, {}, {}, from {from}.",
				count(c.dashboards.len(), "dashboard"),
				count(c.actions.len(), "action"),
				count(c.links.len(), "link"),
				count(c.profiles.len(), "profile")
			);
			let mut warn = settings::warnings(&c);
			if old {
				let first = sources[0].name.as_str();
				warn.push(match migrate::movable(&place) {
					Ok(()) if file.is_none() => format!(
						"{first} has no schema: a file of elamond 0.6.0, read over the defaults; it moves to schema 3 at the service's next start, the file before kept beside it as {}",
						migrate::KEPT
					),
					_ => format!("{first} has no schema: a file of elamond 0.6.0, read over the defaults as 0.6.0 read it"),
				});
			}
			if !main && place.named.is_none() && file.is_none() {
				warn.push(format!(
					"there is no {} yet: the service writes the settings elamond starts with there at its next start",
					store::shown(&place.main())
				));
			}
			warn.extend(migrate::not_read(&place));
			for w in warn {
				println!("  ! {w}");
			}
			Ok(())
		}
		Err(m) => {
			tell(&m);
			Err(format!("{} mistake{} in the settings", m.len(), if m.len() == 1 { "" } else { "s" }))
		}
	}
}

/// `elamond edit [FILE]`: a copy of elamond.toml (or FILE) in $VISUAL or $EDITOR; once saved, the
/// settings are checked with it in its place. Sound, it replaces the file and the running service
/// takes it up by itself; broken, it is kept out, and the editor opens again if wanted.
pub fn edit(args: &[String]) -> Result<(), String> {
	let place = Place::here();
	let target = match args {
		[] => place.main(),
		[f] if !f.starts_with('-') => PathBuf::from(f),
		_ => return Err("usage: elamond edit [FILE]".into()),
	};
	let editor = std::env::var("VISUAL").ok().filter(|e| !e.trim().is_empty()).or_else(|| std::env::var("EDITOR").ok().filter(|e| !e.trim().is_empty()));
	let editor = editor.unwrap_or_else(|| "vi".into());
	let original = match fs::read_to_string(&target) {
		Ok(t) => Some(t),
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
		Err(e) => return Err(format!("can't read {}: {e}", target.display())),
	};
	// A file Nix or Home Manager put there is theirs: replacing their link would be undone, or
	// stop the next switch.
	if store::from_nix_store(&target) {
		return Err(format!("{} comes from the Nix store; change it where it is made, or put your own settings in a drop-in in {}/", target.display(), store::DROPIN_DIR));
	}
	let dir = target.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
	if fs::create_dir_all(dir).is_err() || fs::metadata(dir).is_ok_and(|m| m.permissions().readonly()) {
		return Err(format!("can't write to {}; is it managed elsewhere (NixOS, Home Manager)? A drop-in in elamond.d/ is yours to edit", dir.display()));
	}
	// The copy is edited away from the settings, so the service never reads it half done.
	let base = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|d| d.is_absolute() && d.is_dir()).unwrap_or_else(std::env::temp_dir);
	let scratch = scratch_dir(&base).map_err(|e| format!("can't make a folder to edit in, in {}: {e}", base.display()))?;
	let copy = scratch.join(target.file_name().unwrap_or("elamond.toml".as_ref()));
	// A new main file starts as the service would write it; a new drop-in, empty but for a word.
	let start = original.clone().unwrap_or_else(|| {
		if target == place.main() {
			DEFAULTS.to_string()
		} else {
			"# A drop-in of elamond's settings (docs/config.md): its tables add to those of elamond.toml\n# with the same id, key by key.\n\n".into()
		}
	});
	let result = edit_loop(&place, &editor, &target, &copy, &start, original.as_deref());
	let _ = fs::remove_dir_all(&scratch);
	result
}

fn edit_loop(place: &Place, editor: &str, target: &Path, copy: &Path, start: &str, original: Option<&str>) -> Result<(), String> {
	fs::write(copy, start).map_err(|e| format!("can't write {}: {e}", copy.display()))?;
	loop {
		let mut words = editor.split_whitespace();
		let program = words.next().unwrap_or("vi");
		let status = std::process::Command::new(program).args(words).arg(copy).status().map_err(|e| format!("can't run {program}: {e}"))?;
		if !status.success() {
			return Err(format!("{program} ended with {status}; the settings weren't changed"));
		}
		let text = fs::read_to_string(copy).map_err(|e| format!("can't read {}: {e}", copy.display()))?;
		if Some(text.as_str()) == original || (original.is_none() && text == start) {
			println!("No change.");
			return Ok(());
		}
		let checked = sources_with(place, Some(target), Some(text.clone())).and_then(|(s, main)| settings::read_settings(&s, main));
		match checked {
			Ok(_) => {
				save(target, &text).map_err(|e| format!("can't write {}: {e}", target.display()))?;
				println!("Saved {}. A running service takes it up within a few seconds.", target.display());
				return Ok(());
			}
			Err(m) => {
				tell(&m);
				let again = std::io::stdin().is_terminal() && {
					print!("The settings aren't changed until these are mended. Edit again? [Y/n] ");
					let _ = std::io::stdout().flush();
					let mut line = String::new();
					std::io::stdin().read_line(&mut line).is_ok() && !matches!(line.trim().to_ascii_lowercase().as_str(), "n" | "no")
				};
				if !again {
					return Err(match keep_broken(target, &text) {
						Ok(kept) => format!("the settings weren't changed; the edit is kept in {}", kept.display()),
						Err(e) => format!("the settings weren't changed, and the edit couldn't be kept beside them: {e}"),
					});
				}
			}
		}
	}
}

/// A folder of its own for `elamond edit`'s copy in `base`, only its owner may enter: a new one,
/// with a name no one can tell in advance, never one that was there.
fn scratch_dir(base: &Path) -> io::Result<PathBuf> {
	let dir = base.join(format!("elamond-edit-{}", data_encoding::HEXLOWER.encode(&store::random_bytes::<8>())));
	fs::DirBuilder::new().mode(0o700).create(&dir)?;
	Ok(dir)
}

/// The permissions settings written for `path` get: its own, or only its owner's for a new file
/// (a link may hold a password, a command a token).
fn mode_for(path: &Path) -> u32 {
	store::mode_of(path, 0o600)
}

/// Replaces `path` with `text` in one step, keeping its permissions (only its owner's for a new
/// file). The text is never in a file others may read where the settings themselves aren't.
fn save(path: &Path, text: &str) -> io::Result<()> {
	store::replace_as(path, text.as_bytes(), mode_for(path))
}

/// Keeps an edit the settings refused beside `target`, as `<name>.broken`, as private as it.
fn keep_broken(target: &Path, text: &str) -> io::Result<PathBuf> {
	let kept = target.with_file_name(format!("{}.broken", target.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()));
	store::replace_as(&kept, text.as_bytes(), mode_for(target))?;
	Ok(kept)
}

/// `elamond schema`: the JSON Schema of a settings file.
pub fn schema() -> Result<(), String> {
	println!("{}", serde_json::to_string_pretty(&settings::json_schema()).unwrap_or_default());
	Ok(())
}

/// `elamond defaults`: the settings elamond starts with; `--diff`, where the settings differ from
/// them and how they changed since the version before; `TABLE`, one table of them to copy.
pub fn defaults(args: &[String]) -> Result<(), String> {
	match args {
		[] => {
			print!("{DEFAULTS}");
			Ok(())
		}
		[d] if d == "--diff" => {
			print!("{}", defaults_diff(&Place::here(), env!("CARGO_PKG_VERSION"))?);
			Ok(())
		}
		[t] if !t.starts_with('-') => {
			print!("{}", defaults_table(t)?);
			Ok(())
		}
		_ => Err("usage: elamond defaults [--diff | TABLE]".into()),
	}
}

/// The tables with ids, in the order `elamond defaults --diff` lists them: as the starter file
/// has them, then those it has only as examples.
pub const SECTIONS: [&str; 5] = ["profiles", "dashboards", "templates", "links", "actions"];

/// How a part of the settings differs from the same part of others.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
	Changed,
	Added,
	Removed,
}

impl Kind {
	fn word(self) -> &'static str {
		match self {
			Kind::Changed => "changed",
			Kind::Added => "added",
			Kind::Removed => "removed",
		}
	}
}

/// A part of the settings, `(top level)` or a table with an id (`dashboards.sensors`), that
/// differs, and in which of its keys when it is in both.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
	pub kind: Kind,
	pub unit: String,
	pub keys: Vec<String>,
}

impl fmt::Display for Change {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{} {}", self.kind.word(), self.unit)?;
		if !self.keys.is_empty() {
			write!(f, " ({})", self.keys.join(", "))?;
		}
		Ok(())
	}
}

/// The keys whose values differ between `a` and `b`, of those `keep` keeps, sorted.
fn keys_differing(a: &Map<String, Value>, b: &Map<String, Value>, keep: impl Fn(&str) -> bool) -> Vec<String> {
	let mut keys: Vec<String> = a.keys().chain(b.keys()).filter(|k| keep(k) && a.get(*k) != b.get(*k)).cloned().collect();
	keys.sort();
	keys.dedup();
	keys
}

/// Where the settings `yours` differ from `base`, both as merged from their files: the top-level
/// keys (but `schema`), then each table with an id, added, changed (and in which keys) or removed.
pub fn diff(yours: &Map<String, Value>, base: &Map<String, Value>) -> Vec<Change> {
	let mut out = Vec::new();
	let top = keys_differing(yours, base, |k| k != "schema" && !SECTIONS.contains(&k));
	if !top.is_empty() {
		out.push(Change { kind: Kind::Changed, unit: "(top level)".into(), keys: top });
	}
	let empty = Map::new();
	let mut tables = Vec::new();
	for section in SECTIONS {
		let of = |m: &'_ Map<String, Value>| m.get(section).and_then(Value::as_object).cloned().unwrap_or_default();
		let (mine, theirs) = (of(yours), of(base));
		let mut ids: Vec<&String> = mine.keys().chain(theirs.keys()).collect();
		ids.sort();
		ids.dedup();
		for id in ids {
			let unit = format!("{section}.{id}");
			let change = match (mine.get(id), theirs.get(id)) {
				(Some(_), None) => Change { kind: Kind::Added, unit, keys: Vec::new() },
				(None, Some(_)) => Change { kind: Kind::Removed, unit, keys: Vec::new() },
				(Some(a), Some(b)) if a != b => {
					let keys = keys_differing(a.as_object().unwrap_or(&empty), b.as_object().unwrap_or(&empty), |_| true);
					Change { kind: Kind::Changed, unit, keys }
				}
				_ => continue,
			};
			tables.push(change);
		}
	}
	// Changed, then added, then removed, each in the order of the sections.
	tables.sort_by_key(|c| c.kind);
	out.extend(tables);
	out
}

/// A settings file as JSON, its tables as they are: one file needs no merging.
fn as_json(text: &str) -> Result<Map<String, Value>, String> {
	let t: toml::Table = text.parse().map_err(|e: toml::de::Error| e.message().trim().to_string())?;
	match serde_json::to_value(t) {
		Ok(Value::Object(o)) => Ok(o),
		_ => Err("isn't a table".into()),
	}
}

/// The lines of `changes`, the keys in a column of their own.
fn lines_of(changes: &[Change]) -> String {
	let width = changes.iter().filter(|c| !c.keys.is_empty()).map(|c| c.unit.chars().count()).max().unwrap_or(0).max(19);
	let mut out = String::new();
	for c in changes {
		let line = format!("  {:<7}  {:<width$}  {}", c.kind.word(), c.unit, c.keys.join(", "));
		out.push_str(line.trim_end());
		out.push('\n');
	}
	out
}

/// `elamond defaults --diff`: where the settings of `place` differ from those elamond `version`
/// starts with, and what changed in those since the version whose defaults.toml.old is there.
fn defaults_diff(place: &Place, version: &str) -> Result<String, String> {
	let files = place.files();
	let sources = settings::read_files(&files).map_err(|m| settings::join_mistakes(&m))?;
	let (_, yours) = settings::read_merged(&sources, settings::main_in(&files, &sources))
		.map_err(|m| format!("{}\nthe settings have mistakes; elamond check names them", settings::join_mistakes(&m)))?;
	let defaults = as_json(DEFAULTS)?;
	let mut out = String::new();
	let mine = diff(&yours, &defaults);
	if mine.is_empty() {
		out.push_str(&format!("Your settings are the defaults of elamond {version}.\n"));
	} else {
		out.push_str(&format!("Your settings beside the defaults of elamond {version}:\n{}", lines_of(&mine)));
	}
	out.push('\n');
	let old_path = place.dir.join(migrate::DEFAULTS_OLD);
	let since = match fs::read_to_string(&old_path) {
		Ok(text) => {
			let before = migrate::version_of(&text).unwrap_or("the version before").to_string();
			let old = as_json(&text).map_err(|e| format!("{} doesn't parse: {e}", store::shown(&old_path)))?;
			let changed = diff(&defaults, &old);
			if changed.is_empty() {
				out.push_str(&format!("The defaults are the same as those of elamond {before} ({}).\n", migrate::DEFAULTS_OLD));
			} else {
				out.push_str(&format!("What changed in the defaults since elamond {before} ({}):\n{}", migrate::DEFAULTS_OLD, lines_of(&changed)));
			}
			changed
		}
		Err(_) => {
			out.push_str(&format!(
				"There is no {} yet: nothing to compare the defaults of elamond {version} with until another version starts here.\n",
				migrate::DEFAULTS_OLD
			));
			Vec::new()
		}
	};
	// A table worth a look: one the defaults changed or added, or one of the defaults yours changed.
	let worth = since
		.iter()
		.find(|c| c.kind != Kind::Removed && c.unit.contains('.'))
		.or_else(|| mine.iter().find(|c| c.kind != Kind::Added && c.unit.contains('.')));
	if let Some(c) = worth {
		out.push_str(&format!("\nCopy a table to start from: elamond defaults {}\n", c.unit));
	}
	Ok(out)
}

/// `elamond defaults TABLE`: the table `name` (`dashboards.sensors`) of the settings elamond starts
/// with, as TOML to copy into the settings, its lists of tables and comments with it.
fn defaults_table(name: &str) -> Result<String, String> {
	let doc: toml_edit::DocumentMut = DEFAULTS.parse().map_err(|e| format!("the defaults don't parse: {e}"))?;
	let found = name.split_once('.').filter(|(s, _)| SECTIONS.contains(s)).and_then(|(section, id)| {
		let s = doc.get(section)?.as_table()?;
		Some((section, s.key(id)?.clone(), s.get(id)?.clone()))
	});
	let Some((section, key, mut item)) = found else {
		let mut all = Vec::new();
		for section in SECTIONS {
			if let Some(t) = doc.get(section).and_then(toml_edit::Item::as_table) {
				all.extend(t.iter().map(|(id, _)| format!("{section}.{id}")));
			}
		}
		return Err(format!("no table \"{name}\" in the defaults; they have {}", all.join(", ")));
	};
	// Its own comment, not the heading of the part of the file it is in.
	if let Some(t) = item.as_table_mut() {
		let prefix = t.decor().prefix().and_then(|p| p.as_str()).unwrap_or("").to_string();
		t.decor_mut().set_prefix(crate::phones::split_own(&prefix).1.to_string());
	}
	let mut holder = toml_edit::Table::new();
	holder.set_implicit(true);
	holder.insert_formatted(&key, item);
	let mut out = toml_edit::DocumentMut::new();
	out.insert(section, toml_edit::Item::Table(holder));
	Ok(out.to_string())
}

/// `elamond values`: every address this computer has now, with its value, unit and format; then
/// the collections and how many items each has.
pub fn values() -> Result<(), String> {
	let config = store::load_config()?;
	let (interval, history_len, _) = crate::timing(&config);
	let mut c = collect::Collector::new(crate::consts::cadence::Ticks::new(crate::consts::cadence::CLI_SNAPSHOT));
	let all = crate::ui::Sources::ALL;
	c.read(true, all);
	std::thread::sleep(crate::consts::cadence::CLI_SNAPSHOT);
	let mut reading = c.read(true, all);
	reading.sample.links = crate::links::statuses(&config);
	let info = c.info("-", &crate::machine_name(&config), interval, history_len);
	let media = crate::control::volume_get().map(|v| crate::control::Media { players: Vec::new(), volume: Some(v) });
	let activity = crate::activity::Activity::load(&store::data_dir()).newest(crate::consts::limit::ACTIVITY_MAX);
	let v = values::read(&values::Reading {
		sample: &reading.sample,
		info: &info,
		media: media.as_ref(),
		processes: reading.processes.as_deref(),
		activity: &activity,
		actions: &config.actions,
	});
	let width = v.scalars.keys().map(|a| a.chars().count()).max().unwrap_or(0);
	let rows: Vec<(String, String)> = v
		.scalars
		.values()
		.map(|s| (crate::format::format(s.v.shown(), s.format, s.unit), s.unit.to_string()))
		.collect();
	let shown_width = rows.iter().map(|(t, _)| t.chars().count()).max().unwrap_or(0).min(40);
	println!("{:width$}  {:shown_width$}  {:8}  format", "address", "value", "unit");
	for ((address, s), (shown, unit)) in v.scalars.iter().zip(&rows) {
		println!("{address:width$}  {shown:shown_width$}  {unit:8}  {}", s.format);
	}
	println!();
	println!("Collections, for table and for_each (the fields are in docs/config.md):");
	for def in values::COLLECTIONS {
		let n = v.collections.get(def.name).map_or(0, Vec::len);
		println!("  {:10} {n:>4} {}  (role {})", def.name, if n == 1 { "item " } else { "items" }, def.role.as_str());
	}
	println!();
	println!("Processes and media are read only while a client shows them; here media is the volume alone.");
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::os::unix::fs::PermissionsExt;

	#[test]
	fn a_file_stands_in_its_place() {
		// Run with no settings directory of the user's: the main file is replaced, a drop-in joins.
		let dir = std::env::temp_dir().join(format!("elamond-commands-{}", std::process::id()));
		let d = dir.join(store::DROPIN_DIR);
		fs::create_dir_all(&d).unwrap();
		fs::write(d.join("x.toml"), "port = 2\n").unwrap();
		let other = dir.join("other.toml");
		fs::write(&other, "port = 3\n").unwrap();
		let place = Place { dir: dir.join("config"), named: None };
		let (s, main) = sources_with(&place, Some(&d.join("x.toml")), None).unwrap();
		assert!(s.iter().any(|s| s.name.ends_with("x.toml") && s.text == "port = 2\n") && !main);
		let (s, main) = sources_with(&place, Some(&other), Some("port = 4\n".into())).unwrap();
		assert!(s.iter().any(|s| s.name.ends_with("other.toml") && s.text == "port = 4\n") && main);
		assert!(sources_with(&place, Some(&dir.join("missing.toml")), None).is_err());
		fs::remove_dir_all(dir).unwrap();
	}

	/// Settings with no main file are the drop-ins alone; with one of 0.6.0, the defaults under it.
	#[test]
	fn the_main_file_says_what_is_under_the_drop_ins() {
		let dir = std::env::temp_dir().join(format!("elamond-main-{}", data_encoding::HEXLOWER.encode(&store::random_bytes::<6>())));
		let place = Place { dir: dir.clone(), named: None };
		fs::create_dir_all(place.dropins()).unwrap();
		fs::write(place.dropins().join("a.toml"), "[profiles.mine]\nname = \"Mine\"\n").unwrap();
		let read = || {
			let (s, main) = sources_with(&place, None, None).unwrap();
			settings::read_settings(&s, main).unwrap()
		};
		// No elamond.toml: nothing under the drop-in, though it says no schema.
		let c = read();
		assert!(c.dashboards.is_empty() && c.profiles.keys().eq(["mine"]));
		assert!(settings::warnings(&c).iter().any(|w| w.starts_with("there are no dashboards")));
		assert!(settings::warnings(&c).iter().any(|w| w.starts_with("default_profile \"default\" is none of the profiles (the profiles are mine)")));
		// One of 0.6.0: the defaults under it.
		fs::write(place.main(), "control = true\n").unwrap();
		let c = read();
		assert!(c.control && c.dashboards.contains_key("overview") && c.profiles.contains_key("mine"));
		// One of schema 3: nothing under it.
		fs::write(place.main(), "schema = 3\n").unwrap();
		assert!(read().dashboards.is_empty());
		fs::remove_dir_all(dir).unwrap();
	}

	#[test]
	fn the_defaults_beside_the_settings() {
		let dir = std::env::temp_dir().join(format!("elamond-diff-{}", data_encoding::HEXLOWER.encode(&store::random_bytes::<6>())));
		let place = Place { dir: dir.clone(), named: None };
		fs::create_dir_all(place.dropins()).unwrap();
		fs::write(place.main(), DEFAULTS.replace("control = false", "control = true").replace("theme = \"system\"", "theme = \"dark\"")).unwrap();
		fs::write(place.dropins().join("mine.toml"), "listen = [\"eno1\"]\n[profiles.owner]\nuse = \"default\"\n").unwrap();
		let out = defaults_diff(&place, "0.6.1").unwrap();
		assert_eq!(
			out,
			"Your settings beside the defaults of elamond 0.6.1:\n\
			 \x20 changed  (top level)          control, listen\n\
			 \x20 changed  profiles.default     theme\n\
			 \x20 added    profiles.owner\n\
			 \n\
			 There is no defaults.toml.old yet: nothing to compare the defaults of elamond 0.6.1 with until another version starts here.\n\
			 \n\
			 Copy a table to start from: elamond defaults profiles.default\n"
		);
		// The version before had other sensors, and a dashboard this one hasn't.
		let before = DEFAULTS.replace("label = \"Wireless\"", "label = \"Wi-Fi\"") + "\n[dashboards.extra]\ntitle = \"Extra\"\n";
		fs::write(place.dir.join(migrate::DEFAULTS_OLD), migrate::defaults_header("0.6.0") + &before).unwrap();
		let out = defaults_diff(&place, "0.6.1").unwrap();
		assert!(
			out.contains("What changed in the defaults since elamond 0.6.0 (defaults.toml.old):\n  changed  dashboards.sensors   widgets\n  removed  dashboards.extra\n\nCopy a table to start from: elamond defaults dashboards.sensors\n"),
			"{out}"
		);
		fs::remove_dir_all(dir).unwrap();
	}

	#[test]
	fn a_table_of_the_defaults_to_copy() {
		let sensors = defaults_table("dashboards.sensors").unwrap();
		assert!(sensors.starts_with("[dashboards.sensors]\ntitle = \"Sensors\"\n"), "{sensors}");
		assert_eq!(sensors.matches("[[dashboards.sensors.widgets]]").count(), 8);
		assert!(!sensors.contains("templates") && !sensors.contains("# ----"), "{sensors}");
		let profile = defaults_table("profiles.default").unwrap();
		assert!(profile.starts_with("[profiles.default]\n") && profile.contains("\n[profiles.default.android]\n"), "{profile}");
		let template = defaults_table("templates.sensor_table").unwrap();
		assert!(template.starts_with("[templates.sensor_table]\ntype = \"table\"\n"), "{template}");
		// Each reads as settings, beside the template it uses.
		let text = format!("schema = 3\n{template}\n{sensors}");
		settings::read_sources(&[Source::new("a.toml", text)]).unwrap();
		let e = defaults_table("dashboards.nope").unwrap_err();
		assert!(e.starts_with("no table \"dashboards.nope\" in the defaults; they have profiles.default, dashboards.header, dashboards.overview"), "{e}");
	}

	#[test]
	fn edits_are_written_as_private_as_the_settings() {
		let dir = std::env::temp_dir().join(format!("elamond-edit-test-{}", data_encoding::HEXLOWER.encode(&store::random_bytes::<6>())));
		fs::create_dir_all(&dir).unwrap();
		let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
		// A new file: only its owner reads it.
		let new = dir.join("new.toml");
		save(&new, "port = 1\n").unwrap();
		assert_eq!((mode(&new), fs::read_to_string(&new).unwrap().as_str()), (0o600, "port = 1\n"));
		// One that is there keeps its own, whatever the umask.
		let shared = dir.join("shared.toml");
		fs::write(&shared, "port = 2\n").unwrap();
		fs::set_permissions(&shared, fs::Permissions::from_mode(0o640)).unwrap();
		save(&shared, "port = 3\n").unwrap();
		assert_eq!((mode(&shared), fs::read_to_string(&shared).unwrap().as_str()), (0o640, "port = 3\n"));
		// An edit the settings refused, kept beside the file, no more readable than it; again over it.
		let private = dir.join("elamond.toml");
		fs::write(&private, "port = 4\n").unwrap();
		fs::set_permissions(&private, fs::Permissions::from_mode(0o600)).unwrap();
		let kept = keep_broken(&private, "port = \n").unwrap();
		assert_eq!((kept.clone(), mode(&kept)), (dir.join("elamond.toml.broken"), 0o600));
		keep_broken(&private, "port = x\n").unwrap();
		assert_eq!(fs::read_to_string(&kept).unwrap(), "port = x\n");
		// Nothing else left behind.
		let mut names: Vec<String> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
		names.sort();
		assert_eq!(names, ["elamond.toml", "elamond.toml.broken", "new.toml", "shared.toml"]);
		// The folder the copy is edited in: a new one each time, of a name of its own, the owner's alone.
		let (a, b) = (scratch_dir(&dir).unwrap(), scratch_dir(&dir).unwrap());
		assert_ne!(a, b);
		assert_eq!(mode(&a), 0o700);
		assert!(fs::DirBuilder::new().mode(0o700).create(&a).is_err());
		fs::remove_dir_all(dir).unwrap();
	}
}
