//! Web services this computer hosts: links for the app, whether they answer, their units.
//!
//! Services come from elamond.toml (fixed) and from the app (services.json in the data
//! directory, editable from any paired phone).

use std::collections::HashMap;
use std::fs;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::store;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ServiceDef {
	pub id: String,
	pub name: String,
	pub url: String,
	#[serde(default)]
	pub unit: Option<String>,
	/// "user" or "system".
	#[serde(default)]
	pub scope: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ServiceStatus {
	pub id: String,
	pub name: String,
	pub url: String,
	pub unit: Option<String>,
	pub scope: String,
	/// Added from the app, so the app may change or remove it.
	pub editable: bool,
	pub port: Option<u16>,
	/// Something accepts connections on the port, seen from the computer itself.
	pub up: Option<bool>,
	/// Listening on loopback only: other devices can't open it.
	pub local_only: bool,
	pub unit_state: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Listener {
	pub port: u16,
	pub local_only: bool,
	pub process: Option<String>,
}

fn path() -> std::path::PathBuf {
	store::data_dir().join("services.json")
}

pub fn load() -> Vec<ServiceDef> {
	fs::read(path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save(list: &[ServiceDef]) -> std::io::Result<()> {
	store::write_private(&path(), &serde_json::to_vec_pretty(list).map_err(std::io::Error::other)?)
}

/// Every service, the configured ones first.
pub fn all(config: &store::Config, app: &[ServiceDef]) -> Vec<(ServiceDef, bool)> {
	let fixed = config.services.iter().map(|s| {
		(
			ServiceDef { id: format!("config-{}", s.id()), name: s.name.clone(), url: s.url.clone(), unit: s.unit.clone(), scope: s.scope.clone() },
			false,
		)
	});
	fixed.chain(app.iter().cloned().map(|s| (s, true))).collect()
}

/// The port a service URL points at: explicit, or the scheme's.
pub fn port_of(url: &str) -> Option<u16> {
	let (scheme, rest) = match url.split_once("://") {
		Some((s, r)) => (s.to_ascii_lowercase(), r),
		None => ("http".to_string(), url),
	};
	let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
	let authority = authority.rsplit('@').next().unwrap_or(authority);
	let port = if let Some(end) = authority.find(']') {
		authority[end + 1..].strip_prefix(':').map(str::to_string)
	} else {
		authority.rsplit_once(':').map(|(_, p)| p.to_string())
	};
	match port {
		Some(p) => p.parse().ok(),
		None => match scheme.as_str() {
			"https" => Some(443),
			"http" => Some(80),
			_ => None,
		},
	}
}

/// Whether anything accepts connections on this port of the computer itself.
pub fn probe(port: u16) -> bool {
	let t = Duration::from_millis(250);
	TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), t).is_ok()
		|| TcpStream::connect_timeout(&SocketAddr::from((Ipv6Addr::LOCALHOST, port)), t).is_ok()
}

/// Statuses without the units' state, which needs D-Bus and is filled in by the caller.
pub fn statuses(config: &store::Config, app: &[ServiceDef]) -> Vec<ServiceStatus> {
	let listening = listening_sockets();
	all(config, app)
		.into_iter()
		.map(|(s, editable)| {
			let port = port_of(&s.url);
			let socks: Vec<&(u16, bool, u64)> = listening.iter().filter(|l| Some(l.0) == port).collect();
			ServiceStatus {
				up: port.map(|p| !socks.is_empty() || probe(p)),
				local_only: !socks.is_empty() && socks.iter().all(|l| l.1),
				port,
				scope: s.scope.clone().unwrap_or_else(|| "user".into()),
				id: s.id,
				name: s.name,
				url: s.url,
				unit: s.unit,
				editable,
				unit_state: None,
			}
		})
		.collect()
}

/// Listening TCP sockets: (port, bound to loopback, socket inode).
fn listening_sockets() -> Vec<(u16, bool, u64)> {
	let mut out = Vec::new();
	for (file, v6) in [("/proc/net/tcp", false), ("/proc/net/tcp6", true)] {
		let Ok(text) = fs::read_to_string(file) else { continue };
		for line in text.lines().skip(1) {
			let f: Vec<&str> = line.split_whitespace().collect();
			// State 0A is LISTEN.
			if f.len() < 10 || f[3] != "0A" {
				continue;
			}
			let Some((addr, port)) = f[1].split_once(':') else { continue };
			let Ok(port) = u16::from_str_radix(port, 16) else { continue };
			let loopback = if v6 { v6_loopback(addr) } else { v4_loopback(addr) };
			out.push((port, loopback, f[9].parse().unwrap_or(0)));
		}
	}
	out
}

/// The kernel prints IPv4 addresses as one little-endian 32-bit word.
fn v4_loopback(hex: &str) -> bool {
	u32::from_str_radix(hex, 16).map(|w| Ipv4Addr::from(w.swap_bytes()).is_loopback()).unwrap_or(false)
}

/// IPv6 as four little-endian words; also catches v4-mapped loopback.
fn v6_loopback(hex: &str) -> bool {
	if hex.len() != 32 {
		return false;
	}
	let mut bytes = [0u8; 16];
	for w in 0..4 {
		let Ok(word) = u32::from_str_radix(&hex[w * 8..w * 8 + 8], 16) else { return false };
		bytes[w * 4..w * 4 + 4].copy_from_slice(&word.to_le_bytes());
	}
	let a = Ipv6Addr::from(bytes);
	a.is_loopback() || a.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
}

/// Ports something listens on, with the program when it's this user's.
pub fn detect() -> Vec<Listener> {
	let socks = listening_sockets();
	let owners = socket_owners();
	let mut by_port: HashMap<u16, Listener> = HashMap::new();
	for (port, loopback, inode) in socks {
		let process = owners.get(&inode).cloned();
		let e = by_port.entry(port).or_insert(Listener { port, local_only: true, process: None });
		e.local_only &= loopback;
		if e.process.is_none() {
			e.process = process;
		}
	}
	let mut v: Vec<Listener> = by_port.into_values().collect();
	v.sort_by_key(|l| l.port);
	v
}

/// Socket inode to program name, for the processes whose descriptors this user can read.
fn socket_owners() -> HashMap<u64, String> {
	let mut map = HashMap::new();
	let Ok(procs) = fs::read_dir("/proc") else { return map };
	for p in procs.flatten() {
		let pid = p.file_name();
		if !pid.to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
			continue;
		}
		let Ok(fds) = fs::read_dir(p.path().join("fd")) else { continue };
		let mut name = None;
		for fd in fds.flatten() {
			let Ok(target) = fs::read_link(fd.path()) else { continue };
			let t = target.to_string_lossy();
			let Some(inode) = t.strip_prefix("socket:[").and_then(|s| s.strip_suffix(']')) else { continue };
			let Ok(inode) = inode.parse::<u64>() else { continue };
			let name = name.get_or_insert_with(|| {
				let comm = fs::read_to_string(p.path().join("comm")).unwrap_or_default().trim().to_string();
				let argv0 = fs::read(p.path().join("cmdline"))
					.ok()
					.and_then(|c| c.split(|b| *b == 0).next().map(|a| String::from_utf8_lossy(a).into_owned()));
				crate::collect::process_name(&comm, argv0.map(std::ffi::OsString::from).as_ref())
			});
			map.insert(inode, name.clone());
		}
	}
	map
}

/// A service as the app sends it, checked before it's stored.
pub fn validate(name: &str, url: &str, unit: Option<&str>, scope: Option<&str>) -> Result<(), &'static str> {
	if name.trim().is_empty() || name.chars().count() > 64 {
		return Err("bad_name");
	}
	if url.trim().is_empty() || url.len() > 512 || url.chars().any(char::is_control) {
		return Err("bad_url");
	}
	if let Some(u) = unit
		&& (u.is_empty() || u.len() > 256 || !u.chars().all(|c| c.is_ascii_alphanumeric() || "@._-:\\".contains(c)))
	{
		return Err("bad_unit");
	}
	if !matches!(scope, None | Some("user") | Some("system")) {
		return Err("bad_scope");
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn ports() {
		assert_eq!(port_of("http://{host}:8384/"), Some(8384));
		assert_eq!(port_of("https://{host}/admin"), Some(443));
		assert_eq!(port_of("{host}:3000"), Some(3000));
		assert_eq!(port_of("http://[::1]:9090/x"), Some(9090));
		assert_eq!(port_of("http://user:pw@{host}:81"), Some(81));
		assert_eq!(port_of("http://{host}"), Some(80));
	}

	#[test]
	fn loopback() {
		assert!(v4_loopback("0100007F"));
		assert!(!v4_loopback("00000000"));
		assert!(v6_loopback("00000000000000000000000001000000"));
		assert!(!v6_loopback("00000000000000000000000000000000"));
	}
}
