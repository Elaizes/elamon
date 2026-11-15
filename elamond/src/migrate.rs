//! What the service writes beside the settings itself: the starter elamond.toml where there is none,
//! and defaults.toml for reference.

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use crate::settings::DEFAULTS;
use crate::store::{self, Place};

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
					Ok(()) => eprintln!("settings: wrote {}, the settings elamond starts with (control is off)", store::shown(&main)),
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
	if let Some(note) = write_defaults(&place.dir, env!("CARGO_PKG_VERSION")) {
		eprintln!("settings: {note}");
	}
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

#[cfg(test)]
mod tests {
	use super::*;

	fn scratch() -> Place {
		let dir = std::env::temp_dir().join(format!("elamond-migrate-{}", data_encoding::HEXLOWER.encode(&store::random_bytes::<6>())));
		fs::create_dir_all(&dir).unwrap();
		Place { dir, named: None }
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
