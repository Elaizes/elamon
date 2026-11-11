//! Files elamond keeps: settings, paired phones and the pending pairing code; its TLS identity
//! is in `identity`.
//!
//! Settings:  $XDG_CONFIG_HOME/elamon/elamond.toml (optional), then elamond.d/*.toml beside it
//!            (elamond.toml and elamond.d/ before 0.5, still read; see `settings_files`)
//! State:     $XDG_DATA_HOME/elamon/{id, cert.der, key.der, clients.json, pairing.json,
//!            control-off.json, activity.jsonl, links.json, rotations.json}

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::net::IpAddr;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::access::Role;
use crate::listen::Target;
use crate::consts::{limit, protocol, timeout};

#[derive(Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
	/// Addresses and interfaces to listen on; "::", the default, takes every address, IPv4 too.
	#[serde(deserialize_with = "crate::listen::deserialize")]
	pub listen: Vec<Target>,
	pub port: u16,
	/// Host names that reach this computer, for `elamond pair` to offer after its addresses.
	#[serde(deserialize_with = "crate::names::deserialize")]
	pub names: Vec<String>,
	/// Name shown in the app; the hostname when empty.
	pub name: String,
	/// Announce the service on the local network so the app finds it.
	pub mdns: bool,
	pub interval_ms: u64,
	pub history_minutes: u64,
	/// Let paired phones act on this computer: power, profiles, units, processes,
	/// media and the actions below. Off unless turned on here.
	pub control: bool,
	/// The role a phone pairs with when `elamond pair` isn't given one.
	pub default_role: Role,
	/// Links to web pages this computer hosts, shown in the app beside those added from the
	/// app itself.
	pub links: Vec<LinkConfig>,
	/// `[[services]]`, what `[[links]]` were called before 0.5, still accepted. `load_config`
	/// moves them into `links` file by file, so this is only filled while one file is checked.
	#[serde(rename = "services")]
	pub old_links: Vec<LinkConfig>,
	/// Commands the app can run, defined only here: a phone can't add its own.
	pub actions: Vec<ActionConfig>,
	/// Units a link added from the app may start, stop and restart, beside those of `[[links]]`:
	/// a phone can only name a unit the settings name.
	#[serde(deserialize_with = "deserialize_units")]
	pub units: Vec<UnitRule>,
}

/// A unit the settings let phones start, stop and restart, in its scope: "user" or "system".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitRule {
	pub name: String,
	pub scope: String,
}

