//! Files elamond keeps: settings, paired phones and the pending pairing code; its TLS identity
//! is in `identity`.
//!
//! Settings:  $XDG_CONFIG_HOME/elamon/elamond.toml, then elamond.d/*.toml beside it (phones.toml
//!            among them); defaults.toml beside them is for reading, not read
//! State:     $XDG_DATA_HOME/elamon/{id, cert.der, key.der, clients.json, pairing.json,
//!            control-off.json, activity.jsonl, rotations.json}

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::net::IpAddr;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::access::Role;
use crate::consts::{limit, protocol, timeout};

/// The settings: the files (see `settings`).
pub use crate::settings::Config;

pub fn config_dir() -> PathBuf {
	xdg_dir("ELAMON_CONFIG_DIR", "XDG_CONFIG_HOME", ".config")
}

pub fn data_dir() -> PathBuf {
	xdg_dir("ELAMON_DATA_DIR", "XDG_DATA_HOME", ".local/share")
}

fn xdg_dir(own: &str, xdg: &str, fallback: &str) -> PathBuf {
	if let Some(d) = std::env::var_os(own) {
		return PathBuf::from(d);
	}
	let base = std::env::var_os(xdg)
		.map(PathBuf::from)
		.filter(|p| p.is_absolute())
		.unwrap_or_else(|| home().join(fallback));
	base.join("elamon")
}

fn home() -> PathBuf {
	std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

/// The settings file in the config directory, and the folder of drop-ins beside it.
pub const CONFIG_FILE: &str = "elamond.toml";
pub const DROPIN_DIR: &str = "elamond.d";

/// The settings files in the order they are read: the main file, then the drop-ins by name.
pub fn config_files() -> Vec<PathBuf> {
	Place::here().files()
}

/// Where the settings are: the config directory, with the drop-ins in elamond.d/ (phones.toml among
/// them), and the main file, elamond.toml there unless ELAMON_CONFIG names another.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
	pub dir: PathBuf,
	/// The main file ELAMON_CONFIG names, when it does.
	pub named: Option<PathBuf>,
}

impl Place {
	/// Where this elamond's settings are, as its environment says.
	pub fn here() -> Place {
		Place { dir: config_dir(), named: std::env::var_os("ELAMON_CONFIG").map(PathBuf::from) }
	}

	/// The main file: the one ELAMON_CONFIG names, or elamond.toml in the config directory.
	pub fn main(&self) -> PathBuf {
		self.named.clone().unwrap_or_else(|| self.dir.join(CONFIG_FILE))
	}

	/// The folder of drop-ins.
	pub fn dropins(&self) -> PathBuf {
		self.dir.join(DROPIN_DIR)
	}

	/// The settings files in the order they are read: the main file, then the drop-ins by name.
	pub fn files(&self) -> Vec<PathBuf> {
		settings_files(&self.dir, self.named.clone())
	}

	/// The settings as the service reads them; every mistake, one to a line, each with its file and
	/// line.
	pub fn load(&self) -> Result<Config, String> {
		let mut config = crate::settings::load(&self.files()).map_err(|m| crate::settings::join_mistakes(&m))?;
		// Where a profile is, as a person finds it: within the settings' folder, from there.
		for w in config.profile_sources.values_mut() {
			if let Ok(within) = Path::new(&w.file).strip_prefix(&self.dir) {
				w.file = within.display().to_string();
			}
		}
		Ok(config)
	}

	/// What the settings files look like on disk now, to notice an edit. The resolved path is
	/// part of it: a store file keeps the same mtime, its symlink gets a new target.
	pub fn stamp(&self) -> Vec<(PathBuf, Option<PathBuf>, Option<SystemTime>, u64)> {
		self.files()
			.into_iter()
			.map(|p| {
				let target = fs::canonicalize(&p).ok();
				let meta = fs::metadata(&p).ok();
				let modified = meta.as_ref().and_then(|m| m.modified().ok());
				let len = meta.map(|m| m.len()).unwrap_or(0);
				(p, target, modified, len)
			})
			.collect()
	}
}

/// The main file is the one ELAMON_CONFIG names (the NixOS module points it into the store), or
/// elamond.toml in the config directory `dir`. Drop-ins are always in the user's own config
/// directory: where NixOS or Home Manager make the main file a read-only one in the store, these
/// stay the user's to write.
pub fn settings_files(dir: &Path, named: Option<PathBuf>) -> Vec<PathBuf> {
	let main = named.unwrap_or_else(|| dir.join(CONFIG_FILE));
	let mut dropins: Vec<PathBuf> = fs::read_dir(dir.join(DROPIN_DIR))
		.map(|d| d.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect())
		.unwrap_or_default();
	dropins.sort();
	std::iter::once(main).chain(dropins).collect()
}

/// Settings from elamond.toml and the drop-ins; every mistake, one to a line, each with its file
/// and line.
pub fn load_config() -> Result<Config, String> {
	Place::here().load()
}

/// Whether `path` is a link into the Nix store: a file NixOS or Home Manager made, not to be written.
pub fn from_nix_store(path: &Path) -> bool {
	fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) && fs::canonicalize(path).is_ok_and(|p| p.starts_with("/nix/store"))
}

/// A path with the home folder written `~`, for people to read.
pub fn shown(path: &Path) -> String {
	let home = home();
	match path.strip_prefix(&home) {
		Ok(rest) if home != Path::new("/") => format!("~/{}", rest.display()),
		_ => path.display().to_string(),
	}
}

pub fn now() -> u64 {
	SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
	let mut b = [0u8; N];
	rand::fill(&mut b);
	b
}

pub fn sha256_hex(data: &[u8]) -> String {
	data_encoding::HEXLOWER.encode(&Sha256::digest(data))
}

