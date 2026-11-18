//! `elamond backup` and `elamond restore`: what makes this service the same computer to the phones
//! paired with it — its id, its certificate and key, the paired phones, the activity log and
//! `elamond control off` — in one file, and the settings files beside them, which go back only when
//! asked.
//!
//! A backup is three parts: the line "elamond-backup", a line of JSON (`Header`) with the payload's
//! SHA-256, and the payload, JSON (`Payload`). It isn't encrypted: the file is written for its owner
//! alone to read. elamond 0.6.1 and older encrypted backups with a passphrase unless told `--plain`;
//! those are refused, saying so.
//!
//! A restore checks everything before it writes anything, then writes each file in one step.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::consts::backup::FILE_MAX;
use crate::{activity, identity, listen, store};

const B64: data_encoding::Encoding = data_encoding::BASE64;

/// A backup's first line.
const MAGIC: &[u8] = b"elamond-backup\n";
/// The format written, and the newest read: one that changes what an older elamond would get
/// wrong raises it.
pub const FORMAT: u32 = 1;
/// How the payload is kept, as the header says: as it is. 0.6.1 and older wrote others too.
const PLAIN: &str = "none";

/// The state files a backup carries, in the order a restore writes them: control-off.json first, so
/// a computer whose control was held off is never open in between; the identity before the phones
/// that trust it; the key before the certificate, as a running service takes up a new cert.der with
/// the key beside it. The pending pairing code is never carried.
const STATE_FILES: [&str; 6] = [
	store::CONTROL_OFF_FILE,
	identity::ID_FILE,
	identity::KEY_FILE,
	identity::CERT_FILE,
	store::CLIENTS_FILE,
	activity::FILE,
];

/// State files older elamonds carried that this one no longer uses: left out of a restore, as
/// nothing reads them, and not named as a newer elamond's. links.json: 0.5's links added from the
/// app; rotations.json and next-identity.json: 0.6.1's signed certificate rotations.
const RETIRED_FILES: [&str; 3] = ["links.json", "rotations.json", "next-identity.json"];

/// The second line of a backup.
#[derive(Serialize, Deserialize)]
struct Header {
	format: u32,
	/// `PLAIN`.
	encryption: String,
	/// The payload's SHA-256, hex: a file damaged on the way is noticed. 0.6.1's encrypted backups
	/// have none.
	#[serde(default)]
	sha256: Option<String>,
}

/// The rest of a backup, once opened.
#[derive(Serialize, Deserialize)]
struct Payload {
	/// The elamond that made it, when (Unix time) and on which computer (its host name).
	service_version: String,
	created: u64,
	host: String,
	/// The state files by name, in base64.
	state: BTreeMap<String, String>,
	/// The settings files as text, by where they are in the config directory: "elamond.toml" for
	/// the main one, wherever it was read from, "elamond.d/NAME.toml" for drop-ins.
	settings: BTreeMap<String, String>,
}

/// What a backup holds.
pub struct Contents {
	pub service_version: String,
	pub created: u64,
	pub host: String,
	pub state: BTreeMap<String, Vec<u8>>,
	pub settings: BTreeMap<String, String>,
}

/// The state in the data directory `dir`, and the settings files `settings` (each with its name in
/// the backup) that are there. Without an identity there is nothing that makes this computer itself.
pub fn collect(dir: &Path, settings: &[(String, PathBuf)]) -> Result<Contents, String> {
	let mut state = BTreeMap::new();
	for name in STATE_FILES {
		let path = dir.join(name);
		match fs::read(&path) {
			Ok(b) => {
				state.insert(name.to_string(), b);
			}
			Err(e) if e.kind() == io::ErrorKind::NotFound => {}
			Err(e) => return Err(format!("can't read {}: {e}", path.display())),
		}
	}
	if ![identity::ID_FILE, identity::CERT_FILE, identity::KEY_FILE].iter().all(|n| state.contains_key(*n)) {
		return Err(format!(
			"there is no service identity in {} to back up; the service makes one when it first starts",
			dir.display()
		));
	}
	let mut texts = BTreeMap::new();
	for (name, path) in settings {
		match fs::read_to_string(path) {
			Ok(t) => {
				texts.insert(name.clone(), t);
			}
			Err(e) if e.kind() == io::ErrorKind::NotFound => {}
			Err(e) => return Err(format!("can't read {}: {e}", path.display())),
		}
	}
	Ok(Contents {
		service_version: env!("CARGO_PKG_VERSION").into(),
		created: store::now(),
		host: sysinfo::System::host_name().unwrap_or_default(),
		state,
		settings: texts,
	})
}

/// The settings files read now, each with its name in a backup.
fn settings_now() -> Vec<(String, PathBuf)> {
	let dir = store::config_dir();
	store::config_files()
		.into_iter()
		.enumerate()
		.map(|(i, path)| {
			let name = match path.strip_prefix(&dir) {
				_ if i == 0 => store::CONFIG_FILE.to_string(),
				Ok(rel) => rel.to_string_lossy().into_owned(),
				Err(_) => path.to_string_lossy().into_owned(),
			};
			(name, path)
		})
		.collect()
}

/// The backup of `c`, as written to its file.
pub fn pack(c: &Contents) -> Result<Vec<u8>, String> {
	let payload = Payload {
		service_version: c.service_version.clone(),
		created: c.created,
		host: c.host.clone(),
		state: c.state.iter().map(|(k, v)| (k.clone(), B64.encode(v))).collect(),
		settings: c.settings.clone(),
	};
	let body = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;
	let header = Header { format: FORMAT, encryption: PLAIN.into(), sha256: Some(store::sha256_hex(&body)) };
	let mut out = MAGIC.to_vec();
	out.extend(serde_json::to_vec(&header).map_err(|e| e.to_string())?);
	out.push(b'\n');
	out.extend(body);
	Ok(out)
}

