//! Links to the web pages this computer hosts: what the app opens, whether they answer, their units.
//!
//! Links come from elamond.toml (fixed) and from the app (links.json in the data directory,
//! editable from any paired phone).

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
pub const FILE: &str = "links.json";

/// The links added from the app, kept in the data directory `dir`; none when there are none
/// yet. A file that doesn't parse is an error, and stays on disk until the next save sets it
/// aside.
pub fn load_in(dir: &Path) -> io::Result<Vec<LinkDef>> {
	Ok(store::read_json(&dir.join(FILE))?.unwrap_or_default())
}

pub fn save_in(dir: &Path, list: &[LinkDef]) -> io::Result<()> {
	store::write_json::<Vec<LinkDef>>(&dir.join(FILE), list)
}

/// Every link the app may open, the configured ones first; one whose URL isn't a web page's
/// (`web_url`) is left out, wherever it came from.
pub fn all(config: &store::Config, app: &[LinkDef]) -> Vec<(LinkDef, bool)> {
	let fixed = config.links.iter().map(|s| {
		(
			LinkDef { id: format!("config-{}", s.id()), name: s.name.clone(), url: s.url.clone(), unit: s.unit.clone(), scope: s.scope.clone() },
			false,
		)
	});
	fixed.chain(app.iter().cloned().map(|s| (s, true))).filter(|(l, _)| web_url(&l.url)).collect()
}

/// Whether the app may open `url`: an http:// or https:// one, or a bare host and port, which the
/// app opens as http. Another scheme (javascript:, intent:, file:, data:…) never.
pub fn web_url(url: &str) -> bool {
	let u = url.trim();
	let scheme = u.bytes().take_while(|b| b.is_ascii_alphanumeric() || b"+.-".contains(b)).count();
	// "{host}:8080", "192.0.2.10:80": no scheme at all.
	if !u.starts_with(|c: char| c.is_ascii_alphabetic()) || !u[scheme..].starts_with(':') {
		return true;
	}
	let rest = &u[scheme + 1..];
	if rest.starts_with("//") {
		return matches!(u[..scheme].to_ascii_lowercase().as_str(), "http" | "https");
	}
	// "localhost:8080/admin": a host and its port, not a scheme.
	let port = rest.split(['/', '?', '#']).next().unwrap_or("");
	!port.is_empty() && port.bytes().all(|b| b.is_ascii_digit())
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

/// What to tell about `[[links]]` in the settings that won't work as written.
pub fn settings_notes(config: &store::Config) -> Vec<String> {
	config
		.links
		.iter()
		.filter_map(|l| {
			if !web_url(&l.url) {
				return Some(format!("[[links]] \"{}\" is left out: its url isn't an http:// or https:// one", l.name));
			}
			let unit = l.unit.as_deref()?;
			let why = unit_shape(unit).err()?;
			Some(format!("[[links]] \"{}\": its unit {unit} {why}; the app can't start or stop it", l.name))
		})
		.collect()
}

/// A link as the app sends it, checked before it's stored. Control characters are refused
/// everywhere: these end up in the log and on other phones. Whether the settings allow its unit
/// is `unit_refusal`'s to say.
pub fn validate(name: &str, url: &str, unit: Option<&str>, scope: Option<&str>) -> Result<(), ErrorCode> {
	if name.trim().is_empty() || name.chars().count() > limit::NAME_MAX || name.chars().any(store::unseen) {
		return Err(ErrorCode::BadName);
	}
	if url.trim().is_empty() || url.len() > limit::URL_MAX || url.chars().any(char::is_control) || !web_url(url) {
		return Err(ErrorCode::BadUrl);
	}
	if unit.is_some_and(|u| unit_shape(u).is_err()) {
		return Err(ErrorCode::BadUnit);
	}
	if !matches!(scope, None | Some("user") | Some("system")) {
		return Err(ErrorCode::BadScope);
	}
	Ok(())
}

/// Whether `name` is a unit a phone could ever be let start, stop and restart: a service, its name
/// as systemd writes one, never one the service itself runs in. A template's instance (`@`) or an
/// escaped name (`\`) passes only as itself: `unit_refusal` wants the exact name in the settings.
pub fn unit_shape(name: &str) -> Result<(), &'static str> {
	let chars_ok = name.chars().all(|c| c.is_ascii_alphanumeric() || "@._-:\\".contains(c));
	if name.is_empty() || name.len() > limit::UNIT_MAX || !chars_ok {
		return Err("isn't a unit name");
	}
	if !name.ends_with(".service") || name == ".service" {
		return Err("isn't a service unit (NAME.service)");
	}
	if name.starts_with('-') {
		return Err("starts with \"-\"");
	}
	if own_units().iter().any(|u| u == name) {
		return Err("is the Elamon service's own unit");
	}
	Ok(())
}

