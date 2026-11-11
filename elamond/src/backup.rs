//! `elamond backup` and `elamond restore`: what makes this service the same computer to the phones
//! paired with it — its id, its certificate and key with the rotations behind them, the paired
//! phones, the links added from the app, the activity log and `elamond control off` — in one file,
//! and the settings files beside them, which go back only when asked.
//!
//! A backup is three parts: the line "elamond-backup", a line of JSON saying how the rest is kept
//! (`Header`), and the rest. Encrypted, as by default, the rest is the payload sealed with
//! XChaCha20-Poly1305 under a key Argon2id derives from the passphrase, the two lines before it
//! authenticated with it, so neither can be changed unnoticed. With `--plain` the rest is the
//! payload itself, its SHA-256 in the header. The payload is JSON (`Payload`).
//!
//! A restore checks everything before it writes anything, sets each file it replaces or takes away
//! beside it as NAME.before-restore-TIME, and puts them all back if a write fails halfway.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::mem::MaybeUninit;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, Ordering};

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload as Message};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::consts::backup as cost;
use crate::{activity, identity, links, listen, store};

const B64: data_encoding::Encoding = data_encoding::BASE64;

/// A backup's first line.
const MAGIC: &[u8] = b"elamond-backup\n";
/// The format written, and the newest read: one that changes what an older elamond would get
/// wrong raises it.
pub const FORMAT: u32 = 1;
/// How the payload is kept, as the header says.
const SEALED: &str = "argon2id+xchacha20poly1305";
const PLAIN: &str = "none";
const ARGON2ID: &str = "argon2id";

/// The state files a backup carries, in the order a restore writes them: control-off.json first, so
/// a computer whose control was held off is never open in between; the identity before the phones
/// that trust it; the key before the certificate, as a running service takes up a new cert.der with
/// the key beside it. The pending pairing code is never carried.
const STATE_FILES: [&str; 10] = [
	store::CONTROL_OFF_FILE,
	identity::ID_FILE,
	identity::ROTATIONS_FILE,
	identity::NEXT_FILE,
	identity::KEY_FILE,
	identity::CERT_FILE,
	store::CLIENTS_FILE,
	links::FILE,
	links::OLD_FILE,
	activity::FILE,
];

/// The second line of a backup: how the rest is kept.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
struct Header {
	format: u32,
	/// `SEALED` or `PLAIN`.
	encryption: String,
	/// How the key comes from the passphrase, when sealed.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	kdf: Option<Kdf>,
	/// XChaCha20-Poly1305's nonce, base64, when sealed.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	nonce: Option<String>,
	/// The payload's SHA-256, hex, when plain: a file damaged on the way is noticed.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	sha256: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
struct Kdf {
	/// `ARGON2ID`.
	algorithm: String,
	/// Argon2's version: 19, for 1.3.
	version: u32,
	/// Memory in KiB, passes, lanes.
	m_kib: u32,
	t: u32,
	p: u32,
	/// base64.
	salt: String,
}

/// What making the key costs: memory in KiB, passes, lanes.
#[derive(Clone, Copy, Debug)]
pub struct Cost {
	pub m_kib: u32,
	pub t: u32,
	pub p: u32,
}

impl Cost {
	pub const DEFAULT: Cost = Cost { m_kib: cost::ARGON2_M_KIB, t: cost::ARGON2_T, p: cost::ARGON2_P };
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
	pub state: BTreeMap<String, Zeroizing<Vec<u8>>>,
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
				state.insert(name.to_string(), Zeroizing::new(b));
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

/// How a backup is kept.
pub enum Protection<'a> {
	Plain,
	Passphrase(&'a str, Cost),
}

/// The backup of `c`, as written to its file.
pub fn seal(c: &Contents, how: Protection) -> Result<Vec<u8>, String> {
	let payload = Payload {
		service_version: c.service_version.clone(),
		created: c.created,
		host: c.host.clone(),
		state: c.state.iter().map(|(k, v)| (k.clone(), B64.encode(v))).collect(),
		settings: c.settings.clone(),
	};
	let plain = Zeroizing::new(serde_json::to_vec(&payload).map_err(|e| e.to_string())?);
	// The base64 copies of the state go as soon as they're in the JSON.
	drop(payload.state.into_values().map(Zeroizing::new).collect::<Vec<_>>());
	match how {
		Protection::Plain => {
			let header = Header { format: FORMAT, encryption: PLAIN.into(), kdf: None, nonce: None, sha256: Some(store::sha256_hex(&plain)) };
			let mut out = head(&header)?;
			out.extend_from_slice(&plain);
			Ok(out)
		}
		Protection::Passphrase(passphrase, cost) => {
			let salt = store::random_bytes::<{ cost::SALT_BYTES }>();
			let nonce = store::random_bytes::<{ cost::NONCE_BYTES }>();
			let kdf = Kdf {
				algorithm: ARGON2ID.into(),
				version: cost::ARGON2_VERSION,
				m_kib: cost.m_kib,
				t: cost.t,
				p: cost.p,
				salt: B64.encode(&salt),
			};
			let key = derive(passphrase, &kdf)?;
			let header = Header { format: FORMAT, encryption: SEALED.into(), kdf: Some(kdf), nonce: Some(B64.encode(&nonce)), sha256: None };
			let mut out = head(&header)?;
			let cipher = XChaCha20Poly1305::new_from_slice(&key[..]).map_err(|e| e.to_string())?;
			let sealed = cipher.encrypt(&XNonce::from(nonce), Message { msg: &plain, aad: &out }).map_err(|_| "can't encrypt the backup".to_string())?;
			out.extend_from_slice(&sealed);
			Ok(out)
		}
	}
}

/// The first two lines of a backup with `header`.
fn head(header: &Header) -> Result<Vec<u8>, String> {
	let mut out = MAGIC.to_vec();
	out.extend(serde_json::to_vec(header).map_err(|e| e.to_string())?);
	out.push(b'\n');
	Ok(out)
}

/// The key `kdf` makes of `passphrase`, refused when it asks for more than a restore will give.
fn derive(passphrase: &str, kdf: &Kdf) -> Result<Zeroizing<[u8; 32]>, String> {
	if kdf.algorithm != ARGON2ID || kdf.version != cost::ARGON2_VERSION {
		return Err(format!("this backup's key is made in a way this elamond doesn't know ({} {})", kdf.algorithm, kdf.version));
	}
	if kdf.m_kib > cost::ARGON2_M_KIB_MAX || kdf.t > cost::ARGON2_T_MAX || kdf.p > cost::ARGON2_P_MAX {
		return Err(format!(
			"this backup asks for more memory or time than a restore gives ({} KiB, {} passes, {} lanes); it wasn't made by elamond backup",
			kdf.m_kib, kdf.t, kdf.p
		));
	}
	let params = Params::new(kdf.m_kib, kdf.t, kdf.p, Some(32)).map_err(|e| format!("{DAMAGED} ({e})"))?;
	let salt = B64.decode(kdf.salt.as_bytes()).map_err(|_| DAMAGED.to_string())?;
	let mut key = Zeroizing::new([0u8; 32]);
	Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
		.hash_password_into(passphrase.as_bytes(), &salt, &mut key[..])
		.map_err(|e| format!("{DAMAGED} ({e})"))?;
	Ok(key)
}

const DAMAGED: &str = "the backup is damaged";
const WRONG: &str = "the passphrase is wrong, or the backup was changed or damaged";

/// A backup read as far as it goes without its passphrase.
pub struct Envelope<'a> {
	header: Header,
	/// The first two lines, which a sealed payload authenticates.
	head: &'a [u8],
	body: &'a [u8],
}

impl<'a> Envelope<'a> {
	/// Reads the first two lines of `bytes`. A newer format is refused before anything else in it is
	/// looked at.
	pub fn parse(bytes: &'a [u8]) -> Result<Self, String> {
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
		let (head, body) = bytes.split_at(MAGIC.len() + end + 1);
		Ok(Envelope { header, head, body })
	}