/// `units` as the settings give it: a name is a user unit, `{ name = "…", scope = "system" }` a
/// system one. Only service units that could ever be allowed are taken.
fn deserialize_units<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<UnitRule>, D::Error> {
	#[derive(Deserialize)]
	#[serde(deny_unknown_fields)]
	struct Table {
		name: String,
		scope: Option<String>,
	}
	#[derive(Deserialize)]
	#[serde(untagged)]
	enum Entry {
		Name(String),
		Table(Table),
	}
	let entries = Vec::<Entry>::deserialize(d)?;
	entries
		.into_iter()
		.map(|e| {
			let (name, scope) = match e {
				Entry::Name(n) => (n, None),
				Entry::Table(t) => (t.name, t.scope),
			};
			let name = name.trim().to_string();
			let scope = scope.unwrap_or_else(|| "user".into());
			if scope != "user" && scope != "system" {
				return Err(format!("units: scope \"{scope}\" of {name} is neither \"user\" nor \"system\""));
			}
			crate::links::unit_shape(&name).map_err(|why| format!("units: \"{name}\" {why}"))?;
			Ok(UnitRule { name, scope })
		})
		.collect::<Result<_, _>>()
		.map_err(serde::de::Error::custom)
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct LinkConfig {
	pub name: String,
	/// What the app opens; "{host}" becomes the address the phone reaches this computer at.
	pub url: String,
	/// A systemd unit the app may start, stop and restart.
	pub unit: Option<String>,
	/// "user" (default) or "system".
	pub scope: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct ActionConfig {
	pub name: String,
	/// Program and arguments, run without a shell.
	pub command: Vec<String>,
	/// Ask in the app before running.
	#[serde(default = "yes")]
	pub confirm: bool,
	/// Seconds before the command is stopped.
	#[serde(default = "minute")]
	pub timeout: u64,
}

/// Ids for what the settings define, from what it is rather than where it stands in the
/// file: reordering the file or adding a drop-in must not point a phone's button at
/// another command.
fn content_id(parts: &[&str]) -> String {
	sha256_hex(parts.join("\0").as_bytes())[..protocol::CONTENT_ID_LEN].to_string()
}

impl ActionConfig {
	pub fn id(&self) -> String {
		let mut parts = vec![self.name.as_str()];
		parts.extend(self.command.iter().map(String::as_str));
		content_id(&parts)
	}
}

impl LinkConfig {
	pub fn id(&self) -> String {
		content_id(&[&self.name, &self.url, self.unit.as_deref().unwrap_or(""), self.scope.as_deref().unwrap_or("")])
	}
}

fn yes() -> bool {
	true
}

fn minute() -> u64 {
	timeout::ACTION_DEFAULT_SECS
}

impl Default for Config {
	fn default() -> Self {
		Config {
			listen: vec![Target::ANY],
			port: protocol::DEFAULT_PORT,
			names: Vec::new(),
			name: String::new(),
			mdns: true,
			interval_ms: 1000,
			history_minutes: 10,
			control: false,
			default_role: Role::Admin,
			links: Vec::new(),
			old_links: Vec::new(),
			actions: Vec::new(),
			units: Vec::new(),
		}
	}
}

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
/// Their names before 0.5, still read: the file while there is no elamond.toml, the folder
/// before elamond.d.
const OLD_CONFIG_FILE: &str = "elamond.toml";
pub const OLD_DROPIN_DIR: &str = "elamond.d";

/// The settings files in the order they are read: the main file, then the drop-ins by name.
pub fn config_files() -> Vec<PathBuf> {
	settings_files(&config_dir(), std::env::var_os("ELAMON_CONFIG").map(PathBuf::from))
}

/// The main file is the one ELAMON_CONFIG names (the NixOS module points it into the store), or
/// elamond.toml in the config directory `dir`, or elamond.toml there while there is no
/// elamond.toml. Drop-ins are always in the user's own config directory: where NixOS or Home
/// Manager make the main file a read-only one in the store, these stay the user's to write.
/// elamond.d/ is read before elamond.d/, so the new folder has the last word.
fn settings_files(dir: &Path, named: Option<PathBuf>) -> Vec<PathBuf> {
	let main = named.unwrap_or_else(|| {
		let (new, old) = (dir.join(CONFIG_FILE), dir.join(OLD_CONFIG_FILE));
		if !new.exists() && old.exists() { old } else { new }
	});
	let dropins = [OLD_DROPIN_DIR, DROPIN_DIR].into_iter().flat_map(|name| {
		let mut files: Vec<PathBuf> = fs::read_dir(dir.join(name))
			.map(|d| d.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect())
			.unwrap_or_default();
		files.sort();
		files
	});
	std::iter::once(main).chain(dropins).collect()
}

/// What to tell about settings under their names before 0.5, among `files` read from `dir`.
fn old_name_notes(dir: &Path, files: &[PathBuf]) -> Vec<String> {
	let (new, old) = (dir.join(CONFIG_FILE), dir.join(OLD_CONFIG_FILE));
	let old_dir = dir.join(OLD_DROPIN_DIR);
	let mut notes = Vec::new();
	if files.first() == Some(&old) {
		notes.push(format!("{} is read for now; rename it to {CONFIG_FILE}", old.display()));
	} else if files.first() == Some(&new) && old.exists() {
		notes.push(format!("{} is not read: {CONFIG_FILE} beside it takes its place", old.display()));
	}
	if files.iter().any(|f| f.parent() == Some(old_dir.as_path())) {
		notes.push(format!("{}/ is read for now; move its files to {DROPIN_DIR}/", old_dir.display()));
	}
	notes
}

/// Set once the old names have been told about, so a reread doesn't repeat it.
static OLD_NAMES_TOLD: AtomicBool = AtomicBool::new(false);

/// Settings from elamond.toml and the drop-ins. A later file adds to the lists (links,
/// actions, units) and replaces anything else it sets.
pub fn load_config() -> Result<Config, String> {
	let files = config_files();
	let notes = old_name_notes(&config_dir(), &files);
	if !notes.is_empty() && !OLD_NAMES_TOLD.swap(true, Ordering::SeqCst) {
		for n in notes {
			eprintln!("settings: {n}");
		}
	}
	read_settings(&files)
}

fn read_settings(files: &[PathBuf]) -> Result<Config, String> {
	let mut merged = toml::Table::new();
	for path in files {
		let text = match fs::read_to_string(path) {
			Ok(t) => t,
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
			Err(e) => return Err(format!("{}: {e}", path.display())),
		};
		// Each file on its own first, so a mistake names the file and the line it is in.
		toml::from_str::<Config>(&text).map_err(|e| format!("{}: {e}", path.display()))?;
		let mut table: toml::Table = text.parse().map_err(|e| format!("{}: {e}", path.display()))?;
		old_links_to_links(&mut table);
		merge_toml(&mut merged, table);
	}
	toml::Value::Table(merged).try_into().map_err(|e| format!("settings: {e}"))
}

/// A file's `[[services]]` after its own `[[links]]`, under the one name: done to each file
/// before they are merged, so links in one file add to services in another, and the merged
/// settings never hold both.
fn old_links_to_links(table: &mut toml::Table) {
	let Some(old) = table.remove("services") else { return };
	match (table.get_mut("links"), old) {
		(Some(toml::Value::Array(links)), toml::Value::Array(old)) => links.extend(old),
		(_, old) => {
			table.insert("links".into(), old);
		}
	}
}

/// The settings a later file adds to rather than replaces.
const ADDED_UP: [&str; 4] = ["links", "services", "actions", "units"];

fn merge_toml(into: &mut toml::Table, from: toml::Table) {
	for (key, value) in from {
		match (into.get_mut(&key), value) {
			(Some(toml::Value::Array(a)), toml::Value::Array(b)) if ADDED_UP.contains(&key.as_str()) => a.extend(b),
			(Some(toml::Value::Table(a)), toml::Value::Table(b)) => merge_toml(a, b),
			(_, v) => {
				into.insert(key, v);
			}
		}
	}
}

/// What the settings files look like on disk now, to notice an edit. The resolved path is
/// part of it: a store file keeps the same mtime, its symlink gets a new target.
pub fn config_stamp() -> Vec<(PathBuf, Option<PathBuf>, Option<SystemTime>, u64)> {
	config_files()
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Client {
	pub id: String,
	pub name: String,
	/// SHA-256 of the client's token; the token itself is only ever on the phone.
	pub token_hash: String,
	pub created: u64,
	/// What the phone may do. Phones paired before 0.5 have none in the file and are admins,
	/// as they were.
	#[serde(default)]
	pub role: Role,
	/// The entry whose token asked for this one (`POST /api/v1/rekey`): a phone that came by
	/// another's token and got a key of its own. None for a phone paired with a code.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub from: Option<String>,
}

pub const CLIENTS_FILE: &str = "clients.json";

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

/// Adds a phone with `role` and returns its token.
pub fn add_client(name: &str, role: Role) -> io::Result<(Client, String)> {
	let clients = match load_clients() {
		Ok(c) => c,
		// The damaged file is set aside by the write below; the phones in it pair again.
		Err(e) if e.kind() == io::ErrorKind::InvalidData => Vec::new(),
		Err(e) => return Err(e),
	};
	push_client(&data_dir(), clients, name, role, None)
}

/// Adds a phone with `role` beside those paired in `dir`, asked for with the token of the entry
/// `from` (a key of its own for a phone that came by that token); returns its token. A file that
/// doesn't parse is an error here, never a fresh start: that would unpair every other phone.
pub fn add_client_from(dir: &Path, from: &str, name: &str, role: Role) -> io::Result<(Client, String)> {
	push_client(dir, load_clients_in(dir)?, name, role, Some(from))
}

fn push_client(dir: &Path, mut clients: Vec<Client>, name: &str, role: Role, from: Option<&str>) -> io::Result<(Client, String)> {
	let token = data_encoding::BASE64URL_NOPAD.encode(&random_bytes::<{ protocol::TOKEN_BYTES }>());
	let client = Client {
		id: data_encoding::HEXLOWER.encode(&random_bytes::<{ protocol::CLIENT_ID_BYTES }>()),
		name: sanitize_name(name),
		token_hash: sha256_hex(token.as_bytes()),
		created: now(),
		role,
		from: from.map(str::to_string),
	};
	clients.push(client.clone());
	save_clients_in(dir, &clients)?;
	Ok((client, token))
}

/// Phone-supplied names end up in a terminal: no control characters, sane length.
pub fn sanitize_name(name: &str) -> String {
	let clean: String = name.chars().filter(|c| !c.is_control()).take(limit::NAME_MAX).collect();
	let clean = clean.trim();
	if clean.is_empty() { "Phone".into() } else { clean.to_string() }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Pairing {
	pub code: String,
	pub expires: u64,
	/// Wrong codes tried, from anywhere.
	pub attempts: u32,
	/// The role the phone that pairs with this code gets. A code from `elamond pair` before
	/// 0.5 has none and pairs an admin, as it did.
	#[serde(default)]
	pub role: Role,
	/// Wrong codes tried per address (see `address_key`).
	#[serde(default)]
	pub wrong: BTreeMap<String, u32>,
}

/// How one attempt at the pending code went.
#[derive(Debug, PartialEq, Eq)]
pub enum Attempt {
	Right,
	Wrong,
	/// The address has tried too many wrong codes; it isn't let near this one again.
	Blocked,
}

fn pairing_path() -> PathBuf {
	data_dir().join("pairing.json")
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
pub fn read_pairing() -> Option<Pairing> {
	read_json(&pairing_path()).ok().flatten()
}

/// The pending code, if it is still good.
pub fn load_pairing() -> Option<Pairing> {
	read_pairing().filter(|p| p.usable(now()))
}

pub fn save_pairing(p: &Pairing) -> io::Result<()> {
	write_private(&pairing_path(), &serde_json::to_vec(p).map_err(io::Error::other)?)
}

pub fn clear_pairing() {
	let _ = fs::remove_file(pairing_path());
}

/// Withdraws a code, unless another `elamond pair` has replaced it since.
pub fn clear_pairing_code(code: &str) {
	if read_pairing().is_some_and(|p| p.code == code) {
		clear_pairing();
	}
}

/// A new code, uniformly random, for a phone with `role`.
pub fn new_pairing(role: Role) -> io::Result<Pairing> {
	let span = 10u32.pow(protocol::PAIRING_DIGITS);
	let code = loop {
		let n = u32::from_le_bytes(random_bytes::<4>());
		// Rejecting the top sliver keeps every code equally likely.
		if n < u32::MAX - u32::MAX % span {
			break format!("{:0width$}", n % span, width = protocol::PAIRING_DIGITS as usize);
		}
	};
	let p = Pairing { code, expires: now() + timeout::PAIRING_TTL_SECS, attempts: 0, role, wrong: BTreeMap::new() };
	save_pairing(&p)?;
	Ok(p)
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

	fn table(s: &str) -> toml::Table {
		s.parse().unwrap()
	}

	#[test]
	fn dropins_add_to_lists_and_replace_the_rest() {
		let mut m = table("control = false\nport = 1\n[[actions]]\nname = \"a\"\ncommand = [\"true\"]\n");
		merge_toml(&mut m, table("control = true\n[[actions]]\nname = \"b\"\ncommand = [\"false\"]\n[[links]]\nname = \"s\"\nurl = \"http://{host}\"\n"));
		let c: Config = toml::Value::Table(m).try_into().unwrap();
		assert!(c.control);
		assert_eq!(c.port, 1);
		assert_eq!(c.actions.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["a", "b"]);
		assert_eq!(c.links.len(), 1);
		// `listen` is a list, but a drop-in's replaces the one before, in either form.
		let mut m = table("listen = [\"eth0\", \"wg0\"]\n");
		merge_toml(&mut m, table("listen = [\"tailscale0\"]\n"));
		let c: Config = toml::Value::Table(m).try_into().unwrap();
		assert_eq!(c.listen, [Target::Interface("tailscale0".into())]);
		let mut m = table("listen = [\"eth0\"]\n");
		merge_toml(&mut m, table("listen = \"::\"\n"));
		assert_eq!(toml::Value::Table(m).try_into::<Config>().unwrap().listen, [Target::ANY]);
		assert_eq!(Config::default().listen, [Target::ANY]);
	}

	fn link_names(c: &Config) -> Vec<&str> {
		c.links.iter().map(|l| l.name.as_str()).collect()
	}

	#[test]
	fn services_are_links_under_their_old_name() {
		let dir = scratch();
		let write = |name: &str, text: &str| fs::write(dir.join(name), text).unwrap();
		write("main.toml", "[[services]]\nname = \"a\"\nurl = \"http://{host}:1\"\n");
		// Both names in one file: its links first.
		write("both.toml", "[[services]]\nname = \"c\"\nurl = \"http://{host}:3\"\n[[links]]\nname = \"b\"\nurl = \"http://{host}:2\"\n");
		write("new.toml", "[[links]]\nname = \"d\"\nurl = \"http://{host}:4\"\n");
		let c = read_settings(&[dir.join("main.toml"), dir.join("both.toml"), dir.join("new.toml"), dir.join("missing.toml")]).unwrap();
		assert_eq!(link_names(&c), ["a", "b", "c", "d"]);
		assert!(c.old_links.is_empty());
		// A mistake under the old name still names the file and the line.
		write("bad.toml", "port = 1\n\n[[services]]\nname = 5\nurl = \"x\"\n");
		let e = read_settings(&[dir.join("bad.toml")]).unwrap_err();
		assert!(e.contains("bad.toml") && e.contains("line 4"), "{e}");
		fs::remove_dir_all(dir).unwrap();
	}

	#[test]
	fn settings_files_by_their_old_names_too() {
		let dir = scratch();
		let link = |name: &str| format!("[[links]]\nname = \"{name}\"\nurl = \"http://{{host}}\"\n");
		let old = dir.join("elamond.toml");
		let new = dir.join("elamond.toml");
		// Nothing yet: the new name, for the settings to be written under.
		assert_eq!(settings_files(&dir, None), std::slice::from_ref(&new));
		assert!(old_name_notes(&dir, &settings_files(&dir, None)).is_empty());

		fs::write(&old, "port = 1\n[[services]]\nname = \"main\"\nurl = \"http://{host}\"\n").unwrap();
		fs::create_dir_all(dir.join("elamond.d")).unwrap();
		fs::write(dir.join("elamond.d/b.toml"), link("old b")).unwrap();
		fs::write(dir.join("elamond.d/a.toml"), link("old a")).unwrap();
		fs::write(dir.join("elamond.d/notes.txt"), "not settings").unwrap();
		let files = settings_files(&dir, None);
		assert_eq!(files, [old.clone(), dir.join("elamond.d/a.toml"), dir.join("elamond.d/b.toml")]);
		assert_eq!(old_name_notes(&dir, &files).len(), 2);
		let c = read_settings(&files).unwrap();
		assert_eq!((c.port, link_names(&c)), (1, vec!["main", "old a", "old b"]));

		// The new folder beside the old one: both, the new one last.
		fs::create_dir_all(dir.join("elamond.d")).unwrap();
		fs::write(dir.join("elamond.d/a.toml"), "port = 2\n".to_string() + &link("new a")).unwrap();
		let c = read_settings(&settings_files(&dir, None)).unwrap();
		assert_eq!((c.port, link_names(&c)), (2, vec!["main", "old a", "old b", "new a"]));

		// elamond.toml takes elamond.toml's place, which is then left unread.
		fs::write(&new, "port = 3\n").unwrap();
		let files = settings_files(&dir, None);
		assert_eq!(files[0], new);
		let notes = old_name_notes(&dir, &files);
		assert!(notes[0].contains("not read"), "{notes:?}");
		let c = read_settings(&files).unwrap();
		assert_eq!((c.port, link_names(&c)), (2, vec!["old a", "old b", "new a"]));

		// ELAMON_CONFIG names the main file outright; the drop-ins stay in the config directory.
		let named = dir.join("store.toml");
		assert_eq!(settings_files(&dir, Some(named.clone()))[..2], [named, dir.join("elamond.d/a.toml")]);
		fs::remove_dir_all(dir).unwrap();
	}

	#[test]
	fn ids_follow_content_not_position() {
		let a = |name: &str, cmd: &[&str]| ActionConfig {
			name: name.into(),
			command: cmd.iter().map(|s| s.to_string()).collect(),
			confirm: true,
			timeout: 60,
		};
		assert_eq!(a("Update", &["nh", "os", "switch"]).id(), a("Update", &["nh", "os", "switch"]).id());
		assert_ne!(a("Update", &["nh", "os", "switch"]).id(), a("Update", &["nh", "os", "boot"]).id());
		// The separator keeps ["ab"] and ["a", "b"] apart.
		assert_ne!(a("x", &["ab"]).id(), a("x", &["a", "b"]).id());
		assert_eq!(a("x", &["y"]).id().len(), 12);
	}

	#[test]
	fn names_lose_control_characters() {
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
		let p = Pairing { code: "123456".into(), expires: 100, attempts: 0, role: Role::Glance, wrong: BTreeMap::new() };
		assert!(p.usable(99));
		assert!(!p.usable(100));
		assert!(!Pairing { attempts: limit::PAIRING_ATTEMPTS, ..p }.usable(0));
	}

	#[test]
	fn wrong_codes_are_counted_per_address_and_in_all() {
		let mut p = Pairing { code: "123456".into(), expires: u64::MAX, attempts: 0, role: Role::Admin, wrong: BTreeMap::new() };
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
		// What it saves reads back, and a code from before 0.5 pairs an admin.
		let back: Pairing = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
		assert_eq!(back, p);
		let old: Pairing = serde_json::from_str(r#"{"code":"123456","expires":1,"attempts":2}"#).unwrap();
		assert_eq!((old.role, old.wrong.len()), (Role::Admin, 0));
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
	fn phones_paired_before_roles_are_admins() {
		let dir = scratch();
		fs::write(dir.join("clients.json"), r#"[{"id":"a","name":"Old","token_hash":"h","created":1}]"#).unwrap();
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
		let client = Client { id: "a".into(), name: "b".into(), token_hash: "c".into(), created: 1, role: Role::Admin, from: None };
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