/// YYYY-MM-DD from Unix time (UTC), without pulling in a date library.
pub fn format_date(secs: u64) -> String {
	let days = (secs / 86_400) as i64;
	// Howard Hinnant's civil_from_days.
	let z = days + 719_468;
	let era = z.div_euclid(146_097);
	let doe = z.rem_euclid(146_097);
	let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
	let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
	let mp = (5 * doy + 2) / 153;
	let d = doy - (153 * mp + 2) / 5 + 1;
	let m = if mp < 10 { mp + 3 } else { mp - 9 };
	let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
	format!("{y:04}-{m:02}-{d:02}")
}

/// The permissions a file written over `path` gets: its own, or `new` for a file not there yet.
pub fn mode_of(path: &Path, new: u32) -> u32 {
	fs::metadata(path).map(|m| m.permissions().mode() & 0o777).unwrap_or(new)
}

/// Writes `text` to `path` in one step, the file having `mode`: through a new file beside it of a
/// name of its own, made with that mode, renamed over `path`.
pub fn replace_as(path: &Path, text: &[u8], mode: u32) -> io::Result<()> {
	let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
	let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
	let tag = data_encoding::HEXLOWER.encode(&random_bytes::<4>());
	let tmp = dir.join(format!(".{name}.{}.{tag}.saving", std::process::id()));
	let written = (|| {
		let mut f = fs::OpenOptions::new().write(true).create_new(true).mode(mode).open(&tmp)?;
		f.write_all(text)?;
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

/// Writes a file only its owner can read, replacing it atomically, in a directory only its owner
/// can enter. The temporary file's name is this writer's own, so the service and a command run
/// beside it can't trip over each other.
pub fn write_private(path: &Path, data: &[u8]) -> io::Result<()> {
	private_dir(path.parent().expect("state files live in a directory"))?;
	replace_private(path, data)
}

/// Makes `dir` if it isn't there, and lets only its owner in.
pub fn private_dir(dir: &Path) -> io::Result<()> {
	fs::create_dir_all(dir)?;
	fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
}

/// Writes a file only its owner can read, replacing it atomically, and leaves the directory it is
/// in as it was: for a file put where someone chose, like a backup.
pub fn replace_private(path: &Path, data: &[u8]) -> io::Result<()> {
	let dir = match path.parent() {
		Some(d) if !d.as_os_str().is_empty() => d,
		_ => Path::new("."),
	};
	let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
	let tag = data_encoding::HEXLOWER.encode(&random_bytes::<4>());
	let tmp = dir.join(format!(".{name}.{}.{tag}.tmp", std::process::id()));
	let written = (|| {
		let mut f = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
		f.write_all(data)?;
		f.sync_all()?;
		fs::rename(&tmp, path)
	})();
	if written.is_err() {
		let _ = fs::remove_file(&tmp);
	}
	written
}

/// A JSON state file: `None` when it doesn't exist. One that doesn't parse is an
/// `InvalidData` error naming the file.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> io::Result<Option<T>> {
	let named = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
	let bytes = match fs::read(path) {
		Ok(b) => b,
		Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
		Err(e) => return Err(io::Error::new(e.kind(), named(&e))),
	};
	serde_json::from_slice(&bytes).map(Some).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, named(&e)))
}

/// Replaces a JSON state file. One that no longer parses is not overwritten but kept beside
/// it as `<name>.corrupt-<unix time>`, so nothing in it is lost without a trace.
pub fn write_json<T: DeserializeOwned>(path: &Path, value: &(impl Serialize + ?Sized)) -> io::Result<()> {
	match read_json::<T>(path) {
		Err(e) if e.kind() == io::ErrorKind::InvalidData => {
			let kept = path.with_file_name(format!(
				"{}.corrupt-{}",
				path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
				now()
			));
			fs::rename(path, &kept)?;
			eprintln!("{e}\n  the damaged file is kept as {} and a new one written", kept.display());
		}
		Err(e) => return Err(e),
		Ok(_) => {}
	}
	write_private(path, &serde_json::to_vec_pretty(value).map_err(io::Error::other)?)
}

/// The fingerprint in groups of four, for people to compare by eye.
pub fn pretty_fingerprint(fp: &str) -> String {
	fp.as_bytes()[..32.min(fp.len())]
		.chunks(4)
		.map(|c| std::str::from_utf8(c).unwrap_or("").to_ascii_uppercase())
		.collect::<Vec<_>>()
		.join(" ")
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Client {
	pub id: String,
	pub name: String,
	/// SHA-256 of the client's token; the token itself is only ever on the phone.
	pub token_hash: String,
	pub created: u64,
	/// What the phone may do. A phone with no role in the file is an admin.
	#[serde(default)]
	pub role: Role,
	/// The entry whose token asked for this one (`POST /api/v1/rekey`): a phone that came by
	/// another's token and got a key of its own. None for a phone paired with a code.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub from: Option<String>,
	/// Its profile; none for `default_profile` (and one the settings no longer have is that too).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub profile: Option<String>,
	/// The app it said it is last (`Elamon-Client`), "android/0.6.0".
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub app: Option<String>,
	/// The id of its own table in phones.toml, made when it paired; `profile` is the one it gets,
	/// this one or another (`elamond clients profile`). None for a phone without one.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub own_profile: Option<String>,
	/// The address its requests came from last, "192.0.2.7": where the phone was, for `elamond clients`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub address: Option<String>,
}

pub const CLIENTS_FILE: &str = "clients.json";
/// Locked around every change to clients.json, by the service and the commands alike, so two
/// changes at once never lose one of them.
pub const CLIENTS_LOCK_FILE: &str = "clients.lock";

/// The lock on the paired phones in a data directory, let go when dropped.
pub struct ClientsLock(#[allow(dead_code)] fs::File);

/// Waits for, and takes, the lock on the paired phones in `dir`: hold it from reading clients.json
/// to writing it back. An advisory flock, so another elamond (the service, or a command) that
/// changes the file waits, and two in one process too.
pub fn lock_clients(dir: &Path) -> io::Result<ClientsLock> {
	use std::os::fd::AsRawFd;
	private_dir(dir)?;
	let f = fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).mode(0o600).open(dir.join(CLIENTS_LOCK_FILE))?;
	loop {
		// SAFETY: flock on a descriptor `f` owns; it doesn't touch memory.
		if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) } == 0 {
			return Ok(ClientsLock(f));
		}
		let e = io::Error::last_os_error();
		if e.kind() != io::ErrorKind::Interrupted {
			return Err(e);
		}
	}
}