const DAMAGED: &str = "the backup is damaged";

/// What the backup `bytes` holds. A newer format is refused before anything else in it is looked
/// at.
pub fn unpack(bytes: &[u8]) -> Result<Contents, String> {
	let rest = bytes.strip_prefix(MAGIC).ok_or("this isn't a backup made by elamond backup")?;
	let end = rest.iter().position(|b| *b == b'\n').ok_or(DAMAGED)?;
	let value: serde_json::Value = serde_json::from_slice(&rest[..end]).map_err(|_| DAMAGED)?;
	let format = value.get("format").and_then(serde_json::Value::as_u64).ok_or(DAMAGED)?;
	if format > u64::from(FORMAT) {
		return Err(format!(
			"this backup was made by a newer elamond, in format {format}; this one reads format {FORMAT} and before. Update elamond, then restore"
		));
	}
	let header: Header = serde_json::from_value(value).map_err(|_| DAMAGED)?;
	if header.encryption != PLAIN {
		return Err("this backup is encrypted with a passphrase, as elamond 0.6.1 and older made them by default; this \
			 elamond reads only unencrypted backups. Restore it with elamond 0.6.1, or make a new backup"
			.into());
	}
	let body = &rest[end + 1..];
	if header.sha256.as_deref() != Some(store::sha256_hex(body).as_str()) {
		return Err(format!("{DAMAGED}: it doesn't match the checksum it was written with"));
	}
	let payload: Payload = serde_json::from_slice(body).map_err(|e| format!("{DAMAGED} ({e})"))?;
	let mut state = BTreeMap::new();
	for (name, data) in payload.state {
		let bytes = B64.decode(data.as_bytes()).map_err(|_| format!("{DAMAGED}: {name} in it doesn't read"))?;
		state.insert(name, bytes);
	}
	Ok(Contents { service_version: payload.service_version, created: payload.created, host: payload.host, state, settings: payload.settings })
}

/// What a backup will put back, as checked.
#[derive(Debug)]
pub struct Summary {
	pub id: String,
	pub fingerprint: String,
	pub phones: Vec<store::Client>,
	/// Entries in the activity log.
	pub activity: usize,
	pub control_off: bool,
	/// Files in the backup this elamond doesn't know, which are left out.
	pub unknown: Vec<String>,
}

/// Checks that everything in `c` reads as the service will read it, and the settings too
/// `with_settings`: nothing is written unless all of it does.
pub fn verify(c: &Contents, with_settings: bool) -> Result<Summary, String> {
	let file = |name: &str| c.state.get(name).map(|b| b.as_slice()).filter(|b| !b.is_empty());
	let id = file(identity::ID_FILE)
		.map(|b| String::from_utf8_lossy(b).trim().to_string())
		.filter(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric()))
		.ok_or("the backup has no service id it could put back")?;
	let (Some(cert), Some(key)) = (file(identity::CERT_FILE), file(identity::KEY_FILE)) else {
		return Err("the backup lacks the certificate or its key".into());
	};
	crate::tls_with(cert, key).map_err(|e| format!("the certificate and key in the backup don't load together ({e})"))?;
	let phones = parsed::<Vec<store::Client>>(c, store::CLIENTS_FILE)?.unwrap_or_default();
	let activity = file(activity::FILE)
		.map(|b| String::from_utf8_lossy(b).lines().filter(|l| serde_json::from_str::<activity::Entry>(l).is_ok()).count())
		.unwrap_or(0);
	if with_settings && !c.settings.is_empty() {
		if let Some(name) = c.settings.keys().find(|n| !settings_name(n)) {
			return Err(format!("the backup names a settings file \"{name}\" outside the settings folder; it wasn't made by elamond backup"));
		}
		// As the service reads them: the main file, then the drop-ins by name, each over the ones before.
		let main = c.settings.get_key_value(store::CONFIG_FILE);
		let sources: Vec<crate::settings::Source> = main
			.into_iter()
			.chain(c.settings.iter().filter(|(n, _)| n.as_str() != store::CONFIG_FILE))
			.map(|(n, t)| crate::settings::Source::new(format!("{n} in the backup"), t.as_str()))
			.collect();
		crate::settings::read_sources(&sources).map_err(|m| crate::settings::join_mistakes(&m))?;
	}
	Ok(Summary {
		id,
		fingerprint: store::sha256_hex(cert),
		phones,
		activity,
		control_off: c.state.contains_key(store::CONTROL_OFF_FILE),
		unknown: c.state.keys().filter(|k| !STATE_FILES.contains(&k.as_str()) && !RETIRED_FILES.contains(&k.as_str())).cloned().collect(),
	})
}

/// The state file `name` in `c` read as the service reads it, if `c` has it.
fn parsed<T: serde::de::DeserializeOwned>(c: &Contents, name: &str) -> Result<Option<T>, String> {
	c.state
		.get(name)
		.map(|b| serde_json::from_slice(b).map_err(|e| format!("{name} in the backup doesn't read ({e})")))
		.transpose()
}

/// Whether a settings file's name in a backup is one `elamond backup` gives: the main file, or a
/// drop-in right in the drop-in folder.
fn settings_name(name: &str) -> bool {
	if name == store::CONFIG_FILE {
		return true;
	}
	let Some((folder, file)) = name.split_once('/') else { return false };
	folder == store::DROPIN_DIR
		&& !file.contains('/')
		&& !file.starts_with('.')
		&& file.strip_suffix(".toml").is_some_and(|stem| !stem.is_empty())
}

