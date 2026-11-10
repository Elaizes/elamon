//! Files the service keeps: settings, its TLS identity, paired phones and the pending pairing code.
//!
//! Settings:  $XDG_CONFIG_HOME/elamon/elamond.toml (optional), then elamond.d/*.toml beside it
//! State:     $XDG_DATA_HOME/elamon/{id, cert.der, key.der, clients.json, pairing.json}

use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const DEFAULT_PORT: u16 = 47823;
/// A pairing code is good for this long after `elamond pair` prints it.
pub const PAIRING_TTL: u64 = 300;
/// Wrong guesses before a code is thrown away.
pub const PAIRING_ATTEMPTS: u32 = 5;

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
	sha256_hex(parts.join("\0").as_bytes())[..12].to_string()
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
	60
}

impl Default for Config {
	fn default() -> Self {
		Config {
			listen: "::".into(),
			port: DEFAULT_PORT,
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

/// Writes a file only its owner can read, replacing it atomically.
pub fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
	let dir = path.parent().expect("state files live in a directory");
	fs::create_dir_all(dir)?;
	fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
	let tmp = path.with_extension("tmp");
	let mut f = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
	f.write_all(data)?;
	f.sync_all()?;
	fs::rename(&tmp, path)
}

pub struct Identity {
	pub id: String,
	pub cert_der: Vec<u8>,
	pub key_der: Vec<u8>,
	/// SHA-256 of the certificate, the thing the app pins.
	pub fingerprint: String,
}

impl Identity {
	/// Loads the service's identity, creating it on first run.
	pub fn load_or_create() -> std::io::Result<Identity> {
		let dir = data_dir();
		let id_path = dir.join("id");
		let cert_path = dir.join("cert.der");
		let key_path = dir.join("key.der");

		let id = match fs::read_to_string(&id_path) {
			Ok(s) if !s.trim().is_empty() => s.trim().to_string(),
			_ => {
				let id = data_encoding::HEXLOWER.encode(&random_bytes::<16>());
				write_private(&id_path, id.as_bytes())?;
				id
			}
		};

		let (cert_der, key_der) = match (fs::read(&cert_path), fs::read(&key_path)) {
			(Ok(c), Ok(k)) if !c.is_empty() && !k.is_empty() => (c, k),
			_ => {
				// The name inside doesn't matter: the app trusts this exact certificate, not a name.
				let ck = rcgen::generate_simple_self_signed(vec!["elamon".to_string()])
					.map_err(|e| std::io::Error::other(e.to_string()))?;
				let cert = ck.cert.der().to_vec();
				let key = ck.signing_key.serialize_der();
				write_private(&key_path, &key)?;
				write_private(&cert_path, &cert)?;
				(cert, key)
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

pub fn load_clients() -> Vec<Client> {
	fs::read(data_dir().join("clients.json"))
		.ok()
		.and_then(|b| serde_json::from_slice(&b).ok())
		.unwrap_or_default()
}

pub fn save_clients(clients: &[Client]) -> std::io::Result<()> {
	let json = serde_json::to_vec_pretty(clients).map_err(std::io::Error::other)?;
	write_private(&data_dir().join("clients.json"), &json)
}

pub fn clients_mtime() -> Option<SystemTime> {
	fs::metadata(data_dir().join("clients.json")).and_then(|m| m.modified()).ok()
}

/// Adds a phone and returns its token.
pub fn add_client(name: &str) -> std::io::Result<(Client, String)> {
	let token = data_encoding::BASE64URL_NOPAD.encode(&random_bytes::<32>());
	let client = Client {
		id: data_encoding::HEXLOWER.encode(&random_bytes::<4>()),
		name: sanitize_name(name),
		token_hash: sha256_hex(token.as_bytes()),
		created: now(),
	};
	let mut clients = load_clients();
	clients.push(client.clone());
	save_clients(&clients)?;
	Ok((client, token))
}

/// Phone-supplied names end up in a terminal: no control characters, sane length.
pub fn sanitize_name(name: &str) -> String {
	let clean: String = name.chars().filter(|c| !c.is_control()).take(64).collect();
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

pub fn load_pairing() -> Option<Pairing> {
	let p: Pairing = serde_json::from_slice(&fs::read(pairing_path()).ok()?).ok()?;
	(p.expires > now() && p.attempts < PAIRING_ATTEMPTS).then_some(p)
}

pub fn save_pairing(p: &Pairing) -> std::io::Result<()> {
	write_private(&pairing_path(), &serde_json::to_vec(p).map_err(std::io::Error::other)?)
}

pub fn clear_pairing() {
	let _ = fs::remove_file(pairing_path());
}

/// A new six-digit code, uniformly random.
pub fn new_pairing() -> std::io::Result<Pairing> {
	let code = loop {
		let n = u32::from_le_bytes(random_bytes::<4>());
		// Rejecting the top sliver keeps every code equally likely.
		if n < u32::MAX - u32::MAX % 1_000_000 {
			break format!("{:06}", n % 1_000_000);
		}
	};
	let p = Pairing { code, expires: now() + PAIRING_TTL, attempts: 0 };
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
}