/// The units the service runs as: elamond.service, and the unit its cgroup says it is in, should
/// it be started under another name.
fn own_units() -> &'static [String] {
	static OWN: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
	OWN.get_or_init(|| {
		let mut own = vec!["elamond.service".to_string()];
		let cgroup = fs::read_to_string("/proc/self/cgroup").unwrap_or_default();
		for line in cgroup.lines() {
			if let Some(unit) = line.rsplit('/').next().filter(|u| u.ends_with(".service"))
				&& !own.iter().any(|o| o == unit)
			{
				own.push(unit.to_string());
			}
		}
		own
	})
}

/// Why the unit `unit` in `scope` ("user" when none) may not be started, stopped or restarted from
/// the app, if it may not: only a service unit the settings name, on a `[[links]]` entry or in
/// `units`, in that scope, ever may. A link added from the app names any unit it likes; this is
/// asked when it is added and again whenever its unit is to be used.
pub fn unit_refusal(config: &store::Config, unit: &str, scope: Option<&str>) -> Option<String> {
	if let Err(why) = unit_shape(unit) {
		return Some(format!("{unit} {why}"));
	}
	let scope = scope.unwrap_or("user");
	let on_a_link = config.links.iter().any(|l| l.unit.as_deref() == Some(unit) && l.scope.as_deref().unwrap_or("user") == scope);
	let listed = config.units.iter().any(|u| u.name == unit && u.scope == scope);
	(!on_a_link && !listed).then(|| format!("the settings don't name {unit} ({scope}) on a [[links]] entry or in units"))
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
		for unit in ["-.slice", "-.service", "user.slice", "x.socket", ".service", "elamond.service"] {
			assert_eq!(validate("a", "http://x", Some(unit), None), Err(ErrorCode::BadUnit), "{unit}");
		}
		assert_eq!(validate(" ", "http://x", None, None), Err(ErrorCode::BadName));
		assert_eq!(validate(&"n".repeat(65), "http://x", None, None), Err(ErrorCode::BadName));
		assert_eq!(validate("a\u{1b}[31m", "http://x", None, None), Err(ErrorCode::BadName));
		assert_eq!(validate("Web\u{202e}", "http://x", None, None), Err(ErrorCode::BadName));
		assert_eq!(validate("a", "http://x\n", None, None), Err(ErrorCode::BadUrl));
		for bad in ["javascript:alert(1)", "intent://x#Intent;end", "file:///etc/passwd", "data:text/html,x", "ftp://{host}", "http:x", "mailto:a@b"] {
			assert_eq!(validate("a", bad, None, None), Err(ErrorCode::BadUrl), "{bad}");
		}
		for good in ["HTTPS://{host}/", "{host}:3000", "localhost:8080/admin", "pc.lan:81", "192.0.2.10:8096", "[::1]:9090"] {
			assert_eq!(validate("a", good, None, None), Ok(()), "{good}");
		}
		assert_eq!(validate("a", &"u".repeat(513), None, None), Err(ErrorCode::BadUrl));
		assert_eq!(validate("a", "http://x", Some("a b.service"), None), Err(ErrorCode::BadUnit));
		assert_eq!(validate("a", "http://x", Some("a\tb"), None), Err(ErrorCode::BadUnit));
		assert_eq!(validate("a", "http://x", Some(""), None), Err(ErrorCode::BadUnit));
		assert_eq!(validate("a", "http://x", None, Some("global")), Err(ErrorCode::BadScope));
	}

	#[test]
	fn only_units_the_settings_name_may_be_used() {
		let link = |unit: &str, scope: Option<&str>| store::LinkConfig {
			name: "x".into(),
			url: "http://{host}".into(),
			unit: Some(unit.into()),
			scope: scope.map(str::to_string),
		};
		let config: store::Config = toml::from_str(
			"units = [\"syncthing.service\", { name = \"jellyfin.service\", scope = \"system\" }, \"podman-kube@a.service\"]",
		)
		.unwrap();
		let config = store::Config { links: vec![link("nginx.service", Some("system")), link("mine.service", None)], ..config };
		let allowed = |unit: &str, scope: Option<&str>| unit_refusal(&config, unit, scope).is_none();
		assert!(allowed("syncthing.service", None) && allowed("syncthing.service", Some("user")));
		assert!(allowed("jellyfin.service", Some("system")) && allowed("nginx.service", Some("system")) && allowed("mine.service", None));
		// Each in its own scope only.
		assert!(!allowed("jellyfin.service", None) && !allowed("syncthing.service", Some("system")) && !allowed("nginx.service", None));
		// A template's instance only as the settings name it; never anything else, whatever it is.
		assert!(allowed("podman-kube@a.service", None));
		for unit in ["podman-kube@https:--evil.service", "-.slice", "user@1000.service", "getty@tty1.service", "x.service"] {
			assert!(!allowed(unit, None) && !allowed(unit, Some("system")), "{unit}");
		}
		assert!(unit_refusal(&config, "-.slice", None).unwrap().contains("isn't a service unit"));
		// The service's own unit is refused even when named.
		let own = store::Config { links: vec![link("elamond.service", None)], ..Default::default() };
		assert!(unit_refusal(&own, "elamond.service", None).unwrap().contains("own unit"));
		// Settings that list what could never be allowed don't load.
		for bad in ["units = [\"-.slice\"]", "units = [\"x.socket\"]", "units = [{ name = \"a.service\", scope = \"global\" }]", "units = [\"elamond.service\"]", "units = [{ name = \"a.service\", other = 1 }]"] {
			assert!(toml::from_str::<store::Config>(bad).is_err(), "{bad}");
		}
	}

	#[test]
	fn links_that_arent_web_pages_are_left_out_wherever_they_came_from() {
		let link = |name: &str, url: &str| store::LinkConfig { name: name.into(), url: url.into(), unit: None, scope: None };
		let config = store::Config { links: vec![link("Web", "http://{host}"), link("File", "file:///etc/passwd")], ..Default::default() };
		let app = [
			LinkDef { id: "a".into(), name: "Old".into(), url: "javascript:alert(1)".into(), unit: None, scope: None },
			LinkDef { id: "b".into(), name: "Mine".into(), url: "{host}:81".into(), unit: None, scope: None },
		];
		let names: Vec<String> = all(&config, &app).into_iter().map(|(l, _)| l.name).collect();
		assert_eq!(names, ["Web", "Mine"]);
		assert!(settings_notes(&config)[0].contains("\"File\" is left out"));
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
	fn links_saved_read_back() {
		let dir = std::env::temp_dir().join(format!("elamond-links-{}", data_encoding::HEXLOWER.encode(&store::random_bytes::<6>())));
		fs::create_dir_all(&dir).unwrap();
		let def = |id: &str| LinkDef { id: id.into(), name: id.into(), url: "http://{host}:81".into(), unit: None, scope: None };
		let ids = |list: Vec<LinkDef>| list.into_iter().map(|l| l.id).collect::<Vec<_>>();
		assert!(load_in(&dir).unwrap().is_empty());

		save_in(&dir, &[def("a"), def("b")]).unwrap();
		assert_eq!(ids(load_in(&dir).unwrap()), ["a", "b"]);
		save_in(&dir, &[]).unwrap();
		assert!(load_in(&dir).unwrap().is_empty());
		fs::remove_dir_all(dir).unwrap();
	}
}