/// The paired phones; none when the file doesn't exist yet. A file that can't be read or
/// doesn't parse is an error, never an empty list: that would unpair every phone.
pub fn load_clients() -> io::Result<Vec<Client>> {
	load_clients_in(&data_dir())
}

pub fn load_clients_in(dir: &Path) -> io::Result<Vec<Client>> {
	Ok(read_json(&dir.join(CLIENTS_FILE))?.unwrap_or_default())
}

pub fn save_clients(clients: &[Client]) -> io::Result<()> {
	save_clients_in(&data_dir(), clients)
}

pub fn save_clients_in(dir: &Path, clients: &[Client]) -> io::Result<()> {
	write_json::<Vec<Client>>(&dir.join(CLIENTS_FILE), clients)
}

/// What clients.json in `dir` looks like on disk, to notice that `pair`, `revoke` or
/// `clients role` changed it. Every write is a new file, so its inode changes even when the
/// clock doesn't.
pub fn clients_stamp_in(dir: &Path) -> Option<(SystemTime, u64, u64)> {
	let m = fs::metadata(dir.join(CLIENTS_FILE)).ok()?;
	Some((m.modified().ok()?, m.ino(), m.len()))
}

/// Why a phone wasn't added.
#[derive(Debug)]
pub enum NotAdded {
	/// `limit::PHONES_MAX` phones are paired already.
	Full,
	/// The entry that asked for a key of its own got it by asking itself, or has given one already.
	Rekeyed,
	/// The entry that asked is no longer paired.
	Gone,
	Io(io::Error),
}

impl From<io::Error> for NotAdded {
	fn from(e: io::Error) -> Self {
		NotAdded::Io(e)
	}
}

impl std::fmt::Display for NotAdded {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			NotAdded::Full => write!(f, "{} phones are paired already, the most there may be; revoke some first", limit::PHONES_MAX),
			NotAdded::Rekeyed => write!(f, "that entry was itself added with another's key, or has given a key of its own already"),
			NotAdded::Gone => write!(f, "the entry that asked is no longer paired"),
			NotAdded::Io(e) => write!(f, "{e}"),
		}
	}
}

/// The profiles a new phone gets: the one it shows (`None` for `default_profile`), and its own
/// table in phones.toml, if it has one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profiles {
	pub profile: Option<String>,
	pub own: Option<String>,
}

/// A phone added: its entry and token, and the entry it replaced (the same phone, paired before).
#[derive(Debug)]
pub struct Added {
	pub client: Client,
	pub token: String,
	pub replaced: Option<Client>,
}

/// Whether `token` is the one `client` was given.
pub fn proves(client: &Client, token: &str) -> bool {
	same(sha256_hex(token.as_bytes()).as_bytes(), client.token_hash.as_bytes())
}

/// Adds a phone with `role` and `profile` to those paired in `dir` and returns its token.
#[cfg(test)]
pub fn add_client_in(dir: &Path, name: &str, role: Role, profile: Option<&str>) -> Result<(Client, String), NotAdded> {
	let given = Profiles { profile: profile.map(str::to_string), own: None };
	pair_client(dir, name, role, None, |_, _, _| given).map(|a| (a.client, a.token))
}

/// Adds a phone named `name` with `role` to those paired in `dir`. `previous`, an entry's id and
/// its token, is the same phone paired before: while that entry is still paired with that token, it
/// goes in the same write. `give` decides the profiles under the lock, from the phones that stay
/// paired, the entry replaced and the new phone's id; it may write phones.toml, which is changed
/// under the same lock.
pub fn pair_client(
	dir: &Path,
	name: &str,
	role: Role,
	previous: Option<(&str, &str)>,
	give: impl FnOnce(&[Client], Option<&Client>, &str) -> Profiles,
) -> Result<Added, NotAdded> {
	let _lock = lock_clients(dir)?;
	let mut clients = match load_clients_in(dir) {
		Ok(c) => c,
		// The damaged file is set aside by the write below; the phones in it pair again.
		Err(e) if e.kind() == io::ErrorKind::InvalidData => Vec::new(),
		Err(e) => return Err(e.into()),
	};
	let at = previous.and_then(|(id, token)| clients.iter().position(|c| c.id == id && proves(c, token)));
	let replaced = at.map(|i| clients.remove(i));
	if clients.len() >= limit::PHONES_MAX {
		return Err(NotAdded::Full);
	}
	let (id, token) = new_key(&clients);
	let given = give(&clients, replaced.as_ref(), &id);
	let client = new_client(id, &token, name, role, given, None);
	clients.push(client.clone());
	save_clients_in(dir, &clients)?;
	Ok(Added { client, token, replaced })
}

/// Adds a phone beside those paired in `dir`, asked for with the token of the entry `from` (a key
/// of its own for a phone that came by that token), with that entry's role and profile; returns its
/// token. Only an entry paired with a code may ask, and only once, so one token gives one key of
/// its own, however often it is copied. A file that doesn't parse is an error here, never a fresh
/// start: that would unpair every other phone.
#[cfg(test)]
pub fn add_client_from(dir: &Path, from: &str, name: &str) -> Result<(Client, String), NotAdded> {
	rekey_client(dir, from, name, |_, asking, _| Profiles { profile: asking.profile.clone(), own: None }).map(|a| (a.client, a.token))
}