/// Where restored settings go: the config directory, and the main file, which ELAMON_CONFIG may
/// name elsewhere.
pub struct SettingsPlace {
	pub dir: PathBuf,
	pub main: PathBuf,
}

impl SettingsPlace {
	fn now() -> Self {
		let dir = store::config_dir();
		let main = std::env::var_os("ELAMON_CONFIG").map(PathBuf::from).unwrap_or_else(|| dir.join(store::CONFIG_FILE));
		SettingsPlace { dir, main }
	}

	fn target(&self, name: &str) -> PathBuf {
		if name == store::CONFIG_FILE { self.main.clone() } else { self.dir.join(name) }
	}
}

/// What a restore did.
#[derive(Debug, Default)]
pub struct Restored {
	pub settings_written: Vec<PathBuf>,
	/// Settings files left as they were, with why.
	pub settings_skipped: Vec<String>,
	/// Drop-ins there already that the backup doesn't have, which still apply.
	pub settings_kept: Vec<PathBuf>,
	/// The standard dashboards a fresh start wrote beside the settings, taken out: the backup's
	/// settings, whole without them, would be read under them.
	pub settings_removed: Vec<PathBuf>,
}

/// Puts `c` back in the data directory `dir`, and its settings in `settings` when given. Files
/// that are already as in the backup are left alone, each of the others is written in one step, and
/// state files the backup doesn't have are taken away, but for `elamond control off`'s: control
/// held off stays so. The service is stopped (or `--force`d), so nothing is kept to undo a write
/// that fails halfway: restore again.
pub fn restore_into(c: &Contents, dir: &Path, settings: Option<&SettingsPlace>) -> Result<Restored, String> {
	let mut restored = Restored::default();
	let written = (|| -> io::Result<()> {
		store::private_dir(dir)?;
		for name in STATE_FILES {
			let path = dir.join(name);
			match c.state.get(name) {
				Some(data) => {
					put(&path, data, true)?;
				}
				// Control held off here stays held off: a restore never lets go of it.
				None if name == store::CONTROL_OFF_FILE => {}
				None => match fs::remove_file(&path) {
					Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
					_ => {}
				},
			}
		}
		let Some(place) = settings else { return Ok(()) };
		for (name, text) in &c.settings {
			let target = place.target(name);
			if let Some(why) = managed_elsewhere(&target) {
				restored.settings_skipped.push(format!("{name}: {} is {why}; left as it is", target.display()));
				continue;
			}
			if put(&target, text.as_bytes(), false)? {
				restored.settings_written.push(target);
			}
		}
		// A fresh install's elamond.d/dashboards.toml, untouched, over settings that never had one
		// (a whole elamond.toml of 0.6.1, say) would put the standard dashboards back over theirs.
		let dashboards = format!("{}/{}", store::DROPIN_DIR, crate::migrate::DASHBOARDS_FILE);
		let stock = place.dir.join(&dashboards);
		if c.settings.contains_key(store::CONFIG_FILE)
			&& !c.settings.contains_key(&dashboards)
			&& fs::read_to_string(&stock).is_ok_and(|t| t == crate::migrate::starter_dashboards())
		{
			fs::remove_file(&stock)?;
			restored.settings_removed.push(stock);
		}
		// The same of its elamond.d/events.toml: settings that had no rules get none from it.
		let events = format!("{}/{}", store::DROPIN_DIR, crate::migrate::EVENTS_FILE);
		let stock = place.dir.join(&events);
		if c.settings.contains_key(store::CONFIG_FILE)
			&& !c.settings.contains_key(&events)
			&& fs::read_to_string(&stock).is_ok_and(|t| t == crate::migrate::starter_events())
		{
			fs::remove_file(&stock)?;
			restored.settings_removed.push(stock);
		}
		if let Ok(entries) = fs::read_dir(place.dir.join(store::DROPIN_DIR)) {
			let mut kept: Vec<PathBuf> = entries
				.flatten()
				.filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
				.filter(|e| !c.settings.contains_key(&format!("{}/{}", store::DROPIN_DIR, e.file_name().to_string_lossy())))
				.map(|e| e.path())
				.collect();
			kept.sort();
			restored.settings_kept.extend(kept);
		}
		Ok(())
	})();
	written.map(|()| restored).map_err(|e| e.to_string())
}

/// Writes `data` to `path` unless it holds that already, and says whether it wrote. A state file's
/// folder is made private; a settings file's is made if it isn't there, and left as it is.
fn put(path: &Path, data: &[u8], state: bool) -> io::Result<bool> {
	if fs::read(path).is_ok_and(|now| now == data) {
		return Ok(false);
	}
	if state {
		store::write_private(path, data)?;
	} else {
		if let Some(dir) = path.parent() {
			fs::create_dir_all(dir)?;
		}
		store::replace_as(path, data, 0o600)?;
	}
	Ok(true)
}

/// Why a settings file can't be put back where it is, if it can't: NixOS or Home Manager make it,
/// or it isn't the user's to write.
fn managed_elsewhere(path: &Path) -> Option<String> {
	use std::os::unix::ffi::OsStrExt;
	if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
		return Some("a link, as NixOS and Home Manager make: set it in their services.elamond.settings".into());
	}
	// The file, or the nearest folder above it that is there.
	let there = path.ancestors().find(|p| p.exists())?;
	if fs::canonicalize(there).is_ok_and(|p| p.starts_with("/nix/store")) {
		return Some("in the Nix store: set it in services.elamond.settings".into());
	}
	let c = std::ffi::CString::new(there.as_os_str().as_bytes()).ok()?;
	// SAFETY: access only reads the NUL-terminated path it is given.
	(unsafe { libc::access(c.as_ptr(), libc::W_OK) } != 0).then(|| "not writable".into())
}

// ---- the commands ------------------------------------------------------------------------------

