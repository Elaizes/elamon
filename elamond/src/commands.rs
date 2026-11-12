//! The commands about the settings: `elamond check`, `edit`, `schema`, `defaults` and `values`.

use std::fs;
use std::io::{self, IsTerminal, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::settings::{self, Mistake, Source};
use crate::{collect, store, values};

/// The settings files as the service reads them, with `file` taking its place among them: in
/// place of the one it is, as a drop-in when it is in a folder named elamond.d, else in place of
/// the main file. Its text comes from `text` when given (an edited copy), from the file otherwise.
fn sources_with(file: Option<&Path>, text: Option<String>) -> Result<Vec<Source>, Vec<Mistake>> {
	let mut files = store::config_files();
	let Some(file) = file else { return settings::read_files(&files) };
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
	Ok(sources)
}

fn files_main() -> PathBuf {
	store::config_files().remove(0)
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
	let sources = sources_with(file.as_deref(), None).inspect_err(|m| tell(m)).map_err(|_| "the settings can't be read".to_string())?;
	match settings::read_sources(&sources) {
		Ok(c) => {
			let read: Vec<&str> = sources.iter().map(|s| s.name.as_str()).collect();
			let from = if read.is_empty() { "the built-in defaults alone".to_string() } else { format!("the built-in defaults, {}", read.join(", ")) };
			println!(
				"The settings are sound: {}, {}, {}, {}, from {from}.",
				count(c.dashboards.len(), "dashboard"),
				count(c.actions.len(), "action"),
				count(c.links.len(), "link"),
				count(c.profiles.len(), "profile")
			);
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
	let target = match args {
		[] => files_main(),
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
	if fs::symlink_metadata(&target).is_ok_and(|m| m.file_type().is_symlink()) && fs::canonicalize(&target).is_ok_and(|p| p.starts_with("/nix/store")) {
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
	let start = original.clone().unwrap_or_else(|| {
		"# elamond's settings (docs/config.md). `elamond defaults` prints the built-in dashboards and\n# profile; a table here with the same id changes them key by key.\n\n".into()
	});
	let result = edit_loop(&editor, &target, &copy, &start, original.as_deref());
	let _ = fs::remove_dir_all(&scratch);
	result
}

fn edit_loop(editor: &str, target: &Path, copy: &Path, start: &str, original: Option<&str>) -> Result<(), String> {
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
		let checked = sources_with(Some(target), Some(text.clone())).and_then(|s| settings::read_sources(&s));
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
	fs::metadata(path).map(|m| m.permissions().mode() & 0o777).unwrap_or(0o600)
}

/// Replaces `path` with `text` in one step, keeping its permissions (only its owner's for a new
/// file). The text is never in a file others may read where the settings themselves aren't.
fn save(path: &Path, text: &str) -> io::Result<()> {
	replace_as(path, text, mode_for(path))
}

/// Keeps an edit the settings refused beside `target`, as `<name>.broken`, as private as it.
fn keep_broken(target: &Path, text: &str) -> io::Result<PathBuf> {
	let kept = target.with_file_name(format!("{}.broken", target.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()));
	replace_as(&kept, text, mode_for(target))?;
	Ok(kept)
}

/// Writes `text` to `path` in one step, the file having `mode`: through a new file beside it of a
/// name of its own, made with that mode, renamed over `path`.
fn replace_as(path: &Path, text: &str, mode: u32) -> io::Result<()> {
	let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
	let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
	let tag = data_encoding::HEXLOWER.encode(&store::random_bytes::<4>());
	let tmp = dir.join(format!(".{name}.{}.{tag}.saving", std::process::id()));
	let written = (|| {
		let mut f = fs::OpenOptions::new().write(true).create_new(true).mode(mode).open(&tmp)?;
		f.write_all(text.as_bytes())?;
		// The umask may have taken some of the mode away: the file is to have it whole.
		f.set_permissions(fs::Permissions::from_mode(mode))?;
		f.sync_all()?;
		fs::rename(&tmp, path)
	})();
	if written.is_err() {
		let _ = fs::remove_file(&tmp);
	}
	written
}

/// `elamond schema`: the JSON Schema of a settings file.
pub fn schema() -> Result<(), String> {
	println!("{}", serde_json::to_string_pretty(&settings::json_schema()).unwrap_or_default());
	Ok(())
}

/// `elamond defaults`: the built-in dashboards and profile.
pub fn defaults() -> Result<(), String> {
	print!("{}", settings::DEFAULTS);
	Ok(())
}

/// `elamond values`: every address this computer has now, with its value, unit and format; then
/// the collections and how many items each has.
pub fn values() -> Result<(), String> {
	let config = store::load_config()?;
	let (interval, history_len, _) = crate::timing(&config);
	let mut c = collect::Collector::new(crate::consts::cadence::Ticks::new(crate::consts::cadence::CLI_SNAPSHOT));
	let all = crate::ui::Sources { processes: true, media: true, volume: true, gpus: true, displays: true, links: true };
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

	#[test]
	fn a_file_stands_in_its_place() {
		// Run with no settings directory of the user's: the main file is replaced, a drop-in joins.
		let dir = std::env::temp_dir().join(format!("elamond-commands-{}", std::process::id()));
		let d = dir.join(store::DROPIN_DIR);
		fs::create_dir_all(&d).unwrap();
		fs::write(d.join("x.toml"), "port = 2\n").unwrap();
		let other = dir.join("other.toml");
		fs::write(&other, "port = 3\n").unwrap();
		let s = sources_with(Some(&d.join("x.toml")), None).unwrap();
		assert!(s.iter().any(|s| s.name.ends_with("x.toml") && s.text == "port = 2\n"));
		let s = sources_with(Some(&other), Some("port = 4\n".into())).unwrap();
		assert!(s.iter().any(|s| s.name.ends_with("other.toml") && s.text == "port = 4\n"));
		assert!(sources_with(Some(&dir.join("missing.toml")), None).is_err());
		fs::remove_dir_all(dir).unwrap();
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