/// `add_client_from`, `give` deciding the new phone's profiles under the lock from the phones
/// paired, the entry that asked and the new phone's id, as `pair_client`'s does.
pub fn rekey_client(dir: &Path, from: &str, name: &str, give: impl FnOnce(&[Client], &Client, &str) -> Profiles) -> Result<Added, NotAdded> {
	let _lock = lock_clients(dir)?;
	let mut clients = load_clients_in(dir)?;
	let asking = clients.iter().find(|c| c.id == from).ok_or(NotAdded::Gone)?;
	if asking.from.is_some() || clients.iter().any(|c| c.from.as_deref() == Some(from)) {
		return Err(NotAdded::Rekeyed);
	}
	if clients.len() >= limit::PHONES_MAX {
		return Err(NotAdded::Full);
	}
	let role = asking.role;
	let (id, token) = new_key(&clients);
	let given = give(&clients, asking, &id);
	let client = new_client(id, &token, name, role, given, Some(from));
	clients.push(client.clone());
	save_clients_in(dir, &clients)?;
	Ok(Added { client, token, replaced: None })
}

/// A new phone's id and token: short ids, as phones and people know them, never one another phone
/// has or came by.
fn new_key(clients: &[Client]) -> (String, String) {
	let token = data_encoding::BASE64URL_NOPAD.encode(&random_bytes::<{ protocol::TOKEN_BYTES }>());
	let id = loop {
		let id = data_encoding::HEXLOWER.encode(&random_bytes::<{ protocol::CLIENT_ID_BYTES }>());
		if !clients.iter().any(|c| c.id == id || c.from.as_deref() == Some(id.as_str())) {
			break id;
		}
	};
	(id, token)
}

fn new_client(id: String, token: &str, name: &str, role: Role, given: Profiles, from: Option<&str>) -> Client {
	Client {
		id,
		name: sanitize_name(name),
		token_hash: sha256_hex(token.as_bytes()),
		created: now(),
		role,
		from: from.map(str::to_string),
		profile: given.profile,
		app: None,
		own_profile: given.own,
		address: None,
	}
}

/// Notes in clients.json that the phone `id` is the app `app` ("android/0.6.0") and came from
/// `address` last; what is None is left as it is. Nothing is written when it says so already, when
/// the phone is gone, or when the file doesn't parse.
pub fn note_client(dir: &Path, id: &str, app: Option<&str>, address: Option<&str>) -> io::Result<()> {
	let _lock = lock_clients(dir)?;
	let mut clients = load_clients_in(dir)?;
	let Some(c) = clients.iter_mut().find(|c| c.id == id) else { return Ok(()) };
	let mut changed = false;
	for (kept, now) in [(&mut c.app, app), (&mut c.address, address)] {
		if let Some(now) = now
			&& kept.as_deref() != Some(now)
		{
			*kept = Some(now.to_string());
			changed = true;
		}
	}
	if !changed {
		return Ok(());
	}
	save_clients_in(dir, &clients)
}

/// The family of the entry `id` among `clients`: the entry whose key it came by, following `from`
/// as far as it goes, or itself. A family shares one rate of acting and one share of the activity
/// log, however many keys of its own it asked for.
pub fn family_of(clients: &[Client], id: &str) -> String {
	let mut at = id.to_string();
	let mut seen = vec![at.clone()];
	while let Some(from) = clients.iter().find(|c| c.id == at).and_then(|c| c.from.clone()) {
		if seen.contains(&from) {
			break;
		}
		seen.push(from.clone());
		at = from;
	}
	at
}

/// The entries added with the key of `id`, and with theirs in turn: every phone whose key came,
/// one step or more, from that one's token.
pub fn added_with<'a>(clients: &'a [Client], id: &str) -> Vec<&'a Client> {
	let mut out: Vec<&Client> = Vec::new();
	let mut look = vec![id.to_string()];
	while let Some(parent) = look.pop() {
		for c in clients.iter().filter(|c| c.from.as_deref() == Some(parent.as_str())) {
			if c.id != id && !out.iter().any(|o| o.id == c.id) {
				out.push(c);
				look.push(c.id.clone());
			}
		}
	}
	out
}

/// Characters that don't show: controls, and Unicode's format characters (bidirectional
/// overrides, zero-width spaces and joiners, tags), which make a name read as another.
pub fn unseen(c: char) -> bool {
	c.is_control()
		|| matches!(
			c as u32,
			0xAD | 0x600..=0x605
				| 0x61C | 0x6DD | 0x70F | 0x890..=0x891
				| 0x8E2 | 0x180E | 0x200B..=0x200F
				| 0x202A..=0x202E
				| 0x2060..=0x2064
				| 0x2066..=0x206F
				| 0xFEFF | 0xFFF9..=0xFFFB
				| 0x110BD | 0x110CD | 0x13430..=0x1343F
				| 0x1BCA0..=0x1BCA3
				| 0x1D173..=0x1D17A
				| 0xE0001 | 0xE0020..=0xE007F
		)
}

/// Phone-supplied names end up in a terminal: nothing that doesn't show, sane length.
pub fn sanitize_name(name: &str) -> String {
	let clean: String = name.chars().filter(|c| !unseen(*c)).take(limit::NAME_MAX).collect();
	let clean = clean.trim();
	if clean.is_empty() { "Phone".into() } else { clean.to_string() }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Pairing {
	pub code: String,
	pub expires: u64,
	/// Wrong codes tried, from anywhere.
	pub attempts: u32,
	/// The role the phone that pairs with this code gets. A code with no role pairs an admin.
	#[serde(default)]
	pub role: Role,
	/// The profile it gets; none for `default_profile`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub profile: Option<String>,
	/// Wrong codes tried per address (see `address_key`).
	#[serde(default)]
	pub wrong: BTreeMap<String, u32>,
	/// A phone that gives the right code waits for the person at `elamond pair` to let it in. Not
	/// with `elamond pair --yes`.
	#[serde(default)]
	pub confirm: bool,
	/// The phone that gave the right code, while it waits for that answer.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub ask: Option<Ask>,
}

/// A phone that gave the right code, as `elamond pair` shows it to the person who decides.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Ask {
	/// Tells this request from any other.
	pub id: String,
	/// The name it gave, cleaned.
	pub name: String,
	/// Where it asked from.
	pub address: String,
	/// Unix time it asked.
	pub at: u64,
	/// The answer, once given.
	#[serde(default)]
	pub allow: Option<bool>,
	/// The entry of this phone paired before, as its `previous` proved: it replaces that one.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub replaces: Option<Replaces>,
	/// A phone's table in phones.toml no paired phone has, of an unpaired phone named as this one:
	/// offered to it, when the person at `elamond pair` says so.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub offer: Option<String>,
	/// That answer, given with `allow`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub take: Option<bool>,
}

