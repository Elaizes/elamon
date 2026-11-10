//! Files the service keeps: settings, its TLS identity, paired phones and the pending pairing code.
//!
//! Settings:  $XDG_CONFIG_HOME/elamon/elamond.toml (optional), then elamond.d/*.toml beside it
//! State:     $XDG_DATA_HOME/elamon/{id, cert.der, key.der, clients.json, pairing.json}

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::consts::{limit, protocol, timeout};

#[derive(Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
	/// Address to listen on; "::" takes both IPv4 and IPv6.
	pub listen: String,
	pub port: u16,
	/// Name shown in the app; the hostname when empty.
	pub name: String,
	/// Announce the service on the local network so the app finds it.
	pub mdns: bool,
	pub interval_ms: u64,
	pub history_minutes: u64,
	/// Let paired phones act on this computer: power, profiles, units, processes,
	/// media and the actions below. Off unless turned on here.
	pub control: bool,
	/// Web services shown in the app, beside those added from the app itself.
	pub services: Vec<ServiceConfig>,
	/// Commands the app can run, defined only here: a phone can't add its own.
	pub actions: Vec<ActionConfig>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct ServiceConfig {
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

impl ServiceConfig {
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
			listen: "::".into(),
			port: protocol::DEFAULT_PORT,
			name: String::new(),
			mdns: true,
			interval_ms: 1000,
			history_minutes: 10,
			control: false,
			services: Vec::new(),
			actions: Vec::new(),
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

/// ELAMON_CONFIG names the file outright (the NixOS module points it into the store).
pub fn config_path() -> PathBuf {
	std::env::var_os("ELAMON_CONFIG").map(PathBuf::from).unwrap_or_else(|| config_dir().join("elamond.toml"))
}

/// Drop-ins, always in the user's own config directory: where NixOS or Home Manager make
/// elamond.toml a read-only file in the store, these stay the user's to write.
pub fn dropin_dir() -> PathBuf {
	config_dir().join("elamond.d")
}

/// elamond.toml first, then elamond.d/*.toml by name.
pub fn config_files() -> Vec<PathBuf> {
	let mut dropins: Vec<PathBuf> = fs::read_dir(dropin_dir())
		.map(|d| d.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect())
		.unwrap_or_default();
	dropins.sort();
	std::iter::once(config_path()).chain(dropins).collect()
}

/// Settings from elamond.toml and the drop-ins. A later file adds to the lists (services,
/// actions) and replaces anything else it sets.
pub fn load_config() -> Result<Config, String> {
	let mut merged = toml::Table::new();
	for path in config_files() {
		let text = match fs::read_to_string(&path) {
			Ok(t) => t,
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
			Err(e) => return Err(format!("{}: {e}", path.display())),
		};
		// Each file on its own first, so a mistake names the file it is in.
		toml::from_str::<Config>(&text).map_err(|e| format!("{}: {e}", path.display()))?;
		let table: toml::Table = text.parse().map_err(|e| format!("{}: {e}", path.display()))?;
		merge_toml(&mut merged, table);
	}
	toml::Value::Table(merged).try_into().map_err(|e| format!("settings: {e}"))
}

fn merge_toml(into: &mut toml::Table, from: toml::Table) {
	for (key, value) in from {
		match (into.get_mut(&key), value) {
			(Some(toml::Value::Array(a)), toml::Value::Array(b)) => a.extend(b),
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

/// Writes a file only its owner can read, replacing it atomically. The temporary file's name
/// is this writer's own, so the service and a command run beside it can't trip over each other.
pub fn write_private(path: &Path, data: &[u8]) -> io::Result<()> {
	let dir = path.parent().expect("state files live in a directory");
	fs::create_dir_all(dir)?;
	fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
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

pub struct Identity {
	pub id: String,
	pub cert_der: Vec<u8>,
	pub key_der: Vec<u8>,
	/// SHA-256 of the certificate, the thing the app pins.
	pub fingerprint: String,
}

/// A file's contents, or `None` when it doesn't exist.
fn read_if_there(path: &Path) -> Result<Option<Vec<u8>>, String> {
	match fs::read(path) {
		Ok(b) => Ok(Some(b)),
		Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
		Err(e) => Err(format!("can't read {}: {e}", path.display())),
	}
}

impl Identity {
	/// Loads the service's identity, creating it on first run. Paired phones trust exactly this
	/// identity, so it is only made anew when its files don't exist: one that can't be read
	/// stops the service with the reason instead of silently unpairing every phone.
	pub fn load_or_create() -> Result<Identity, String> {
		let dir = data_dir();
		let id_path = dir.join("id");
		let cert_path = dir.join("cert.der");
		let key_path = dir.join("key.der");
		let saved = |path: &Path, data: &[u8]| write_private(path, data).map_err(|e| format!("can't write {}: {e}", path.display()));

		let id = match read_if_there(&id_path)? {
			Some(b) => {
				let id = String::from_utf8_lossy(&b).trim().to_string();
				if id.is_empty() {
					return Err(format!("{} is empty; remove it to make a new id, then pair the phones again", id_path.display()));
				}
				id
			}
			None => {
				let id = data_encoding::HEXLOWER.encode(&random_bytes::<{ protocol::ELAMOND_ID_BYTES }>());
				saved(&id_path, id.as_bytes())?;
				id
			}
		};

		let (cert_der, key_der) = match (read_if_there(&cert_path)?, read_if_there(&key_path)?) {
			(Some(c), Some(k)) if !c.is_empty() && !k.is_empty() => (c, k),
			(None, None) => {
				// The name inside doesn't matter: the app trusts this exact certificate, not a name.
				let ck = rcgen::generate_simple_self_signed(vec!["elamon".to_string()])
					.map_err(|e| format!("can't make a certificate: {e}"))?;
				let cert = ck.cert.der().to_vec();
				let key = ck.signing_key.serialize_der();
				saved(&key_path, &key)?;
				saved(&cert_path, &cert)?;
				(cert, key)
			}
			_ => {
				return Err(format!(
					"the TLS identity in {} is incomplete: cert.der and key.der go together; remove both to make a new one, then pair the phones again",
					dir.display()
				));
			}
		};
		let fingerprint = sha256_hex(&cert_der);
		Ok(Identity { id, cert_der, key_der, fingerprint })
	}
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
}

fn clients_path() -> PathBuf {
	data_dir().join("clients.json")
}

/// The paired phones; none when the file doesn't exist yet. A file that can't be read or
/// doesn't parse is an error, never an empty list: that would unpair every phone.
pub fn load_clients() -> io::Result<Vec<Client>> {
	Ok(read_json(&clients_path())?.unwrap_or_default())
}

pub fn save_clients(clients: &[Client]) -> io::Result<()> {
	write_json::<Vec<Client>>(&clients_path(), clients)
}

/// What clients.json looks like on disk, to notice that `pair` or `revoke` changed it. Every
/// write is a new file, so its inode changes even when the clock doesn't.
pub fn clients_stamp() -> Option<(SystemTime, u64, u64)> {
	let m = fs::metadata(clients_path()).ok()?;
	Some((m.modified().ok()?, m.ino(), m.len()))
}

/// Adds a phone and returns its token.
pub fn add_client(name: &str) -> io::Result<(Client, String)> {
	let token = data_encoding::BASE64URL_NOPAD.encode(&random_bytes::<{ protocol::TOKEN_BYTES }>());
	let client = Client {
		id: data_encoding::HEXLOWER.encode(&random_bytes::<{ protocol::CLIENT_ID_BYTES }>()),
		name: sanitize_name(name),
		token_hash: sha256_hex(token.as_bytes()),
		created: now(),
	};
	let mut clients = match load_clients() {
		Ok(c) => c,
		// The damaged file is set aside by the write below; the phones in it pair again.
		Err(e) if e.kind() == io::ErrorKind::InvalidData => Vec::new(),
		Err(e) => return Err(e),
	};
	clients.push(client.clone());
	save_clients(&clients)?;
	Ok((client, token))
}

/// Phone-supplied names end up in a terminal: no control characters, sane length.
pub fn sanitize_name(name: &str) -> String {
	let clean: String = name.chars().filter(|c| !c.is_control()).take(limit::NAME_MAX).collect();
	let clean = clean.trim();
	if clean.is_empty() { "Phone".into() } else { clean.to_string() }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Pairing {
	pub code: String,
	pub expires: u64,
	pub attempts: u32,
}

fn pairing_path() -> PathBuf {
	data_dir().join("pairing.json")
}

impl Pairing {
	/// Still good at Unix time `now`; the service and `elamond pair` both ask this.
	pub fn usable(&self, now: u64) -> bool {
		now < self.expires && self.attempts < limit::PAIRING_ATTEMPTS
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

/// A new code, uniformly random.
pub fn new_pairing() -> io::Result<Pairing> {
	let span = 10u32.pow(protocol::PAIRING_DIGITS);
	let code = loop {
		let n = u32::from_le_bytes(random_bytes::<4>());
		// Rejecting the top sliver keeps every code equally likely.
		if n < u32::MAX - u32::MAX % span {
			break format!("{:0width$}", n % span, width = protocol::PAIRING_DIGITS as usize);
		}
	};
	let p = Pairing { code, expires: now() + timeout::PAIRING_TTL_SECS, attempts: 0 };
	save_pairing(&p)?;
	Ok(p)
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
		merge_toml(&mut m, table("control = true\n[[actions]]\nname = \"b\"\ncommand = [\"false\"]\n[[services]]\nname = \"s\"\nurl = \"http://{host}\"\n"));
		let c: Config = toml::Value::Table(m).try_into().unwrap();
		assert!(c.control);
		assert_eq!(c.port, 1);
		assert_eq!(c.actions.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["a", "b"]);
		assert_eq!(c.services.len(), 1);
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
		let p = Pairing { code: "123456".into(), expires: 100, attempts: 0 };
		assert!(p.usable(99));
		assert!(!p.usable(100));
		assert!(!Pairing { attempts: limit::PAIRING_ATTEMPTS, ..p }.usable(0));
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
		let client = Client { id: "a".into(), name: "b".into(), token_hash: "c".into(), created: 1 };
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
