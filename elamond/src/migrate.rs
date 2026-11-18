//! What the service writes beside the settings itself: the starter elamond.toml where there is none,
//! and defaults.toml for reference.

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use crate::settings::{DASHBOARDS, DEFAULTS, EVENTS, STARTER};
use crate::store::{self, Place};

/// The standard dashboards' drop-in, which the starter comes with.
pub const DASHBOARDS_FILE: &str = "dashboards.toml";
/// And the one of `[events]` and the rules.
pub const EVENTS_FILE: &str = "events.toml";
/// The JSON Schema of the settings, beside them for editors (`#:schema` in the starter files).
pub const SCHEMA_FILE: &str = "elamond-schema.json";
/// The settings elamond starts with, beside the settings, for reference; and the version before's.
pub const DEFAULTS_FILE: &str = "defaults.toml";
pub const DEFAULTS_OLD: &str = "defaults.toml.old";

/// What the service does with the settings files as it starts, before it reads them: the starter
/// where there is no elamond.toml, and defaults.toml beside them. Each said in the journal.
pub fn at_start(place: &Place) {
	match &place.named {
		None => {
			let main = place.main();
			if fs::symlink_metadata(&main).is_err() {
				match write_starter(&main) {
					Ok(()) => eprintln!("settings: wrote {} and its {}/{DASHBOARDS_FILE}, the settings elamond starts with (control is off)", store::shown(&main), store::DROPIN_DIR),
					Err(e) => eprintln!("settings: can't write {}, the settings to start from: {e}", store::shown(&main)),
				}
			}
		}
		Some(_) => {
			if let Some(note) = read_beside(place) {
				eprintln!("settings: {note}");
			}
		}
	}
	for note in [old_main(place), write_defaults(&place.dir, env!("CARGO_PKG_VERSION")), write_schema(&place.dir)].into_iter().flatten() {
		eprintln!("settings: {note}");
	}
}

/// What to say of a main file with keys and no `schema`: one of elamond 0.6.0 held only what
/// differed from the settings built into it, and is now read as the whole of them. Nothing moves it
/// any more, but elamond 0.6.1 started once does.
pub fn old_main(place: &Place) -> Option<String> {
	let main = place.named.clone().unwrap_or_else(|| place.main());
	let table: toml::Table = fs::read_to_string(&main).ok()?.parse().ok()?;
	(!table.is_empty() && !table.contains_key("schema")).then(|| {
		format!(
			"{} has no `schema = 3`: if it is a file of elamond 0.6.0, which held only what differed from the settings built in, it is read now as the whole of them; start elamond 0.6.1 once to move it, then update",
			store::shown(&main)
		)
	})
}

/// How the user's own elamond.toml is read, when ELAMON_CONFIG names another and it is a file of
/// theirs (not a link NixOS or Home Manager made): after that one, as a drop-in would be.
pub fn read_beside(place: &Place) -> Option<String> {
	let named = place.named.as_ref()?;
	let own = place.dir.join(store::CONFIG_FILE);
	store::own_beside(&own, Some(named)).then(|| {
		format!(
			"{} is read after {}, which ELAMON_CONFIG names (the NixOS module's settings), and wins over it as a drop-in would; move it into {}/ to say so",
			store::shown(&own),
			named.display(),
			store::shown(&place.dropins())
		)
	})
}

/// elamond.d/dashboards.toml as the starter writes it.
pub fn starter_dashboards() -> String {
	format!("#:schema ../{SCHEMA_FILE}\n{DASHBOARDS}")
}

/// elamond.d/events.toml as the starter writes it.
pub fn starter_events() -> String {
	format!("#:schema ../{SCHEMA_FILE}\n{EVENTS}")
}