#[derive(Debug, Default, PartialEq)]
struct Args {
	file: Option<PathBuf>,
	force: bool,
	with_settings: bool,
	with_old_phones: bool,
}

const BACKUP_USAGE: &str = "usage: elamond backup FILE";
const RESTORE_USAGE: &str = "usage: elamond restore FILE [--with-settings] [--with-old-phones] [--force]";

impl Args {
	fn parse(restore: bool, args: &[String]) -> Result<Args, String> {
		let usage = if restore { RESTORE_USAGE } else { BACKUP_USAGE };
		let mut a = Args::default();
		for arg in args {
			match arg.as_str() {
				// What 0.6.1 wanted for a backup like those made now; scripts and timers still say it.
				"--plain" if !restore => {}
				"--force" if restore => a.force = true,
				"--with-settings" if restore => a.with_settings = true,
				"--with-old-phones" if restore => a.with_old_phones = true,
				s if s.starts_with('-') => return Err(format!("unknown option \"{s}\"\n{usage}")),
				s if a.file.is_none() => a.file = Some(s.into()),
				_ => return Err(usage.into()),
			}
		}
		if a.file.is_none() {
			return Err(usage.into());
		}
		Ok(a)
	}
}

/// `elamond backup FILE`.
pub fn backup(args: &[String]) -> Result<(), String> {
	let a = Args::parse(false, args)?;
	let file = a.file.expect("checked");
	if file.is_dir() {
		return Err(format!("{} is a folder; give the file to write, like {}/elamond.backup", file.display(), file.display()));
	}
	let dir = store::data_dir();
	let contents = collect(&dir, &settings_now())?;
	// What can't be restored isn't worth keeping: say so now rather than on the day it's needed.
	let summary = verify(&contents, false).map_err(|e| format!("{e}\n  in {}; fix it, then back up", dir.display()))?;
	store::replace_as(&file, &pack(&contents)?, 0o600).map_err(|e| format!("can't write {}: {e}", file.display()))?;

	println!("Backed up to {}:", file.display());
	print_summary(&summary);
	let settings: Vec<&str> = contents.settings.keys().map(String::as_str).collect();
	if !settings.is_empty() {
		println!("  Settings:      {} (put back only by elamond restore --with-settings)", settings.join(", "));
	}
	println!();
	println!("  It isn't encrypted; only you can read it (0600). Whoever reads it can pose as this computer to");
	println!("  its phones: keep it out of shared and cloud folders.");
	println!("  Put it back, on this computer or a new one: elamond restore {}", file.display());
	Ok(())
}

/// `elamond restore FILE`.
pub fn restore(args: &[String]) -> Result<(), String> {
	let a = Args::parse(true, args)?;
	let file = a.file.expect("checked");
	let dir = store::data_dir();
	let running = in_use(&dir);
	if let (Some(why), false) = (&running, a.force) {
		return Err(still_running(why, &file));
	}
	let contents = unpack(&read_backup(&file)?)?;
	let summary = verify(&contents, a.with_settings)?;
	// Phones unpaired since the backup was made don't come back unasked.
	let back = returning(&summary.phones, &dir);
	if !back.is_empty() && !a.with_old_phones {
		let names: Vec<String> = back.iter().map(|c| format!("{}  {} ({})", c.id, c.name, c.role.as_str())).collect();
		return Err(format!(
			"this backup would bring back phones that aren't paired here now:\n    {}\n  \
			 One unpaired since (lost, stolen or replaced) would get in again with its old key. If they all\n  \
			 should, restore with --with-old-phones; nothing was changed.",
			names.join("\n    ")
		));
	}

	// Held while writing: a service that starts meanwhile waits for the restore to end.
	let lock = match store::lock_data_dir(&dir) {
		Ok(lock) => lock,
		Err(e) => return Err(format!("can't lock {}: {e}", dir.display())),
	};
	if lock.is_none() && !a.force {
		return Err(still_running("it started while the backup was read", &file));
	}
	let place = a.with_settings.then(SettingsPlace::now);
	// A running service (--force) doesn't add a phone meanwhile.
	let clients = store::lock_clients(&dir).map_err(|e| format!("can't lock {}: {e}", dir.join(store::CLIENTS_LOCK_FILE).display()))?;
	let restored = restore_into(&contents, &dir, place.as_ref())?;
	drop(clients);
	drop(lock);

	let host = if contents.host.is_empty() { String::new() } else { format!(" on {}", contents.host) };
	println!(
		"Restored {} from {}, made{host} on {} by elamond {}:",
		dir.display(),
		file.display(),
		crate::format_date(contents.created),
		contents.service_version
	);
	print_summary(&summary);
	if !summary.unknown.is_empty() {
		println!("  Left out:      {} (from a newer elamond; this one doesn't use them)", summary.unknown.join(", "));
	}
	if a.with_settings {
		for path in &restored.settings_written {
			println!("  Settings:      {}", path.display());
		}
		if restored.settings_written.is_empty() && restored.settings_skipped.is_empty() {
			println!("  Settings:      already as in the backup");
		}
		for note in &restored.settings_skipped {
			println!("  Not restored:  {note}");
		}
		for path in &restored.settings_removed {
			println!("  Taken out:     {}, the standard dashboards of a fresh start; the backup's settings have their own", path.display());
		}
		for path in &restored.settings_kept {
			println!("  Kept:          {}, which the backup doesn't have", path.display());
		}
	} else if !contents.settings.is_empty() {
		let names: Vec<&str> = contents.settings.keys().map(String::as_str).collect();
		println!("  Settings:      not restored: {} (--with-settings puts them back)", names.join(", "));
	}
	if !summary.control_off && store::control_held_off(&dir) {
		println!("  Control:       still held off by `elamond control off`, which a restore never lets go of");
	}
	println!();
	println!("Phones paired with this computer connect as before, without pairing again.");
	if running.is_some() {
		println!();
		println!("The service was running. It presents the restored certificate within seconds and knows the");
		println!("restored phones at once, but keeps the id and activity log it started with until it restarts.");
		println!("Restart it now: systemctl --user restart elamond");
	} else {
		println!("Start the service: systemctl --user start elamond");
	}
	Ok(())
}