	/// Whether it needs a passphrase to open.
	pub fn encrypted(&self) -> bool {
		self.header.encryption != PLAIN
	}

	/// What it holds, opened with `passphrase` when it is encrypted.
	pub fn open(&self, passphrase: Option<&str>) -> Result<Contents, String> {
		let plain = match self.header.encryption.as_str() {
			PLAIN => {
				if self.header.sha256.as_deref() != Some(store::sha256_hex(self.body).as_str()) {
					return Err(format!("{DAMAGED}: it doesn't match the checksum it was written with"));
				}
				Zeroizing::new(self.body.to_vec())
			}
			SEALED => {
				let passphrase = passphrase.ok_or("this backup is encrypted: it needs its passphrase")?;
				let kdf = self.header.kdf.as_ref().ok_or(DAMAGED)?;
				let nonce: [u8; cost::NONCE_BYTES] = self
					.header
					.nonce
					.as_deref()
					.and_then(|n| B64.decode(n.as_bytes()).ok())
					.and_then(|n| n.try_into().ok())
					.ok_or(DAMAGED)?;
				let key = derive(passphrase, kdf)?;
				let cipher = XChaCha20Poly1305::new_from_slice(&key[..]).map_err(|e| e.to_string())?;
				Zeroizing::new(cipher.decrypt(&XNonce::from(nonce), Message { msg: self.body, aad: self.head }).map_err(|_| WRONG)?)
			}
			other => return Err(format!("this backup is kept in a way this elamond doesn't know (\"{other}\")")),
		};
		let payload: Payload = serde_json::from_slice(&plain).map_err(|e| format!("{DAMAGED} ({e})"))?;
		let mut state = BTreeMap::new();
		for (name, data) in &payload.state {
			let bytes = B64.decode(data.as_bytes()).map_err(|_| format!("{DAMAGED}: {name} in it doesn't read"))?;
			state.insert(name.clone(), Zeroizing::new(bytes));
		}
		drop(payload.state.into_values().map(Zeroizing::new).collect::<Vec<_>>());
		Ok(Contents { service_version: payload.service_version, created: payload.created, host: payload.host, state, settings: payload.settings })
	}
}

/// What a backup will put back, as checked.
#[derive(Debug)]
pub struct Summary {
	pub id: String,
	pub fingerprint: String,
	pub phones: Vec<store::Client>,
	/// Links added from the app.
	pub links: usize,
	/// Entries in the activity log.
	pub activity: usize,
	/// Certificate rotations kept.
	pub rotations: usize,
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
	let rotations = parsed::<Vec<identity::Rotation>>(c, identity::ROTATIONS_FILE)?.map_or(0, |r| r.len());
	if c.state.get(identity::NEXT_FILE).is_some_and(|b| !identity::next_reads(b)) {
		return Err(format!("{} in the backup doesn't read", identity::NEXT_FILE));
	}
	let phones = parsed::<Vec<store::Client>>(c, store::CLIENTS_FILE)?.unwrap_or_default();
	// links.json, or services.json while there is none, as the service reads them.
	let old_links = parsed::<Vec<links::LinkDef>>(c, links::OLD_FILE)?;
	let links = parsed::<Vec<links::LinkDef>>(c, links::FILE)?.or(old_links).map_or(0, |l| l.len());
	let activity = file(activity::FILE)
		.map(|b| String::from_utf8_lossy(b).lines().filter(|l| serde_json::from_str::<activity::Entry>(l).is_ok()).count())
		.unwrap_or(0);
	if with_settings {
		for (name, text) in &c.settings {
			if !settings_name(name) {
				return Err(format!("the backup names a settings file \"{name}\" outside the settings folder; it wasn't made by elamond backup"));
			}
			toml::from_str::<store::Config>(text).map_err(|e| format!("{name} in the backup: {e}"))?;
		}
	}
	Ok(Summary {
		id,
		fingerprint: store::sha256_hex(cert),
		phones,
		links,
		activity,
		rotations,
		control_off: c.state.contains_key(store::CONTROL_OFF_FILE),
		unknown: c.state.keys().filter(|k| !STATE_FILES.contains(&k.as_str())).cloned().collect(),
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
/// drop-in right in one of the drop-in folders.
fn settings_name(name: &str) -> bool {
	if name == store::CONFIG_FILE {
		return true;
	}
	let Some((folder, file)) = name.split_once('/') else { return false };
	[store::DROPIN_DIR, store::OLD_DROPIN_DIR].contains(&folder)
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
	/// Files replaced or taken away, each kept beside as NAME.before-restore-TIME.
	pub set_aside: Vec<PathBuf>,
	pub settings_written: Vec<PathBuf>,
	/// Settings files left as they were, with why.
	pub settings_skipped: Vec<String>,
	/// Drop-ins there already that the backup doesn't have, which still apply.
	pub settings_kept: Vec<PathBuf>,
}

/// Puts `c` back in the data directory `dir`, and its settings in `settings` when given. Files
/// that are already as in the backup are left alone; the others are set aside before they are
/// replaced, and state files the backup doesn't have are taken away beside them. A write that
/// fails puts back everything done before it. `stamp` names the files set aside.
pub fn restore_into(c: &Contents, dir: &Path, settings: Option<&SettingsPlace>, stamp: u64) -> Result<Restored, String> {
	let mut journal = Journal { stamp, done: Vec::new() };
	let mut restored = Restored::default();
	let written = (|| -> io::Result<()> {
		store::private_dir(dir)?;
		let mut later = None;
		for name in STATE_FILES {
			let path = dir.join(name);
			match c.state.get(name) {
				Some(data) => {
					journal.write(&path, data, true)?;
				}
				// Let go of control last, once everything else is in place.
				None if name == store::CONTROL_OFF_FILE => later = Some(path),
				None => journal.take_away(&path)?,
			}
		}
		if let Some(path) = later {
			journal.take_away(&path)?;
		}
		let Some(place) = settings else { return Ok(()) };
		for (name, text) in &c.settings {
			let target = place.target(name);
			if let Some(why) = managed_elsewhere(&target) {
				restored.settings_skipped.push(format!("{name}: {} is {why}; left as it is", target.display()));
				continue;
			}
			if journal.write(&target, text.as_bytes(), false)? {
				restored.settings_written.push(target);
			}
		}
		for folder in [store::OLD_DROPIN_DIR, store::DROPIN_DIR] {
			let Ok(entries) = fs::read_dir(place.dir.join(folder)) else { continue };
			let mut kept: Vec<PathBuf> = entries
				.flatten()
				.filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
				.filter(|e| !c.settings.contains_key(&format!("{folder}/{}", e.file_name().to_string_lossy())))
				.map(|e| e.path())
				.collect();
			kept.sort();
			restored.settings_kept.extend(kept);
		}
		Ok(())
	})();
	match written {
		Ok(()) => {
			restored.set_aside = journal.set_aside();
			Ok(restored)
		}
		Err(e) => {
			let left = journal.undo();
			if left.is_empty() {
				Err(format!("{e}\n  nothing was changed: what was written is undone"))
			} else {
				Err(format!("{e}\n  and undoing what was written failed too:\n    {}", left.join("\n    ")))
			}
		}
	}
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

/// The files a restore changed, to undo them if a later write fails.
struct Journal {
	stamp: u64,
	done: Vec<Done>,
}

enum Done {
	/// `path` written, what was there before kept as `before`.
	Wrote { path: PathBuf, before: Option<PathBuf> },
	/// `path` moved to `aside`.
	TookAway { path: PathBuf, aside: PathBuf },
}

impl Journal {
	/// Writes `data` to `path` unless it holds that already, the file there before set aside, and
	/// says whether it wrote. A state file's folder is made private; a settings file's is left as
	/// it is.
	fn write(&mut self, path: &Path, data: &[u8], state: bool) -> io::Result<bool> {
		if fs::read(path).is_ok_and(|now| now == data) {
			return Ok(false);
		}
		let before = self.keep_aside(path, true)?;
		self.done.push(Done::Wrote { path: path.to_path_buf(), before });
		if state {
			store::write_private(path, data)?;
		} else {
			if let Some(dir) = path.parent() {
				fs::create_dir_all(dir)?;
			}
			store::replace_private(path, data)?;
		}
		Ok(true)
	}

	/// Moves the file at `path`, if there is one, aside.
	fn take_away(&mut self, path: &Path) -> io::Result<()> {
		if let Some(aside) = self.keep_aside(path, false)? {
			self.done.push(Done::TookAway { path: path.to_path_buf(), aside });
		}
		Ok(())
	}

	/// NAME.before-restore-TIME beside `path`, holding what is there now: a second name for the
	/// same file when `copy` (or a copy, where links can't be made), the file itself moved there
	/// when not. Nothing when there is no file.
	fn keep_aside(&self, path: &Path, copy: bool) -> io::Result<Option<PathBuf>> {
		match fs::symlink_metadata(path) {
			Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
			Err(e) => return Err(e),
			Ok(_) => {}
		}
		let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
		for n in 1.. {
			let suffix = if n == 1 { String::new() } else { format!("-{n}") };
			let aside = path.with_file_name(format!("{name}.before-restore-{}{suffix}", self.stamp));
			if fs::symlink_metadata(&aside).is_ok() {
				continue;
			}
			if !copy {
				fs::rename(path, &aside)?;
			} else if fs::hard_link(path, &aside).is_err() {
				let data = fs::read(path)?;
				let mut f = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&aside)?;
				f.write_all(&data)?;
				f.sync_all()?;
			}
			return Ok(Some(aside));
		}
		unreachable!("some name is free")
	}

	/// Every file set aside.
	fn set_aside(&self) -> Vec<PathBuf> {
		self.done
			.iter()
			.filter_map(|d| match d {
				Done::Wrote { before, .. } => before.clone(),
				Done::TookAway { aside, .. } => Some(aside.clone()),
			})
			.collect()
	}

	/// Puts everything back as it was, the last change first; what couldn't be put back.
	fn undo(self) -> Vec<String> {
		let mut left = Vec::new();
		for d in self.done.into_iter().rev() {
			let (path, result) = match d {
				// A rename between two names of one file does nothing; the second name then goes.
				Done::Wrote { path, before: Some(before) } => {
					let r = fs::rename(&before, &path).and_then(|()| match fs::remove_file(&before) {
						Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
						_ => Ok(()),
					});
					(path, r)
				}
				Done::Wrote { path, before: None } => {
					let r = match fs::remove_file(&path) {
						Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
						_ => Ok(()),
					};
					(path, r)
				}
				Done::TookAway { path, aside } => {
					let r = fs::rename(&aside, &path);
					(path, r)
				}
			};
			if let Err(e) = result {
				left.push(format!("{}: {e}", path.display()));
			}
		}
		left
	}
}

// ---- the commands ------------------------------------------------------------------------------

#[derive(Debug, Default, PartialEq)]
struct Args {
	file: Option<PathBuf>,
	plain: bool,
	force: bool,
	with_settings: bool,
	passphrase_file: Option<PathBuf>,
}

const BACKUP_USAGE: &str = "usage: elamond backup FILE [--passphrase-file PATH | --plain]";
const RESTORE_USAGE: &str = "usage: elamond restore FILE [--passphrase-file PATH] [--with-settings] [--force]";

impl Args {
	fn parse(restore: bool, args: &[String]) -> Result<Args, String> {
		let usage = if restore { RESTORE_USAGE } else { BACKUP_USAGE };
		let mut a = Args::default();
		let mut it = args.iter();
		while let Some(arg) = it.next() {
			match arg.as_str() {
				"--plain" if !restore => a.plain = true,
				"--force" if restore => a.force = true,
				"--with-settings" if restore => a.with_settings = true,
				"--passphrase-file" => a.passphrase_file = Some(it.next().ok_or(usage)?.into()),
				s if s.starts_with("--passphrase-file=") => a.passphrase_file = Some(s["--passphrase-file=".len()..].into()),
				s if s.starts_with('-') => return Err(format!("unknown option \"{s}\"\n{usage}")),
				s if a.file.is_none() => a.file = Some(s.into()),
				_ => return Err(usage.into()),
			}
		}
		if a.file.is_none() {
			return Err(usage.into());
		}
		if a.plain && a.passphrase_file.is_some() {
			return Err(format!("--plain writes no passphrase; give one or the other\n{usage}"));
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
	let passphrase = if a.plain { None } else { Some(passphrase(a.passphrase_file.as_deref(), true)?) };
	let how = match &passphrase {
		Some(p) => Protection::Passphrase(p.as_str(), Cost::DEFAULT),
		None => Protection::Plain,
	};
	let bytes = seal(&contents, how)?;
	store::replace_private(&file, &bytes).map_err(|e| format!("can't write {}: {e}", file.display()))?;

	if a.plain {
		println!("Backed up to {}, NOT encrypted:", file.display());
	} else {
		println!("Backed up to {}, encrypted with the passphrase (Argon2id, XChaCha20-Poly1305):", file.display());
	}
	print_summary(&summary);
	let settings: Vec<&str> = contents.settings.keys().map(String::as_str).collect();
	if !settings.is_empty() {
		println!("  Settings:      {} (put back only by elamond restore --with-settings)", settings.join(", "));
	}
	println!();
	if a.plain {
		eprintln!("  ! This backup is NOT encrypted. Whoever reads it can pose as this computer to every phone");
		eprintln!("  ! paired with it. Keep it where only you can read it, never in cloud storage or a shared");
		eprintln!("  ! folder; without --plain it is encrypted with a passphrase.");
	} else {
		println!("  Keep the passphrase apart from the file: without it the backup can't be opened, and whoever");
		println!("  has both can pose as this computer to its phones.");
	}
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
	let size = fs::metadata(&file).map_err(|e| format!("can't read {}: {e}", file.display()))?.len();
	if size > cost::FILE_MAX {
		return Err(format!("{} is {size} bytes, far more than a backup; it isn't one", file.display()));
	}
	let bytes = fs::read(&file).map_err(|e| format!("can't read {}: {e}", file.display()))?;
	let envelope = Envelope::parse(&bytes)?;
	let passphrase = if envelope.encrypted() { Some(passphrase(a.passphrase_file.as_deref(), false)?) } else { None };
	let contents = envelope.open(passphrase.as_deref().map(String::as_str))?;
	drop(passphrase);
	let summary = verify(&contents, a.with_settings)?;

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
	let restored = restore_into(&contents, &dir, place.as_ref(), store::now())?;
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
		for path in &restored.settings_kept {
			println!("  Kept:          {}, which the backup doesn't have", path.display());
		}
	} else if !contents.settings.is_empty() {
		let names: Vec<&str> = contents.settings.keys().map(String::as_str).collect();
		println!("  Settings:      not restored: {} (--with-settings puts them back)", names.join(", "));
	}
	if !restored.set_aside.is_empty() {
		let names: Vec<String> = restored.set_aside.iter().map(|p| p.display().to_string()).collect();
		println!("  Set aside:     {}", names.join("\n                 "));
	}
	println!();
	println!("Phones paired with this computer connect as before, without pairing again.");
	if running.is_some() {
		println!();
		println!("The service was running. It presents the restored certificate within seconds and knows the");
		println!("restored phones at once, but keeps the id, links and activity log it started with until it");
		println!("restarts. Restart it now: systemctl --user restart elamond");
	} else {
		println!("Start the service: systemctl --user start elamond");
	}
	Ok(())
}

fn print_summary(s: &Summary) {
	println!("  Computer id:   {}", s.id);
	println!("  Certificate:   {}", store::pretty_fingerprint(&s.fingerprint));
	let phones: Vec<String> = s.phones.iter().map(|c| format!("{} ({}, {})", c.name, c.id, c.role.as_str())).collect();
	if phones.is_empty() {
		println!("  Phones:        none paired");
	} else {
		println!("  Phones:        {}", phones.join("\n                 "));
	}
	println!("  Links:         {} added from the app", s.links);
	println!("  Activity log:  {} entries", s.activity);
	if s.rotations > 0 {
		println!("  Rotations:     {} kept, for phones that missed one", s.rotations);
	}
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

// ---- the passphrase ----------------------------------------------------------------------------

/// The passphrase from `file`, or asked on the terminal, `twice` for a new backup.
fn passphrase(file: Option<&Path>, twice: bool) -> Result<Zeroizing<String>, String> {
	let p = match file {
		Some(f) => from_file(f)?,
		None => from_terminal(twice)?,
	};
	if twice && p.chars().count() < cost::PASSPHRASE_MIN {
		return Err(format!("the passphrase needs at least {} characters; a few words are best", cost::PASSPHRASE_MIN));
	}
	Ok(p)
}

/// The passphrase in `path`, without the line end after it.
fn from_file(path: &Path) -> Result<Zeroizing<String>, String> {
	let meta = fs::metadata(path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
	if meta.is_file() && meta.permissions().mode() & 0o077 != 0 {
		eprintln!("  ! {} can be read by others; chmod 600 it", path.display());
	}
	let mut text = Zeroizing::new(fs::read_to_string(path).map_err(|e| format!("can't read {}: {e}", path.display()))?);
	let end = text.trim_end_matches(['\n', '\r']).len();
	text.truncate(end);
	if text.is_empty() {
		return Err(format!("{} holds no passphrase", path.display()));
	}
	Ok(text)
}

const NO_TERMINAL: &str = "there is no terminal to ask the passphrase on: give it with --passphrase-file PATH";

fn from_terminal(twice: bool) -> Result<Zeroizing<String>, String> {
	let tty = fs::OpenOptions::new().read(true).write(true).open("/dev/tty").map_err(|_| NO_TERMINAL.to_string())?;
	if !twice {
		return ask(&tty, "Passphrase: ");
	}
	for _ in 0..cost::PASSPHRASE_TRIES {
		let first = ask(&tty, "Passphrase for the backup: ")?;
		if first.chars().count() < cost::PASSPHRASE_MIN {
			let _ = writeln!(&tty, "  At least {} characters, please; a few words are best.", cost::PASSPHRASE_MIN);
			continue;
		}
		if *ask(&tty, "The same again: ")? == *first {
			return Ok(first);
		}
		let _ = writeln!(&tty, "  They differ; once more.");
	}
	Err("no passphrase was given twice alike; nothing was written".into())
}

/// A line typed on the terminal `tty` without showing it, after `prompt`.
fn ask(tty: &fs::File, prompt: &str) -> Result<Zeroizing<String>, String> {
	let mut out = tty;
	out.write_all(prompt.as_bytes()).and_then(|()| out.flush()).map_err(|e| e.to_string())?;
	let quiet = Quiet::new(tty.as_raw_fd()).map_err(|e| format!("can't hide what is typed: {e}"))?;
	let mut line = Zeroizing::new(String::new());
	let read = BufReader::new(tty).read_line(&mut line);
	drop(quiet);
	match read {
		Ok(0) => return Err("no passphrase was given".into()),
		Ok(_) => {}
		Err(e) => return Err(format!("can't read the passphrase: {e}")),
	}
	let end = line.trim_end_matches(['\n', '\r']).len();
	line.truncate(end);
	Ok(line)
}

/// The terminal's settings from before echo was turned off, for a signal to put back.
static QUIET_FD: AtomicI32 = AtomicI32::new(-1);
static mut SAVED: MaybeUninit<libc::termios> = MaybeUninit::uninit();

extern "C" fn on_stop_at_prompt(sig: libc::c_int) {
	let fd = QUIET_FD.load(Ordering::SeqCst);
	if fd >= 0 {
		// SAFETY: SAVED is written before QUIET_FD is set; tcsetattr is async-signal-safe.
		unsafe { libc::tcsetattr(fd, libc::TCSANOW, (&raw const SAVED).cast()) };
	}
	// SAFETY: _exit is async-signal-safe.
	unsafe { libc::_exit(128 + sig) };
}

/// Echo off on a terminal while it lives; Ctrl+C meanwhile turns it back on before elamond exits.
struct Quiet {
	fd: libc::c_int,
	saved: libc::termios,
	handlers: Vec<(libc::c_int, libc::sighandler_t)>,
}

impl Quiet {
	fn new(fd: libc::c_int) -> io::Result<Quiet> {
		let mut t = MaybeUninit::<libc::termios>::uninit();
		// SAFETY: tcgetattr fills the termios it is given, or fails and it is left unread.
		if unsafe { libc::tcgetattr(fd, t.as_mut_ptr()) } != 0 {
			return Err(io::Error::last_os_error());
		}
		// SAFETY: filled by tcgetattr above.
		let saved = unsafe { t.assume_init() };
		// SAFETY: nothing reads SAVED until QUIET_FD is set, below.
		unsafe { (&raw mut SAVED).write(MaybeUninit::new(saved)) };
		QUIET_FD.store(fd, Ordering::SeqCst);
		let handlers = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP]
			.into_iter()
			// SAFETY: the handler only calls async-signal-safe functions.
			.map(|sig| (sig, unsafe { libc::signal(sig, on_stop_at_prompt as *const () as libc::sighandler_t) }))
			.collect();
		let mut quiet = saved;
		// Nothing typed is shown, but the Enter that ends it is.
		quiet.c_lflag &= !libc::ECHO;
		quiet.c_lflag |= libc::ECHONL;
		// SAFETY: sets the terminal from a termios that tcgetattr filled.
		if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &quiet) } != 0 {
			let e = io::Error::last_os_error();
			drop(Quiet { fd, saved, handlers });
			return Err(e);
		}
		Ok(Quiet { fd, saved, handlers })
	}
}

impl Drop for Quiet {
	fn drop(&mut self) {
		// SAFETY: puts back the settings tcgetattr gave, and the handlers there were before.
		unsafe {
			libc::tcsetattr(self.fd, libc::TCSANOW, &self.saved);
			QUIET_FD.store(-1, Ordering::SeqCst);
			for (sig, before) in &self.handlers {
				libc::signal(*sig, *before);
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::access::Role;

	/// Cheap enough for tests; the format records the cost, so they open as any other.
	const QUICK: Cost = Cost { m_kib: 64, t: 1, p: 1 };
	const PASS: &str = "correct horse battery staple";

	fn scratch() -> PathBuf {
		let dir = std::env::temp_dir().join(format!("elamond-backup-{}", data_encoding::HEXLOWER.encode(&store::random_bytes::<6>())));
		fs::create_dir_all(&dir).unwrap();
		dir
	}

	fn client(name: &str, token: &str, role: Role) -> store::Client {
		store::Client { id: store::sha256_hex(name.as_bytes())[..8].into(), name: name.into(), token_hash: store::sha256_hex(token.as_bytes()), created: 1, role, from: None }
	}

	/// A service that has been used: rotated once, two phones, a link, a log, control held off.
	fn used_service() -> PathBuf {
		let dir = scratch();
		identity::Identity::load_or_create(&dir).unwrap();
		identity::rotate(&dir).unwrap();
		store::save_clients_in(&dir, &[client("Pixel", "token-a", Role::Admin), client("Tablet", "token-b", Role::Glance)]).unwrap();
		let link = links::LinkDef { id: "abc".into(), name: "Jellyfin".into(), url: "http://{host}:8096".into(), unit: None, scope: None };
		links::save_in(&dir, &[link]).unwrap();
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
	fn an_encrypted_backup_brings_the_same_computer_back_on_a_fresh_install() {
		let from = used_service();
		let before = identity::Identity::load(&from).unwrap();
		let bytes = seal(&collect(&from, &[]).unwrap(), Protection::Passphrase(PASS, QUICK)).unwrap();
		// Nothing of the state shows through.
		let token_hash = store::load_clients_in(&from).unwrap()[0].token_hash.clone();
		assert!(!bytes.windows(token_hash.len()).any(|w| w == token_hash.as_bytes()));
		assert!(!bytes.windows(before.key_der.len()).any(|w| w == before.key_der.as_slice()));
		assert!(bytes.starts_with(b"elamond-backup\n{\"format\":1,\"encryption\":\"argon2id+xchacha20poly1305\""));

		// A fresh install: the service made an identity of its own.
		let to = scratch();
		let fresh = identity::Identity::load_or_create(&to).unwrap();
		let envelope = Envelope::parse(&bytes).unwrap();
		assert!(envelope.encrypted());
		let contents = envelope.open(Some(PASS)).unwrap();
		let summary = verify(&contents, false).unwrap();
		assert_eq!((summary.id.as_str(), summary.fingerprint.as_str()), (before.id.as_str(), before.fingerprint.as_str()));
		assert_eq!((summary.phones.len(), summary.links, summary.activity, summary.rotations, summary.control_off), (2, 1, 3, 1, true));
		let restored = restore_into(&contents, &to, None, 1234).unwrap();

		// The same computer: id, certificate, key, the chain behind it and every phone's token.
		let after = identity::Identity::load_or_create(&to).unwrap();
		assert_eq!((&after.id, &after.fingerprint, &after.key_der), (&before.id, &before.fingerprint, &before.key_der));
		for name in STATE_FILES.iter().filter(|n| from.join(n).exists()) {
			assert_eq!(fs::read(to.join(name)).unwrap(), fs::read(from.join(name)).unwrap(), "{name}");
			assert_eq!(fs::metadata(to.join(name)).unwrap().permissions().mode() & 0o777, 0o600, "{name}");
		}
		assert!(store::load_clients_in(&to).unwrap().iter().any(|c| c.token_hash == token_hash));
		assert!(store::control_held_off(&to));
		// What the fresh install had is kept beside, not lost.
		assert_eq!(fs::read(to.join("cert.der.before-restore-1234")).unwrap(), fresh.cert_der);
		assert_eq!(fs::read(to.join("key.der.before-restore-1234")).unwrap(), fresh.key_der);
		assert_eq!(restored.set_aside.len(), 3, "{:?}", restored.set_aside);
		// No temporary files.
		assert!(!listing(&to).keys().any(|k| k.ends_with(".tmp")), "{:?}", listing(&to).keys());

		// Restored again: nothing more to do, nothing more set aside.
		assert!(restore_into(&contents, &to, None, 1235).unwrap().set_aside.is_empty());
		fs::remove_dir_all(from).unwrap();
		fs::remove_dir_all(to).unwrap();
	}

	#[test]
	fn a_plain_backup_reads_without_a_passphrase_and_a_damaged_one_is_refused() {
		let from = used_service();
		let mut bytes = seal(&collect(&from, &[]).unwrap(), Protection::Plain).unwrap();
		let envelope = Envelope::parse(&bytes).unwrap();
		assert!(!envelope.encrypted());
		let c = envelope.open(None).unwrap();
		assert_eq!(*c.state[store::CLIENTS_FILE], fs::read(from.join(store::CLIENTS_FILE)).unwrap());
		assert_eq!(c.service_version, env!("CARGO_PKG_VERSION"));
		// One byte changed in the payload.
		let at = bytes.len() - 10;
		bytes[at] ^= 1;
		let e = Envelope::parse(&bytes).unwrap().open(None).err().unwrap();
		assert!(e.contains("damaged"), "{e}");
		fs::remove_dir_all(from).unwrap();
	}

	#[test]
	fn a_wrong_passphrase_or_a_changed_byte_opens_nothing() {
		let from = used_service();
		let bytes = seal(&collect(&from, &[]).unwrap(), Protection::Passphrase(PASS, QUICK)).unwrap();
		let wrong = |b: &[u8], pass: &str| Envelope::parse(b).and_then(|e| e.open(Some(pass))).err().unwrap();
		assert_eq!(wrong(&bytes, "correct horse battery stapler"), WRONG);
		assert!(Envelope::parse(&bytes).unwrap().open(None).err().unwrap().contains("needs its passphrase"));
		// Any byte of the payload, or of the header it authenticates.
		let header_end = MAGIC.len() + bytes[MAGIC.len()..].iter().position(|b| *b == b'\n').unwrap();
		for at in [bytes.len() - 1, header_end + 1, header_end + 40] {
			let mut b = bytes.clone();
			b[at] ^= 0x20;
			assert_eq!(wrong(&b, PASS), WRONG, "byte {at}");
		}
		// The header: what makes the key, and what doesn't but is authenticated all the same.
		let text = String::from_utf8_lossy(&bytes[..header_end]).into_owned();
		for changed in [text.replace("\"m_kib\":64", "\"m_kib\":72"), text.replacen("{\"format\":1", "{\"note\":1,\"format\":1", 1)] {
			assert_ne!(changed.as_bytes(), &bytes[..header_end]);
			assert_eq!(wrong(&[changed.as_bytes(), &bytes[header_end..]].concat(), PASS), WRONG, "{changed}");
		}
		assert_eq!(wrong(&bytes[..bytes.len() - 1], PASS), WRONG);
		assert!(wrong(b"not a backup", PASS).contains("isn't a backup"));
		assert!(wrong(b"elamond-backup\n{\"format\":1", PASS).contains("damaged"));
		// A file that asks for more than a restore gives is refused before trying.
		let greedy = text.replace("\"m_kib\":64", "\"m_kib\":4194304").into_bytes();
		assert!(wrong(&[greedy.as_slice(), &bytes[header_end..]].concat(), PASS).contains("more memory or time"));
		fs::remove_dir_all(from).unwrap();
	}

	#[test]
	fn a_newer_format_is_refused_whatever_follows() {
		for rest in ["{\"format\":2,\"encryption\":\"none\"}\n{}", "{\"format\":2,\"something\":\"else\"}\n\u{0}\u{1}"] {
			let e = Envelope::parse(format!("elamond-backup\n{rest}").as_bytes()).err().unwrap();
			assert!(e.contains("newer elamond") && e.contains("format 2") && e.contains("Update elamond"), "{e}");
		}
		// The same format with a way of keeping it this elamond doesn't know.
		let bytes = b"elamond-backup\n{\"format\":1,\"encryption\":\"rot13\"}\n";
		assert!(Envelope::parse(bytes).unwrap().open(None).err().unwrap().contains("rot13"));
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
				Some(d) => c.state.insert(name.into(), Zeroizing::new(d.to_vec())),
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
			(with(links::FILE, Some(b"{}")), "links.json in the backup"),
			(with(identity::ROTATIONS_FILE, Some(b"[1]")), "rotations.json in the backup"),
			(with(identity::NEXT_FILE, Some(b"{\"cert\":\"!\",\"key\":\"\"}")), "next-identity.json in the backup"),
		] {
			let e = verify(&c, false).err().unwrap();
			assert!(e.contains(why), "{why}: {e}");
		}
		assert!(verify(&good, false).is_ok());
		assert_eq!(listing(&to), untouched);

		// A write that fails halfway (here a folder where links.json goes) puts back all before it.
		fs::create_dir(to.join(links::FILE)).unwrap();
		fs::write(to.join(links::FILE).join("x"), "x").unwrap();
		let untouched = listing(&to);
		let e = restore_into(&good, &to, None, 99).err().unwrap();
		assert!(e.contains("nothing was changed"), "{e}");
		assert_eq!(listing(&to), untouched);
		fs::remove_dir_all(from).unwrap();
		fs::remove_dir_all(to).unwrap();
	}

	#[test]
	fn state_the_backup_doesnt_have_is_taken_away_beside() {
		// A backup from before any phone, control or rotation, onto a computer with all of them.
		let from = scratch();
		identity::Identity::load_or_create(&from).unwrap();
		let c = collect(&from, &[]).unwrap();
		let to = used_service();
		let r = restore_into(&c, &to, None, 7).unwrap();
		assert!(!store::control_held_off(&to));
		assert!(store::load_clients_in(&to).unwrap().is_empty());
		assert!(identity::rotations(&to).unwrap().is_empty());
		assert!(to.join("control-off.json.before-restore-7").exists() && to.join("clients.json.before-restore-7").exists());
		// Control is let go of last.
		assert!(r.set_aside.last().unwrap().ends_with("control-off.json.before-restore-7"), "{:?}", r.set_aside);
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
		let r = restore_into(&c, &to.join("data"), Some(&place), 5).unwrap();
		assert_eq!(fs::read_to_string(to.join("cfg/elamond.toml")).unwrap(), "port = 47900\n");
		assert_eq!(fs::read_to_string(to.join("cfg/elamond.d/on.toml")).unwrap(), "control = true\n");
		assert_eq!(r.settings_kept, [to.join("cfg/elamond.d/mine.toml")]);
		assert_eq!(r.settings_written.len(), 2);

		// A main file NixOS or Home Manager make, or one that can't be written, is left alone.
		let store_like = to.join("store-elamond.toml");
		fs::write(&store_like, "port = 1\n").unwrap();
		let linked = SettingsPlace { dir: to.join("cfg2"), main: to.join("cfg2/elamond.toml") };
		fs::create_dir_all(&linked.dir).unwrap();
		std::os::unix::fs::symlink(&store_like, &linked.main).unwrap();
		let r = restore_into(&c, &to.join("data"), Some(&linked), 6).unwrap();
		assert!(r.settings_skipped[0].contains("a link"), "{:?}", r.settings_skipped);
		assert_eq!(fs::read_to_string(&store_like).unwrap(), "port = 1\n");
		fs::set_permissions(&store_like, fs::Permissions::from_mode(0o444)).unwrap();
		// SAFETY: geteuid only reads the process's user id. Root may write anywhere.
		if unsafe { libc::geteuid() } != 0 {
			let named = SettingsPlace { dir: to.join("cfg3"), main: store_like.clone() };
			let r = restore_into(&c, &to.join("data"), Some(&named), 6).unwrap();
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
			Args::parse(true, &args(&["b.elamond", "--force", "--passphrase-file=/p", "--with-settings"])).unwrap(),
			Args { file: Some("b.elamond".into()), force: true, with_settings: true, passphrase_file: Some("/p".into()), plain: false }
		);
		assert_eq!(Args::parse(false, &args(&["--plain", "b"])).unwrap(), Args { file: Some("b".into()), plain: true, ..Default::default() });
		assert_eq!(Args::parse(false, &args(&["b", "--passphrase-file", "/p"])).unwrap().passphrase_file, Some("/p".into()));
		// Each command's own options only.
		assert!(Args::parse(true, &args(&["b", "--plain"])).is_err());
		assert!(Args::parse(false, &args(&["b", "--force"])).is_err());
		assert!(Args::parse(false, &args(&["b", "--with-settings"])).is_err());
		assert!(Args::parse(false, &args(&["b", "--plain", "--passphrase-file", "/p"])).is_err());
		assert!(Args::parse(false, &args(&[])).is_err());
		assert!(Args::parse(false, &args(&["a", "b"])).is_err());
		assert!(Args::parse(true, &args(&["b", "--passphrase-file"])).is_err());
	}

	#[test]
	fn a_passphrase_file_gives_its_first_line_end_and_all() {
		let dir = scratch();
		let path = dir.join("pass");
		fs::write(&path, "two words\r\n").unwrap();
		fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
		assert_eq!(*passphrase(Some(&path), true).unwrap(), "two words");
		fs::write(&path, "short\n").unwrap();
		assert!(passphrase(Some(&path), true).err().unwrap().contains("at least 8"));
		// Opening needs no length: the backup decides.
		assert_eq!(*passphrase(Some(&path), false).unwrap(), "short");
		fs::write(&path, "\n").unwrap();
		assert!(passphrase(Some(&path), false).err().unwrap().contains("no passphrase"));
		fs::remove_dir_all(dir).unwrap();
	}
}