/// Writes the settings elamond starts with where there are none: the starter as the main file
/// `main`, and the standard dashboards and the rules as elamond.d/dashboards.toml and
/// elamond.d/events.toml beside it, each unless it is there.
/// Each begins with a `#:schema` line naming the JSON Schema beside them, for editors; never written
/// over a file, and only their owner's to read.
pub fn write_starter(main: &Path) -> io::Result<()> {
	let dir = main.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
	fs::create_dir_all(dir)?;
	let new = |path: &Path, text: String| -> io::Result<()> {
		let mut f = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
		f.write_all(text.as_bytes())
	};
	new(main, format!("#:schema {SCHEMA_FILE}\n{STARTER}"))?;
	let dropins = dir.join(store::DROPIN_DIR);
	fs::create_dir_all(&dropins)?;
	for (file, text) in [(DASHBOARDS_FILE, starter_dashboards()), (EVENTS_FILE, starter_events())] {
		match new(&dropins.join(file), text) {
			Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
			other => other?,
		}
	}
	Ok(())
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

/// Writes elamond-schema.json, the JSON Schema of the settings, into the config directory `dir`
/// where it isn't there or isn't this elamond's: read-only, for editors (`#:schema`). A link there
/// is left alone. What was done, when anything was.
pub fn write_schema(dir: &Path) -> Option<String> {
	let path = dir.join(SCHEMA_FILE);
	let schema = serde_json::to_string_pretty(&crate::settings::json_schema()).unwrap_or_default() + "\n";
	if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) || fs::read_to_string(&path).is_ok_and(|t| t == schema) {
		return None;
	}
	let shown = store::shown(&path);
	Some(match fs::create_dir_all(dir).and_then(|()| store::replace_as(&path, schema.as_bytes(), 0o444)) {
		Ok(()) => format!("wrote {shown}, the settings' schema for editors"),
		Err(e) => format!("can't write {shown}: {e}"),
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	fn scratch() -> Place {
		let dir = std::env::temp_dir().join(format!("elamond-migrate-{}", data_encoding::HEXLOWER.encode(&store::random_bytes::<6>())));
		fs::create_dir_all(&dir).unwrap();
		Place { dir, named: None }
	}

	#[test]
	fn the_starter_is_a_short_elamond_toml_and_the_dashboards_beside_it() {
		let place = scratch();
		write_starter(&place.main()).unwrap();
		let main = fs::read_to_string(place.main()).unwrap();
		let dashboards = fs::read_to_string(place.dropins().join(DASHBOARDS_FILE)).unwrap();
		assert!(main.starts_with("#:schema elamond-schema.json\n") && dashboards.starts_with("#:schema ../elamond-schema.json\n"));
		assert!(main.lines().count() < 80 && !main.contains("[dashboards.") && !main.contains("[templates."), "{main}");
		// Read as the service reads them, they are the settings elamond starts with, whole.
		let files: Vec<_> = place.files();
		let read = crate::settings::read_files(&files).unwrap();
		let (_, merged) = crate::settings::read_merged(&read).unwrap();
		let (_, whole) = crate::settings::read_merged(&[crate::settings::Source::new("defaults", DEFAULTS)]).unwrap();
		assert_eq!(merged, whole);
		// Never over a file: not the main one, nor a dashboards.toml of the person's own.
		assert!(write_starter(&place.main()).is_err());
		fs::remove_file(place.main()).unwrap();
		fs::write(place.dropins().join(DASHBOARDS_FILE), "# mine\n").unwrap();
		write_starter(&place.main()).unwrap();
		assert_eq!(fs::read_to_string(place.dropins().join(DASHBOARDS_FILE)).unwrap(), "# mine\n");
		// The schema the first lines name, beside them: written when it isn't there or is another.
		assert!(write_schema(&place.dir).unwrap().starts_with("wrote"));
		let schema: serde_json::Value = serde_json::from_str(&fs::read_to_string(place.dir.join(SCHEMA_FILE)).unwrap()).unwrap();
		assert_eq!(schema["title"], "elamond settings (schema 3)");
		assert_eq!(write_schema(&place.dir), None);
		fs::remove_file(place.dir.join(SCHEMA_FILE)).unwrap();
		assert!(write_schema(&place.dir).is_some() && place.dir.join(SCHEMA_FILE).exists());
		fs::remove_dir_all(&place.dir).unwrap();
	}

	#[test]
	fn a_file_of_0_6_0_is_named() {
		let place = scratch();
		assert_eq!(old_main(&place), None);
		for quiet in ["", "# nothing but a comment\n", "schema = 3\ncontrol = true\n"] {
			fs::write(place.main(), quiet).unwrap();
			assert_eq!(old_main(&place), None, "{quiet:?}");
		}
		fs::write(place.main(), "control = true\n").unwrap();
		assert!(old_main(&place).unwrap().contains("elamond 0.6.0"));
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
		assert_eq!(fs::read_to_string(&main).unwrap(), format!("#:schema {SCHEMA_FILE}\n{STARTER}"));
		assert_eq!(fs::metadata(&main).unwrap().permissions().mode() & 0o777, 0o600);
		// Never over one.
		fs::write(&main, "control = true\n").unwrap();
		assert!(write_starter(&main).is_err());
		assert_eq!(fs::read_to_string(&main).unwrap(), "control = true\n");
		fs::remove_dir_all(&place.dir).unwrap();
	}
}