/// The backup at `path`, read once: refused unless it is a regular file of `FILE_MAX` bytes at
/// most. A FIFO or a device is refused without waiting on it.
fn read_backup(path: &Path) -> Result<Vec<u8>, String> {
	use std::io::Read;
	let failed = |e: io::Error| format!("can't read {}: {e}", path.display());
	let f = fs::OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY).open(path).map_err(failed)?;
	if !f.metadata().map_err(failed)?.is_file() {
		return Err(format!("{} isn't a file", path.display()));
	}
	let mut bytes = Vec::new();
	f.take(FILE_MAX + 1).read_to_end(&mut bytes).map_err(failed)?;
	if bytes.len() as u64 > FILE_MAX {
		return Err(format!("{} is larger than {} bytes, far more than a backup; it isn't one", path.display(), FILE_MAX));
	}
	Ok(bytes)
}

/// The phones of a backup (`phones`) that aren't paired in `dir` now, the same id with the same
/// key: none where no phone has been paired yet (a fresh install), all of them where the list
/// there doesn't read.
fn returning(phones: &[store::Client], dir: &Path) -> Vec<store::Client> {
	if !dir.join(store::CLIENTS_FILE).exists() {
		return Vec::new();
	}
	match store::load_clients_in(dir) {
		Ok(now) => phones.iter().filter(|b| !now.iter().any(|n| n.id == b.id && n.token_hash == b.token_hash)).cloned().collect(),
		Err(_) => phones.to_vec(),
	}
}

fn print_summary(s: &Summary) {
	println!("  Computer id:   {}", s.id);
	println!("  Certificate:   {}", store::pretty_fingerprint(&s.fingerprint));
	let phones: Vec<String> = s
		.phones
		.iter()
		.map(|c| match &c.profile {
			Some(p) => format!("{} ({}, {}, profile {p})", c.name, c.id, c.role.as_str()),
			None => format!("{} ({}, {})", c.name, c.id, c.role.as_str()),
		})
		.collect();
	if phones.is_empty() {
		println!("  Phones:        none paired");
	} else {
		println!("  Phones:        {}", phones.join("\n                 "));
	}
	println!("  Activity log:  {} entries", s.activity);
	if s.control_off {
		println!("  Control:       held off by `elamond control off`");
	}
}

/// Why the service looks to be running with the data directory `dir`, if it does: it holds the
/// directory's lock, or, for one from before the lock, something answers where it would.
fn in_use(dir: &Path) -> Option<String> {
	if store::data_dir_held(dir) {
		return Some(format!("it holds {}", dir.display()));
	}
	let config = store::load_config().ok()?;
	let plan = listen::Plan::of(&config.listen, &listen::interfaces());
	let probe = plan.probe_addrs();
	(!probe.is_empty() && crate::service_running(&probe, config.port)).then(|| format!("something answers on its port, {}", config.port))
}