/// The entry a phone that pairs again replaces, as `elamond pair` names it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Replaces {
	pub id: String,
	pub name: String,
	/// Its own profile, the one the new entry keeps; none when `elamond pair --profile` decides.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub profile: Option<String>,
}

/// How one attempt at the pending code went.
#[derive(Debug, PartialEq, Eq)]
pub enum Attempt {
	Right,
	Wrong,
	/// The address has tried too many wrong codes; it isn't let near this one again.
	Blocked,
}

/// The pending pairing code's file in a data directory.
pub const PAIRING_FILE: &str = "pairing.json";

fn pairing_path(dir: &Path) -> PathBuf {
	dir.join(PAIRING_FILE)
}

impl Pairing {
	/// Still good at Unix time `now`; the service and `elamond pair` both ask this.
	pub fn usable(&self, now: u64) -> bool {
		now < self.expires && self.attempts < limit::PAIRING_ATTEMPTS
	}

	/// Judges an attempt with `code` from the address `from`, counting it when it's wrong. An
	/// address that has used up its wrong attempts is refused whatever it sends, so it can't
	/// keep guessing; the others can still pair until the code runs out altogether.
	pub fn attempt(&mut self, from: &str, code: &str) -> Attempt {
		if self.wrong.get(from).is_some_and(|n| *n >= limit::PAIRING_ATTEMPTS_PER_ADDRESS) {
			return Attempt::Blocked;
		}
		if same(code.as_bytes(), self.code.as_bytes()) {
			return Attempt::Right;
		}
		self.attempts += 1;
		*self.wrong.entry(from.to_string()).or_default() += 1;
		Attempt::Wrong
	}
}

/// Who an attempt counts against: an IPv4 address, or the network an IPv6 one is in.
pub fn address_key(ip: IpAddr) -> String {
	match ip.to_canonical() {
		IpAddr::V4(a) => a.to_string(),
		IpAddr::V6(a) => {
			let bits = limit::PAIRING_V6_PREFIX;
			let net = u128::from(a) & (u128::MAX << (128 - bits));
			format!("{}/{bits}", std::net::Ipv6Addr::from(net))
		}
	}
}

/// The pending code as saved, whether or not it is still good.
pub fn read_pairing(dir: &Path) -> Option<Pairing> {
	read_json(&pairing_path(dir)).ok().flatten()
}

/// The pending code, if it is still good.
pub fn load_pairing(dir: &Path) -> Option<Pairing> {
	read_pairing(dir).filter(|p| p.usable(now()))
}

pub fn save_pairing(dir: &Path, p: &Pairing) -> io::Result<()> {
	write_private(&pairing_path(dir), &serde_json::to_vec(p).map_err(io::Error::other)?)
}

pub fn clear_pairing(dir: &Path) {
	let _ = fs::remove_file(pairing_path(dir));
}

/// Withdraws a code, unless another `elamond pair` has replaced it since.
pub fn clear_pairing_code(dir: &Path, code: &str) {
	if read_pairing(dir).is_some_and(|p| p.code == code) {
		clear_pairing(dir);
	}
}

/// A new code, uniformly random, for a phone with `role` and `profile`, to be let in by the person
/// at `elamond pair` when `confirm`.
pub fn new_pairing(dir: &Path, role: Role, profile: Option<&str>, confirm: bool) -> io::Result<Pairing> {
	let span = 10u32.pow(protocol::PAIRING_DIGITS);
	let code = loop {
		let n = u32::from_le_bytes(random_bytes::<4>());
		// Rejecting the top sliver keeps every code equally likely.
		if n < u32::MAX - u32::MAX % span {
			break format!("{:0width$}", n % span, width = protocol::PAIRING_DIGITS as usize);
		}
	};
	let p = Pairing {
		code,
		expires: now() + timeout::PAIRING_TTL_SECS,
		attempts: 0,
		role,
		profile: profile.map(str::to_string),
		wrong: BTreeMap::new(),
		confirm,
		ask: None,
	};
	save_pairing(dir, &p)?;
	Ok(p)
}

/// Gives the answer `allow` to the phone `ask_id` waiting with the code `code`, and `take` to the
/// table offered it, unless it no longer waits there.
pub fn answer_pairing(dir: &Path, code: &str, ask_id: &str, allow: bool, take: Option<bool>) -> io::Result<()> {
	match read_pairing(dir) {
		Some(mut p) if p.code == code && p.ask.as_ref().is_some_and(|a| a.id == ask_id && a.allow.is_none()) => {
			if let Some(a) = p.ask.as_mut() {
				a.allow = Some(allow);
				a.take = take;
			}
			save_pairing(dir, &p)
		}
		_ => Ok(()),
	}
}

/// Held by `elamond pair` while it runs, so the service knows someone is there to answer a phone.
const PAIRING_LOCK_FILE: &str = "pairing.lock";

/// Takes the lock `elamond pair` holds in `dir`, waiting up to `wait` for another to let go.
pub fn hold_pairing(dir: &Path, wait: std::time::Duration) -> io::Result<Option<fs::File>> {
	private_dir(dir)?;
	let f = fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).mode(0o600).open(dir.join(PAIRING_LOCK_FILE))?;
	let started = std::time::Instant::now();
	loop {
		if try_lock(&f)? {
			return Ok(Some(f));
		}
		if started.elapsed() >= wait {
			return Ok(None);
		}
		std::thread::sleep(std::time::Duration::from_millis(100));
	}
}

