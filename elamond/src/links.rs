//! Links to the web pages this computer hosts: what the app opens, whether they answer, their units.
//!
//! Links come from elamond.toml (fixed) and from the app (links.json in the data directory,
//! editable from any paired phone). Before 0.5 they were called services, and the file
//! services.json, which is still read while links.json doesn't exist.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::consts::{limit, timeout};
use crate::errors::ErrorCode;
use crate::store;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LinkDef {
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
pub struct LinkStatus {
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

/// The links added from the app, in the data directory.
const FILE: &str = "links.json";
/// Its name before 0.5.
const OLD_FILE: &str = "services.json";

/// The links added from the app, kept in the data directory `dir`; none when there are none
/// yet. A file that doesn't parse is an error, and stays on disk until the next save sets it
/// aside. links.json, or services.json while there is no links.json yet: the first save
/// writes links.json and leaves the old file as it was.
pub fn load_in(dir: &Path) -> io::Result<Vec<LinkDef>> {
	let list = match store::read_json(&dir.join(FILE))? {
		Some(list) => Some(list),
		None => store::read_json(&dir.join(OLD_FILE))?,
	};
	Ok(list.unwrap_or_default())
}

pub fn save_in(dir: &Path, list: &[LinkDef]) -> io::Result<()> {
	store::write_json::<Vec<LinkDef>>(&dir.join(FILE), list)
}

/// Every link, the configured ones first.
pub fn all(config: &store::Config, app: &[LinkDef]) -> Vec<(LinkDef, bool)> {
	let fixed = config.links.iter().map(|s| {
		(
			LinkDef { id: format!("config-{}", s.id()), name: s.name.clone(), url: s.url.clone(), unit: s.unit.clone(), scope: s.scope.clone() },
			false,
		)
	});
	fixed.chain(app.iter().cloned().map(|s| (s, true))).collect()
}

/// The port a link's URL points at: explicit, or the scheme's.
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
	let t = timeout::PROBE;
	TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), t).is_ok()
		|| TcpStream::connect_timeout(&SocketAddr::from((Ipv6Addr::LOCALHOST, port)), t).is_ok()
}

/// Statuses without the units' state, which needs D-Bus and is filled in by the caller.
pub fn statuses(config: &store::Config, app: &[LinkDef]) -> Vec<LinkStatus> {
	let listening = listening_sockets();
	all(config, app)
		.into_iter()
		.map(|(s, editable)| {
			let port = port_of(&s.url);
			let socks: Vec<&(u16, bool, u64)> = listening.iter().filter(|l| Some(l.0) == port).collect();
			LinkStatus {
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

/// A link as the app sends it, checked before it's stored. Control characters are refused
/// everywhere: these end up in the log and on other phones.
pub fn validate(name: &str, url: &str, unit: Option<&str>, scope: Option<&str>) -> Result<(), ErrorCode> {
	if name.trim().is_empty() || name.chars().count() > limit::NAME_MAX || name.chars().any(char::is_control) {
		return Err(ErrorCode::BadName);
	}
	if url.trim().is_empty() || url.len() > limit::URL_MAX || url.chars().any(char::is_control) {
		return Err(ErrorCode::BadUrl);
	}
	if let Some(u) = unit
		&& (u.is_empty() || u.len() > limit::UNIT_MAX || !u.chars().all(|c| c.is_ascii_alphanumeric() || "@._-:\\".contains(c)))
	{
		return Err(ErrorCode::BadUnit);
	}
	if !matches!(scope, None | Some("user") | Some("system")) {
		return Err(ErrorCode::BadScope);
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

	#[test]
	fn what_the_app_may_store() {
		assert_eq!(validate("Syncthing", "http://{host}:8384", Some("syncthing.service"), Some("user")), Ok(()));
		assert_eq!(validate("Grafana", "https://{host}/", Some("getty@tty1.service"), None), Ok(()));
		assert_eq!(validate(" ", "http://x", None, None), Err(ErrorCode::BadName));
		assert_eq!(validate(&"n".repeat(65), "http://x", None, None), Err(ErrorCode::BadName));
		assert_eq!(validate("a\u{1b}[31m", "http://x", None, None), Err(ErrorCode::BadName));
		assert_eq!(validate("a", "http://x\n", None, None), Err(ErrorCode::BadUrl));
		assert_eq!(validate("a", &"u".repeat(513), None, None), Err(ErrorCode::BadUrl));
		assert_eq!(validate("a", "http://x", Some("a b.service"), None), Err(ErrorCode::BadUnit));
		assert_eq!(validate("a", "http://x", Some("a\tb"), None), Err(ErrorCode::BadUnit));
		assert_eq!(validate("a", "http://x", Some(""), None), Err(ErrorCode::BadUnit));
		assert_eq!(validate("a", "http://x", None, Some("global")), Err(ErrorCode::BadScope));
	}

	#[test]
	fn configured_links_first_with_ids_from_content() {
		let config = store::Config {
			links: vec![store::LinkConfig { name: "Web".into(), url: "http://{host}".into(), unit: None, scope: None }],
			..Default::default()
		};
		let app = [LinkDef { id: "a1b2".into(), name: "Mine".into(), url: "http://{host}:81".into(), unit: None, scope: None }];
		let list = all(&config, &app);
		assert_eq!(list.len(), 2);
		assert_eq!(list[0].0.id, format!("config-{}", config.links[0].id()));
		assert_eq!(list[0].0.id.len(), "config-".len() + 12);
		assert!(!list[0].1);
		assert_eq!((list[1].0.id.as_str(), list[1].1), ("a1b2", true));
	}

	#[test]
	fn links_from_before_0_5_are_read_until_saved_anew() {
		let dir = std::env::temp_dir().join(format!("elamond-links-{}", data_encoding::HEXLOWER.encode(&store::random_bytes::<6>())));
		fs::create_dir_all(&dir).unwrap();
		let def = |id: &str| LinkDef { id: id.into(), name: id.into(), url: "http://{host}:81".into(), unit: None, scope: None };
		let ids = |list: Vec<LinkDef>| list.into_iter().map(|l| l.id).collect::<Vec<_>>();
		assert!(load_in(&dir).unwrap().is_empty());

		fs::write(dir.join(OLD_FILE), serde_json::to_vec(&[def("old")]).unwrap()).unwrap();
		assert_eq!(ids(load_in(&dir).unwrap()), ["old"]);

		save_in(&dir, &[def("old"), def("new")]).unwrap();
		assert_eq!(ids(load_in(&dir).unwrap()), ["old", "new"]);
		// The old file is left for an older elamond, and no longer read.
		assert!(dir.join(OLD_FILE).exists());
		save_in(&dir, &[]).unwrap();
		assert!(load_in(&dir).unwrap().is_empty());
		fs::remove_dir_all(dir).unwrap();
	}
}