fn still_running(why: &str, file: &Path) -> String {
	format!(
		"the Elamon service is running ({why}). Stop it, restore, and start it again:\n  \
		 systemctl --user stop elamond\n  \
		 elamond restore {}\n  \
		 systemctl --user start elamond\n\
		 or restore under it with --force and restart it right after.",
		file.display()
	)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::access::Role;
	use std::os::unix::fs::PermissionsExt;

	fn scratch() -> PathBuf {
		let dir = std::env::temp_dir().join(format!("elamond-backup-{}", data_encoding::HEXLOWER.encode(&store::random_bytes::<6>())));
		fs::create_dir_all(&dir).unwrap();
		dir
	}

	fn client(name: &str, token: &str, role: Role) -> store::Client {
		store::Client { id: store::sha256_hex(name.as_bytes())[..8].into(), name: name.into(), token_hash: store::sha256_hex(token.as_bytes()), created: 1, role, ..Default::default() }
	}

	/// A service that has been used: two phones, a log, control held off.
	fn used_service() -> PathBuf {
		let dir = scratch();
		identity::Identity::load_or_create(&dir).unwrap();
		store::save_clients_in(&dir, &[client("Pixel", "token-a", Role::Admin), client("Tablet", "token-b", Role::Glance)]).unwrap();
		let log = activity::Activity::load(&dir);
		for i in 0..3 {
			log.record(activity::Entry { t: i, phone: "Pixel".into(), client_id: "a".into(), what: "power lock".into(), result: "ok".into(), message: None, family: None });
		}
		store::set_control_off(&dir, true).unwrap();
		dir
	}

	/// Every file in `dir` with its contents, to tell that nothing changed.
	fn listing(dir: &Path) -> BTreeMap<String, Vec<u8>> {
		let mut out = BTreeMap::new();
		for e in fs::read_dir(dir).unwrap().flatten() {
			let p = e.path();
			let name = p.strip_prefix(dir).unwrap().to_string_lossy().into_owned();
			if p.is_dir() {
				for (k, v) in listing(&p) {
					out.insert(format!("{name}/{k}"), v);
				}
			} else {
				out.insert(name, fs::read(&p).unwrap());
			}
		}
		out
	}

	#[test]
	fn a_backup_brings_the_same_computer_back_on_a_fresh_install() {
		let from = used_service();
		let before = identity::Identity::load(&from).unwrap();
		let token_hash = store::load_clients_in(&from).unwrap()[0].token_hash.clone();
		let bytes = pack(&collect(&from, &[]).unwrap()).unwrap();
		// What elamond 0.6.1 wrote with --plain, and reads.
		assert!(bytes.starts_with(b"elamond-backup\n{\"format\":1,\"encryption\":\"none\",\"sha256\":\""));

		// A fresh install: the service made an identity of its own.
		let to = scratch();
		identity::Identity::load_or_create(&to).unwrap();
		let contents = unpack(&bytes).unwrap();
		assert_eq!(contents.service_version, env!("CARGO_PKG_VERSION"));
		let summary = verify(&contents, false).unwrap();
		assert_eq!((summary.id.as_str(), summary.fingerprint.as_str()), (before.id.as_str(), before.fingerprint.as_str()));
		assert_eq!((summary.phones.len(), summary.activity, summary.control_off), (2, 3, true));
		restore_into(&contents, &to, None).unwrap();

		// The same computer: id, certificate, key and every phone's token.
		let after = identity::Identity::load_or_create(&to).unwrap();
		assert_eq!((&after.id, &after.fingerprint, &after.key_der), (&before.id, &before.fingerprint, &before.key_der));
		for name in STATE_FILES.iter().filter(|n| from.join(n).exists()) {
			assert_eq!(fs::read(to.join(name)).unwrap(), fs::read(from.join(name)).unwrap(), "{name}");
			assert_eq!(fs::metadata(to.join(name)).unwrap().permissions().mode() & 0o777, 0o600, "{name}");
		}
		assert!(store::load_clients_in(&to).unwrap().iter().any(|c| c.token_hash == token_hash));
		assert!(store::control_held_off(&to));
		// No temporary files, and no copies of what was there.
		assert!(!listing(&to).keys().any(|k| k.ends_with(".saving") || k.contains("before-restore")), "{:?}", listing(&to).keys());

		// Restored again: nothing more to do.
		let written = listing(&to);
		restore_into(&contents, &to, None).unwrap();
		assert_eq!(listing(&to), written);
		fs::remove_dir_all(from).unwrap();
		fs::remove_dir_all(to).unwrap();
	}

	#[test]
	fn a_damaged_backup_or_another_file_is_refused() {
		let from = used_service();
		let mut bytes = pack(&collect(&from, &[]).unwrap()).unwrap();
		// One byte changed in the payload.
		let at = bytes.len() - 10;
		bytes[at] ^= 1;
		assert!(unpack(&bytes).err().unwrap().contains("doesn't match the checksum"));
		assert!(unpack(b"not a backup").err().unwrap().contains("isn't a backup"));
		assert!(unpack(b"elamond-backup\n{\"format\":1").err().unwrap().contains("damaged"));
		fs::remove_dir_all(from).unwrap();
	}

	#[test]
	fn a_newer_format_or_an_encrypted_backup_is_refused() {
		for rest in ["{\"format\":2,\"encryption\":\"none\"}\n{}", "{\"format\":2,\"something\":\"else\"}\n\u{0}\u{1}"] {
			let e = unpack(format!("elamond-backup\n{rest}").as_bytes()).err().unwrap();
			assert!(e.contains("newer elamond") && e.contains("format 2") && e.contains("Update elamond"), "{e}");
		}
		// What elamond 0.6.1 made without --plain: encrypted with a passphrase.
		let sealed = b"elamond-backup\n{\"format\":1,\"encryption\":\"argon2id+xchacha20poly1305\",\"kdf\":{\"algorithm\":\"argon2id\",\"version\":19,\"m_kib\":65536,\"t\":3,\"p\":4,\"salt\":\"AAAAAAAAAAAAAAAAAAAAAA==\"},\"nonce\":\"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\"}\n\x8f\x01";
		let e = unpack(sealed).err().unwrap();
		assert!(e.contains("encrypted with a passphrase") && e.contains("Restore it with elamond 0.6.1"), "{e}");
	}

	#[test]
	fn nothing_is_written_when_the_backup_doesnt_check_out() {
		let from = used_service();
		let to = scratch();
		identity::Identity::load_or_create(&to).unwrap();
		let untouched = listing(&to);
		let good = collect(&from, &[]).unwrap();
		let with = |name: &str, data: Option<&[u8]>| {
			let mut c = collect(&from, &[]).unwrap();
			match data {
				Some(d) => c.state.insert(name.into(), d.to_vec()),
				None => c.state.remove(name),
			};
			c
		};
		let other_key = {
			let other = scratch();
			let key = identity::Identity::load_or_create(&other).unwrap().key_der;
			fs::remove_dir_all(other).unwrap();
			key
		};
		for (c, why) in [
			(with(identity::KEY_FILE, Some(&other_key)), "don't load together"),
			(with(identity::KEY_FILE, Some(b"garbage")), "don't load together"),
			(with(identity::CERT_FILE, None), "lacks the certificate"),
			(with(identity::ID_FILE, Some(b" \n")), "no service id"),
			(with(store::CLIENTS_FILE, Some(b"[{\"id\":")), "clients.json in the backup"),
		] {
			let e = verify(&c, false).err().unwrap();
			assert!(e.contains(why), "{why}: {e}");
		}
		assert!(verify(&good, false).is_ok());
		assert_eq!(listing(&to), untouched);

		// A write that fails (here a folder where activity.jsonl goes) is an error; nothing is undone.
		fs::create_dir(to.join(activity::FILE)).unwrap();
		fs::write(to.join(activity::FILE).join("x"), "x").unwrap();
		assert!(restore_into(&good, &to, None).is_err());
		fs::remove_dir_all(from).unwrap();
		fs::remove_dir_all(to).unwrap();
	}

	#[test]
	fn state_the_backup_doesnt_have_is_taken_away() {
		// A backup from before any phone or control, onto a computer with both.
		let from = scratch();
		identity::Identity::load_or_create(&from).unwrap();
		let mut c = collect(&from, &[]).unwrap();
		// 0.6.1 still carried 0.5's links.json and its signed rotations: nothing reads them, so
		// they aren't written or named.
		for name in RETIRED_FILES {
			c.state.insert(name.into(), b"[]".to_vec());
		}
		assert!(verify(&c, false).unwrap().unknown.is_empty());
		let to = used_service();
		restore_into(&c, &to, None).unwrap();
		assert!(RETIRED_FILES.iter().all(|name| !to.join(name).exists()));
		assert!(store::load_clients_in(&to).unwrap().is_empty());
		assert!(!to.join("clients.json").exists());
		// Control held off stays so: a restore never lets go of it.
		assert!(store::control_held_off(&to));
		fs::remove_dir_all(from).unwrap();
		fs::remove_dir_all(to).unwrap();
	}

	#[test]
	fn phones_unpaired_since_are_named() {
		let from = used_service();
		let summary = verify(&collect(&from, &[]).unwrap(), false).unwrap();
		// A fresh install has no phones to compare with: all come back.
		let to = scratch();
		identity::Identity::load_or_create(&to).unwrap();
		assert!(returning(&summary.phones, &to).is_empty());
		// The same computer with one of them unpaired since, or all of them: those come back.
		let mut now = summary.phones.clone();
		now.retain(|c| c.name != "Tablet");
		store::save_clients_in(&to, &now).unwrap();
		assert_eq!(returning(&summary.phones, &to).iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Tablet"]);
		store::save_clients_in(&to, &[]).unwrap();
		assert_eq!(returning(&summary.phones, &to).len(), 2);
		// The same id with another key is another phone.
		let mut rekeyed = summary.phones.clone();
		rekeyed[0].token_hash = store::sha256_hex(b"another");
		store::save_clients_in(&to, &rekeyed).unwrap();
		assert_eq!(returning(&summary.phones, &to).len(), 1);
		// Only a file is read, and not past the limit.
		assert!(read_backup(&to).unwrap_err().contains("isn't a file"));
		let fifo = to.join("fifo");
		let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
		// SAFETY: mkfifo with a valid C string.
		assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
		assert!(read_backup(&fifo).unwrap_err().contains("isn't a file"));
		let big = to.join("big");
		fs::File::create(&big).unwrap().set_len(FILE_MAX + 1).unwrap();
		assert!(read_backup(&big).unwrap_err().contains("larger than"));
		fs::write(&big, b"elamond-backup\n").unwrap();
		assert_eq!(read_backup(&big).unwrap(), b"elamond-backup\n");
		fs::remove_dir_all(from).unwrap();
		fs::remove_dir_all(to).unwrap();
	}

	#[test]
	fn settings_go_back_only_where_they_may() {
		let from = used_service();
		let cfg = scratch();
		fs::create_dir_all(cfg.join("elamond.d")).unwrap();
		fs::write(cfg.join("main.toml"), "port = 47900\n").unwrap();
		fs::write(cfg.join("elamond.d/on.toml"), "control = true\n").unwrap();
		let settings = [("elamond.toml".to_string(), cfg.join("main.toml")), ("elamond.d/on.toml".to_string(), cfg.join("elamond.d/on.toml"))];
		let c = collect(&from, &settings).unwrap();
		assert_eq!(c.settings.keys().collect::<Vec<_>>(), ["elamond.d/on.toml", "elamond.toml"]);

		// A config directory with a drop-in of its own, which stays.
		let to = scratch();
		let place = SettingsPlace { dir: to.join("cfg"), main: to.join("cfg/elamond.toml") };
		fs::create_dir_all(to.join("cfg/elamond.d")).unwrap();
		fs::write(to.join("cfg/elamond.d/mine.toml"), "name = \"Mine\"\n").unwrap();
		let r = restore_into(&c, &to.join("data"), Some(&place)).unwrap();
		assert_eq!(fs::read_to_string(to.join("cfg/elamond.toml")).unwrap(), "port = 47900\n");
		assert_eq!(fs::read_to_string(to.join("cfg/elamond.d/on.toml")).unwrap(), "control = true\n");
		assert_eq!(r.settings_kept, [to.join("cfg/elamond.d/mine.toml")]);
		assert!(r.settings_removed.is_empty());

		// A fresh install's starter, its dashboards.toml untouched, under a backup without one: the
		// standard dashboards go, as they would be read over the backup's. Changed, they stay.
		let fresh = SettingsPlace { dir: to.join("fresh"), main: to.join("fresh/elamond.toml") };
		crate::migrate::write_starter(&fresh.main).unwrap();
		let r = restore_into(&c, &to.join("data"), Some(&fresh)).unwrap();
		assert_eq!(r.settings_removed, [to.join("fresh/elamond.d/dashboards.toml"), to.join("fresh/elamond.d/events.toml")]);
		assert!(!to.join("fresh/elamond.d/dashboards.toml").exists() && !to.join("fresh/elamond.d/events.toml").exists());
		let changed = SettingsPlace { dir: to.join("changed"), main: to.join("changed/elamond.toml") };
		crate::migrate::write_starter(&changed.main).unwrap();
		fs::write(to.join("changed/elamond.d/dashboards.toml"), "[dashboards.mine]\ntitle = \"Mine\"\n").unwrap();
		let r = restore_into(&c, &to.join("data"), Some(&changed)).unwrap();
		// Its untouched events.toml goes all the same: the backup's settings had no rules.
		assert!(r.settings_removed == [to.join("changed/elamond.d/events.toml")] && r.settings_kept.contains(&to.join("changed/elamond.d/dashboards.toml")));
		assert_eq!(r.settings_written.len(), 2);

		// A main file NixOS or Home Manager make, or one that can't be written, is left alone.
		let store_like = to.join("store-elamond.toml");
		fs::write(&store_like, "port = 1\n").unwrap();
		let linked = SettingsPlace { dir: to.join("cfg2"), main: to.join("cfg2/elamond.toml") };
		fs::create_dir_all(&linked.dir).unwrap();
		std::os::unix::fs::symlink(&store_like, &linked.main).unwrap();
		let r = restore_into(&c, &to.join("data"), Some(&linked)).unwrap();
		assert!(r.settings_skipped[0].contains("a link"), "{:?}", r.settings_skipped);
		assert_eq!(fs::read_to_string(&store_like).unwrap(), "port = 1\n");
		fs::set_permissions(&store_like, fs::Permissions::from_mode(0o444)).unwrap();
		// SAFETY: geteuid only reads the process's user id. Root may write anywhere.
		if unsafe { libc::geteuid() } != 0 {
			let named = SettingsPlace { dir: to.join("cfg3"), main: store_like.clone() };
			let r = restore_into(&c, &to.join("data"), Some(&named)).unwrap();
			assert!(r.settings_skipped[0].contains("not writable"), "{:?}", r.settings_skipped);
			assert_eq!(fs::read_to_string(&store_like).unwrap(), "port = 1\n");
		}

		// Names that lead out of the settings folder, and settings that don't parse, stop the restore.
		for bad in ["../x.toml", "elamond.d/../../x.toml", "elamond.d/.hidden.toml", "other.d/x.toml", "elamond.d/x.txt", "x.toml", "elamond.d/.toml"] {
			let mut c = collect(&from, &[]).unwrap();
			c.settings.insert(bad.into(), String::new());
			assert!(verify(&c, true).err().unwrap().contains("outside the settings folder"), "{bad}");
			// Not looked at without --with-settings.
			assert!(verify(&c, false).is_ok());
		}
		let mut c = collect(&from, &[]).unwrap();
		c.settings.insert("elamond.d/bad.toml".into(), "port = \"x\"\n".into());
		assert!(verify(&c, true).err().unwrap().contains("elamond.d/bad.toml"));
		// A drop-in is read over the main file, as the service reads them (G38): one that changes a
		// key of a link the main file has is sound, though it isn't a whole link by itself.
		let mut c = collect(&from, &[]).unwrap();
		c.settings.insert("elamond.toml".into(), "schema = 3\n[links.media]\nname = \"Media\"\nurl = \"http://{host}:8096\"\n".into());
		c.settings.insert("elamond.d/port.toml".into(), "[links.media]\nurl = \"http://{host}:8097\"\n".into());
		assert!(verify(&c, true).is_ok(), "{:?}", verify(&c, true).err());
		// And one that only makes sense over another drop-in, read after it by name.
		c.settings.insert("elamond.d/a.toml".into(), "[links.more]\nname = \"More\"\nurl = \"http://{host}:1\"\n".into());
		c.settings.insert("elamond.d/b.toml".into(), "[links.more]\nurl = \"http://{host}:2\"\n".into());
		assert!(verify(&c, true).is_ok(), "{:?}", verify(&c, true).err());
		fs::set_permissions(&store_like, fs::Permissions::from_mode(0o644)).unwrap();
		for d in [from, cfg, to] {
			fs::remove_dir_all(d).unwrap();
		}
	}

	#[test]
	fn a_running_service_is_seen_by_its_lock() {
		let dir = scratch();
		assert!(!store::data_dir_held(&dir));
		let held = store::lock_data_dir(&dir).unwrap().expect("free");
		assert!(store::data_dir_held(&dir));
		assert!(store::lock_data_dir(&dir).unwrap().is_none());
		drop(held);
		assert!(!store::data_dir_held(&dir));
		assert!(store::lock_data_dir(&dir).unwrap().is_some());
		// Not carried in a backup.
		identity::Identity::load_or_create(&dir).unwrap();
		assert!(!collect(&dir, &[]).unwrap().state.contains_key("serve.lock"));
		// Nothing to back up without an identity.
		let empty = scratch();
		assert!(collect(&empty, &[]).err().unwrap().contains("no service identity"));
		fs::remove_dir_all(dir).unwrap();
		fs::remove_dir_all(empty).unwrap();
	}

	#[test]
	fn arguments() {
		let args = |s: &[&str]| s.iter().map(|a| a.to_string()).collect::<Vec<_>>();
		assert_eq!(
			Args::parse(true, &args(&["b.elamond", "--force", "--with-settings"])).unwrap(),
			Args { file: Some("b.elamond".into()), force: true, with_settings: true, with_old_phones: false }
		);
		// What 0.6.1 needed for an unencrypted backup changes nothing now.
		assert_eq!(Args::parse(false, &args(&["--plain", "b"])).unwrap(), Args { file: Some("b".into()), ..Default::default() });
		// Each command's own options only, and no passphrase.
		assert!(Args::parse(true, &args(&["b", "--plain"])).is_err());
		assert!(Args::parse(false, &args(&["b", "--force"])).is_err());
		assert!(Args::parse(false, &args(&["b", "--with-settings"])).is_err());
		assert!(Args::parse(false, &args(&["b", "--passphrase-file", "/p"])).err().unwrap().contains("unknown option"));
		assert!(Args::parse(true, &args(&["b", "--passphrase-file=/p"])).is_err());
		assert!(Args::parse(false, &args(&[])).is_err());
		assert!(Args::parse(false, &args(&["a", "b"])).is_err());
	}
}