/// Whether an `elamond pair` that can answer runs with the data directory `dir`.
pub fn pairing_answered(dir: &Path) -> bool {
	match fs::File::open(dir.join(PAIRING_LOCK_FILE)) {
		Ok(f) => matches!(try_lock(&f), Ok(false)),
		Err(_) => false,
	}
}

/// `elamond control off`'s file in the data directory: while it exists, nobody acts on the
/// computer, whatever the settings say.
pub const CONTROL_OFF_FILE: &str = "control-off.json";

#[derive(Serialize, Deserialize)]
struct ControlOff {
	/// Unix time it was turned off.
	since: u64,
}

pub fn control_off_path(dir: &Path) -> PathBuf {
	dir.join(CONTROL_OFF_FILE)
}

/// Whether `elamond control off` holds control off. A file that can't be read or parsed holds
/// it off too: it is there, and was put there for that.
pub fn control_held_off(dir: &Path) -> bool {
	fs::symlink_metadata(control_off_path(dir)).is_ok()
}

/// Since when control has been held off, if it is; 0 when the file doesn't say.
pub fn control_off_since(dir: &Path) -> Option<u64> {
	control_held_off(dir).then(|| read_json::<ControlOff>(&control_off_path(dir)).ok().flatten().map_or(0, |c| c.since))
}

/// Holds control off, or lets the settings decide again.
pub fn set_control_off(dir: &Path, off: bool) -> io::Result<()> {
	let path = control_off_path(dir);
	if off {
		if control_held_off(dir) {
			return Ok(());
		}
		write_private(&path, &serde_json::to_vec(&ControlOff { since: now() }).map_err(io::Error::other)?)
	} else {
		match fs::remove_file(&path) {
			Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
			_ => Ok(()),
		}
	}
}

/// Held by `elamond serve` for as long as it runs, so `elamond restore` can tell that the service
/// is using the data directory, and the service waits for a restore to finish before it starts.
const SERVE_LOCK_FILE: &str = "serve.lock";

/// A lock on the data directory `dir`: `Some` when this process now holds it (it is let go when
/// the file is dropped), `None` when another one does.
pub fn lock_data_dir(dir: &Path) -> io::Result<Option<fs::File>> {
	private_dir(dir)?;
	let f = fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).mode(0o600).open(dir.join(SERVE_LOCK_FILE))?;
	Ok(try_lock(&f)?.then_some(f))
}

/// Whether another process holds `dir`'s lock: a running service, or a restore writing to it.
/// Only looks: nothing is made or changed, and the lock, if taken for the look, is let go.
pub fn data_dir_held(dir: &Path) -> bool {
	match fs::File::open(dir.join(SERVE_LOCK_FILE)) {
		Ok(f) => matches!(try_lock(&f), Ok(false)),
		Err(_) => false,
	}
}

/// Takes the lock on `f` if no one else holds it.
fn try_lock(f: &fs::File) -> io::Result<bool> {
	use std::os::fd::AsRawFd;
	// SAFETY: flock on a descriptor `f` owns; it doesn't touch memory.
	if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
		return Ok(true);
	}
	match io::Error::last_os_error() {
		e if e.raw_os_error() == Some(libc::EWOULDBLOCK) => Ok(false),
		e => Err(e),
	}
}

/// Equal-time comparison, so response timing doesn't reveal how much of a secret matched.
pub fn same(a: &[u8], b: &[u8]) -> bool {
	a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn links_merge_by_id_across_files() {
		let dir = scratch();
		let write = |name: &str, text: &str| fs::write(dir.join(name), text).unwrap();
		write("main.toml", "[links.a]\nname = \"A\"\nurl = \"http://{host}:1\"\n");
		write("two.toml", "[links.b]\nname = \"B\"\nurl = \"http://{host}:2\"\n[links.a]\nunit = \"a.service\"\n");
		let c = crate::settings::load(&[dir.join("main.toml"), dir.join("two.toml"), dir.join("missing.toml")]).unwrap();
		assert_eq!(c.links.keys().collect::<Vec<_>>(), ["a", "b"]);
		// One key of a link changed by a later file; the rest of it stays.
		assert_eq!((c.links["a"].name.as_str(), c.links["a"].unit.as_deref()), ("A", Some("a.service")));
		// A mistake names the file and the line.
		write("bad.toml", "port = 1\n\n[links.c]\nname = 5\nurl = \"x\"\n");
		let e = crate::settings::join_mistakes(&crate::settings::load(&[dir.join("bad.toml")]).unwrap_err());
		assert!(e.contains("bad.toml:4: links.c.name"), "{e}");
		// So does a key the settings don't have.
		write("unknown.toml", "port = 1\n[[pages]]\nname = \"a\"\n");
		let e = crate::settings::join_mistakes(&crate::settings::load(&[dir.join("unknown.toml")]).unwrap_err());
		assert!(e.contains("unknown.toml:2: pages: no key \"pages\""), "{e}");
		fs::remove_dir_all(dir).unwrap();
	}

	#[test]
	fn settings_files_are_the_main_one_then_the_dropins_by_name() {
		let dir = scratch();
		let link = |id: &str| format!("[links.{id}]\nname = \"{id}\"\nurl = \"http://{{host}}\"\n");
		let main = dir.join("elamond.toml");
		// Nothing yet: the main file, for the settings to be written under.
		assert_eq!(settings_files(&dir, None), std::slice::from_ref(&main));

		fs::write(&main, "port = 1\n".to_string() + &link("main")).unwrap();
		fs::create_dir_all(dir.join("elamond.d")).unwrap();
		fs::write(dir.join("elamond.d/b.toml"), "port = 2\n".to_string() + &link("b")).unwrap();
		fs::write(dir.join("elamond.d/a.toml"), link("a")).unwrap();
		fs::write(dir.join("elamond.d/notes.txt"), "not settings").unwrap();
		let files = settings_files(&dir, None);
		assert_eq!(files, [main.clone(), dir.join("elamond.d/a.toml"), dir.join("elamond.d/b.toml")]);
		let c = crate::settings::load(&files).unwrap();
		assert_eq!((c.port, c.links.keys().map(String::as_str).collect::<Vec<_>>()), (2, vec!["a", "b", "main"]));

		// ELAMON_CONFIG names the main file outright; the drop-ins stay in the config directory.
		let named = dir.join("store.toml");
		assert_eq!(settings_files(&dir, Some(named.clone()))[..2], [named, dir.join("elamond.d/a.toml")]);
		fs::remove_dir_all(dir).unwrap();
	}

	#[test]
	fn concurrent_changes_to_the_phones_lose_none() {
		let dir = scratch();
		let mut writers = Vec::new();
		// Pairing and keys of their own add phones while a command changes every phone's role.
		for t in 0..8 {
			let dir = dir.clone();
			writers.push(std::thread::spawn(move || {
				for i in 0..5 {
					add_client_in(&dir, &format!("Phone {t}.{i}"), Role::Monitor, None).unwrap();
				}
			}));
		}
		for t in 0..4 {
			let dir = dir.clone();
			writers.push(std::thread::spawn(move || {
				for _ in 0..5 {
					let _lock = lock_clients(&dir).unwrap();
					let mut list = load_clients_in(&dir).unwrap();
					for c in &mut list {
						c.role = if t % 2 == 0 { Role::Glance } else { Role::Operate };
					}
					save_clients_in(&dir, &list).unwrap();
				}
			}));
		}
		for w in writers {
			w.join().unwrap();
		}
		let list = load_clients_in(&dir).unwrap();
		assert_eq!(list.len(), 40);
		let mut ids: Vec<&str> = list.iter().map(|c| c.id.as_str()).collect();
		ids.sort();
		ids.dedup();
		assert_eq!(ids.len(), 40);
		fs::remove_dir_all(dir).unwrap();
	}

	#[test]
	fn a_token_gives_one_key_of_its_own_and_the_phones_have_a_limit() {
		let dir = scratch();
		let (root, _) = add_client_in(&dir, "Old", Role::Operate, Some("owner")).unwrap();
		let (new, _) = add_client_from(&dir, &root.id, "New").unwrap();
		assert_eq!(new.from.as_deref(), Some(root.id.as_str()));
		// With the role and the profile of the entry whose token asked.
		assert_eq!((new.role, new.profile.as_deref()), (Role::Operate, Some("owner")));
		// Once from the entry paired with a code, never from the one that came by it.
		assert!(matches!(add_client_from(&dir, &root.id, "Again"), Err(NotAdded::Rekeyed)));
		assert!(matches!(add_client_from(&dir, &new.id, "Third"), Err(NotAdded::Rekeyed)));
		assert!(matches!(add_client_from(&dir, "gone", "Ghost"), Err(NotAdded::Gone)));
		// A family: the entry the key came from, however far.
		let list = load_clients_in(&dir).unwrap();
		assert_eq!((family_of(&list, &new.id), family_of(&list, &root.id)), (root.id.clone(), root.id.clone()));
		assert_eq!(added_with(&list, &root.id).iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), [new.id.as_str()]);
		// Chains from before the limit, and loops written by hand, end.
		let chain: Vec<Client> = [("a", None), ("b", Some("a")), ("c", Some("b")), ("x", Some("y")), ("y", Some("x"))]
			.iter()
			.map(|(id, from)| Client { id: id.to_string(), name: id.to_string(), from: from.map(str::to_string), ..Default::default() })
			.collect();
		assert_eq!((family_of(&chain, "c"), family_of(&chain, "a"), family_of(&chain, "gone")), ("a".into(), "a".into(), "gone".into()));
		assert_eq!(added_with(&chain, "a").iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["b", "c"]);
		assert!(["x", "y"].contains(&family_of(&chain, "x").as_str()));
		assert_eq!(added_with(&chain, "x").len(), 1);
		// No more than the limit, by pairing or by a key of its own.
		for i in list.len()..limit::PHONES_MAX {
			add_client_in(&dir, &format!("Phone {i}"), Role::Glance, None).unwrap();
		}
		assert!(matches!(add_client_in(&dir, "One more", Role::Glance, None), Err(NotAdded::Full)));
		let (spare, _) = {
			let mut list = load_clients_in(&dir).unwrap();
			list.retain(|c| c.id != new.id);
			save_clients_in(&dir, &list).unwrap();
			add_client_in(&dir, "Last", Role::Glance, None).unwrap()
		};
		assert!(matches!(add_client_from(&dir, &spare.id, "Over"), Err(NotAdded::Full)));
		assert_eq!(load_clients_in(&dir).unwrap().len(), limit::PHONES_MAX);
		fs::remove_dir_all(dir).unwrap();
	}

	#[test]
	fn names_lose_control_characters() {
		assert_eq!(sanitize_name("Pixel\u{202e}8\u{200d}\u{e0041} "), "Pixel8");
		assert_eq!(sanitize_name("  Pixel\u{7} 8\n "), "Pixel 8");
		assert_eq!(sanitize_name("\u{1b}[2J"), "[2J");
		assert_eq!(sanitize_name(" \t"), "Phone");
		assert_eq!(sanitize_name(&"я".repeat(100)).chars().count(), 64);
	}

	#[test]
	fn secrets_compare_whole() {
		assert!(same(b"abc", b"abc"));
		assert!(!same(b"abc", b"abd"));
		assert!(!same(b"abc", b"abcd"));
		assert!(same(b"", b""));
	}

	#[test]
	fn fingerprints_in_groups_of_four() {
		let fp = sha256_hex(b"x");
		let pretty = pretty_fingerprint(&fp);
		assert_eq!(pretty.split(' ').count(), 8);
		assert_eq!(pretty.replace(' ', ""), fp[..32].to_ascii_uppercase());
		assert_eq!(pretty_fingerprint("abcdef"), "ABCD EF");
	}

	#[test]
	fn pairing_codes_expire_and_run_out() {
		let p = Pairing { code: "123456".into(), expires: 100, attempts: 0, role: Role::Glance, profile: None, wrong: BTreeMap::new(), confirm: true, ask: None };
		assert!(p.usable(99));
		assert!(!p.usable(100));
		assert!(!Pairing { attempts: limit::PAIRING_ATTEMPTS, ..p }.usable(0));
	}

	#[test]
	fn wrong_codes_are_counted_per_address_and_in_all() {
		let mut p = Pairing { code: "123456".into(), expires: u64::MAX, attempts: 0, role: Role::Admin, profile: None, wrong: BTreeMap::new(), confirm: false, ask: None };
		for _ in 0..limit::PAIRING_ATTEMPTS_PER_ADDRESS {
			assert_eq!(p.attempt("192.0.2.1", "000000"), Attempt::Wrong);
		}
		// That address is done with this code, even with the right one; the code isn't.
		assert_eq!(p.attempt("192.0.2.1", "000000"), Attempt::Blocked);
		assert_eq!(p.attempt("192.0.2.1", "123456"), Attempt::Blocked);
		assert!(p.usable(0));
		assert_eq!(p.attempt("192.0.2.2", "123456"), Attempt::Right);
		// Blocked attempts and the right one aren't counted.
		assert_eq!(p.attempts, limit::PAIRING_ATTEMPTS_PER_ADDRESS);
		// From many addresses the code runs out after the total.
		let mut from = 0;
		while p.usable(0) {
			assert_ne!(p.attempt(&format!("198.51.100.{}", from / 4), "000000"), Attempt::Blocked);
			from += 1;
		}
		assert_eq!(p.attempts, limit::PAIRING_ATTEMPTS);
		// What it saves reads back, and a code with no role pairs an admin without asking.
		let back: Pairing = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
		assert_eq!(back, p);
		let bare: Pairing = serde_json::from_str(r#"{"code":"123456","expires":1,"attempts":2}"#).unwrap();
		assert_eq!((bare.role, bare.wrong.len(), bare.confirm, bare.ask), (Role::Admin, 0, false, None));
	}

	#[test]
	fn addresses_and_their_networks() {
		let key = |s: &str| address_key(s.parse().unwrap());
		assert_eq!(key("192.0.2.7"), "192.0.2.7");
		// IPv4 through an IPv6 socket is still the IPv4 address.
		assert_eq!(key("::ffff:192.0.2.7"), "192.0.2.7");
		assert_eq!(key("2001:db8:1:2:3:4:5:6"), "2001:db8:1:2::/64");
		assert_eq!(key("2001:db8:1:2:ffff::1"), key("2001:db8:1:2::9"));
		assert_ne!(key("2001:db8:1:2::1"), key("2001:db8:1:3::1"));
	}

	#[test]
	fn a_phone_with_no_role_is_admin() {
		let dir = scratch();
		fs::write(dir.join("clients.json"), r#"[{"id":"a","name":"Phone","token_hash":"h","created":1}]"#).unwrap();
		let mut list = load_clients_in(&dir).unwrap();
		assert_eq!(list[0].role, Role::Admin);
		list[0].role = Role::Glance;
		save_clients_in(&dir, &list).unwrap();
		assert_eq!(load_clients_in(&dir).unwrap()[0].role, Role::Glance);
		fs::remove_dir_all(dir).unwrap();
	}

	#[test]
	fn control_held_off_by_its_file() {
		let dir = scratch();
		assert!(!control_held_off(&dir));
		assert_eq!(control_off_since(&dir), None);
		set_control_off(&dir, true).unwrap();
		let since = control_off_since(&dir).unwrap();
		assert!(since > 0);
		// Again: the time it was first held off stays.
		set_control_off(&dir, true).unwrap();
		assert_eq!(control_off_since(&dir), Some(since));
		set_control_off(&dir, false).unwrap();
		set_control_off(&dir, false).unwrap();
		assert!(!control_held_off(&dir));
		// A file it can't read still holds control off.
		fs::write(control_off_path(&dir), "?").unwrap();
		assert_eq!(control_off_since(&dir), Some(0));
		fs::remove_dir_all(dir).unwrap();
	}

	fn scratch() -> PathBuf {
		let dir = std::env::temp_dir().join(format!("elamond-test-{}", data_encoding::HEXLOWER.encode(&random_bytes::<6>())));
		fs::create_dir_all(&dir).unwrap();
		dir
	}

	#[test]
	fn a_damaged_state_file_is_kept_not_overwritten() {
		let dir = scratch();
		let path = dir.join("clients.json");
		assert!(read_json::<Vec<Client>>(&path).unwrap().is_none());

		fs::write(&path, b"[{\"id\": broken").unwrap();
		assert_eq!(read_json::<Vec<Client>>(&path).unwrap_err().kind(), io::ErrorKind::InvalidData);
		let client = Client { id: "a".into(), name: "b".into(), token_hash: "c".into(), created: 1, ..Default::default() };
		write_json::<Vec<Client>>(&path, &[client]).unwrap();

		let names: Vec<String> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
		let kept = names.iter().find(|n| n.starts_with("clients.json.corrupt-")).expect("the damaged file is kept");
		assert_eq!(fs::read(dir.join(kept)).unwrap(), b"[{\"id\": broken");
		assert_eq!(read_json::<Vec<Client>>(&path).unwrap().unwrap()[0].id, "a");
		// No temporary files are left behind.
		assert_eq!(names.len(), 2, "{names:?}");
		assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
		fs::remove_dir_all(dir).unwrap();
	}
}
