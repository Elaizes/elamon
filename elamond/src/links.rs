//! Links to the web pages this computer hosts: what a client opens, whether they answer, their units.
//!
//! Links come from the settings alone (`[links.<id>]`). links.json, where 0.5 kept the links added
//! from the app, is carried by backups, and turned into a drop-in of the settings once (`migrate`).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};

use serde::{Deserialize, Serialize};

use crate::consts::{limit, timeout};
use crate::settings::Link;
use crate::store;

/// A link added from the 0.5 app, as links.json kept it.
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
	pub port: Option<u16>,
	/// Something accepts connections on the port, seen from the computer itself.
	pub up: Option<bool>,
	/// Listening on loopback only: other devices can't open it.
	pub local_only: bool,
	pub unit_state: Option<String>,
}

/// links.json in the data directory, which 0.5 kept the links added from the app in.
pub const FILE: &str = "links.json";

#[cfg(test)]
pub fn save_in(dir: &std::path::Path, list: &[LinkDef]) -> std::io::Result<()> {
	store::write_json::<Vec<LinkDef>>(&dir.join(FILE), list)
}

/// Every link a client may open, by id; one whose URL isn't a web page's (`web_url`) is left out.
pub fn all(config: &store::Config) -> Vec<(&str, &Link)> {
	config.links.iter().filter(|(_, l)| web_url(&l.url)).map(|(id, l)| (id.as_str(), l)).collect()
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
pub fn statuses(config: &store::Config) -> Vec<LinkStatus> {
	let listening = listening_sockets();
	all(config)
		.into_iter()
		.map(|(id, l)| {
			let port = port_of(&l.url);
			let socks: Vec<&(u16, bool, u64)> = listening.iter().filter(|s| Some(s.0) == port).collect();
			LinkStatus {
				up: port.map(|p| !socks.is_empty() || probe(p)),
				local_only: !socks.is_empty() && socks.iter().all(|s| s.1),
				port,
				scope: l.scope().to_string(),
				id: id.to_string(),
				name: l.name.clone(),
				url: l.url.clone(),
				unit: l.unit.clone(),
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

/// Why the unit `unit` in `scope` ("user" when none) may not be started, stopped or restarted by a
/// client, if it may not: only a service unit a link in the settings names, in that scope, ever may.
pub fn unit_refusal(config: &store::Config, unit: &str, scope: Option<&str>) -> Option<String> {
	if let Err(why) = unit_shape(unit) {
		return Some(format!("{unit} {why}"));
	}
	let scope = scope.unwrap_or("user");
	let on_a_link = config.links.values().any(|l| l.unit.as_deref() == Some(unit) && l.scope() == scope);
	(!on_a_link).then(|| format!("no link in the settings names {unit} ({scope})"))
}

// ---- the links 0.5's app added -----------------------------------------------------------------

/// The drop-in the links added from 0.5's app are written to, in the settings' elamond.d.
pub const MIGRATED: &str = "links-from-app";

/// A link's id from its name: `a-z`, `0-9`, `_` and `-`, at most 32, starting with a letter or a
/// digit; `link` when nothing of the name is left; with `_2`, `_3`… when `taken` has it.
fn id_from(name: &str, taken: &BTreeSet<String>) -> String {
	let mut base = String::new();
	for c in name.to_lowercase().chars() {
		match c {
			'a'..='z' | '0'..='9' | '_' | '-' => base.push(c),
			_ if !base.is_empty() && !base.ends_with('_') => base.push('_'),
			_ => {}
		}
	}
	let mut base: String = base.trim_matches(|c| c == '_' || c == '-').chars().take(28).collect();
	if base.is_empty() {
		base = "link".into();
	}
	let mut id = base.clone();
	let mut n = 2;
	while taken.contains(&id) {
		id = format!("{base}_{n}");
		n += 1;
	}
	id
}

/// `dir/name.ext`, or `dir/name-2.ext`… when that is taken: never a file that is there.
fn free_path(dir: &Path, name: &str, ext: &str) -> PathBuf {
	let mut path = dir.join(format!("{name}{ext}"));
	let mut n = 2;
	while path.exists() || path.is_symlink() {
		path = dir.join(format!("{name}-{n}{ext}"));
		n += 1;
	}
	path
}

/// The settings drop-in for the links 0.5's app added, as `[links.<id>]` tables; ids made from the
/// names, unique among themselves and `taken`. A link that isn't a web page's is left out, and so is
/// a unit no link could have (with the reason); `None` when nothing is left.
pub fn migrated_toml(list: &[LinkDef], taken: &BTreeSet<String>) -> (Option<String>, Vec<String>) {
	let mut taken = taken.clone();
	let mut notes = Vec::new();
	let mut links = toml::Table::new();
	for l in list {
		let name = store::sanitize_name(&l.name);
		if !web_url(&l.url) {
			notes.push(format!("\"{name}\" left out: {} isn't a web page's address", l.url));
			continue;
		}
		let id = id_from(&name, &taken);
		taken.insert(id.clone());
		let mut t = toml::Table::new();
		t.insert("name".into(), name.clone().into());
		t.insert("url".into(), l.url.clone().into());
		if let Some(u) = l.unit.as_deref().filter(|u| !u.is_empty()) {
			match unit_shape(u) {
				Ok(()) => {
					t.insert("unit".into(), u.into());
					t.insert("scope".into(), (if l.scope.as_deref() == Some("system") { "system" } else { "user" }).into());
				}
				Err(why) => notes.push(format!("\"{name}\" keeps no unit: {u} {why}")),
			}
		}
		links.insert(id, t.into());
	}
	if links.is_empty() {
		return (None, notes);
	}
	let mut top = toml::Table::new();
	top.insert("links".into(), links.into());
	let body = toml::to_string(&top).unwrap_or_default();
	let text = format!(
		"# The links Elamon 0.5's app added, written here once by elamond 0.6 from links.json (which\n\
		 # is now links.json.migrated beside it). Change or remove them here; the app no longer edits\n\
		 # links. docs/config.md, Links.\n\n{body}"
	);
	(Some(text), notes)
}

/// Turns links.json in the data directory `data`, where 0.5 kept the links added from the app, into
/// the settings drop-in elamond.d/links-from-app.toml in the config directory `config`, once:
/// links.json becomes links.json.migrated. No file that is there is written over; a drop-in that
/// wouldn't read as settings isn't written, and links.json stays. Says what it did in the journal.
pub fn migrate(data: &Path, config: &Path, settings: Option<&store::Config>) {
	let from = data.join(FILE);
	if !from.exists() {
		return;
	}
	let list: Vec<LinkDef> = match store::read_json(&from) {
		Ok(Some(l)) => l,
		Ok(None) => return,
		Err(e) => {
			eprintln!("links: {} doesn't read ({e}); its links stay there, out of the settings", from.display());
			return;
		}
	};
	let taken: BTreeSet<String> = settings.map(|c| c.links.keys().cloned().collect()).unwrap_or_default();
	let (text, notes) = migrated_toml(&list, &taken);
	for n in &notes {
		eprintln!("links: from links.json, {n}");
	}
	let dropins = config.join(store::DROPIN_DIR);
	let written = match text {
		None => None,
		Some(text) => {
			if let Err(e) = crate::settings::read_sources(&[crate::settings::Source::new(format!("{MIGRATED}.toml"), text.clone())]) {
				eprintln!("links: the links of links.json don't make sound settings, so they stay there: {}", crate::settings::join_mistakes(&e));
				return;
			}
			let to = free_path(&dropins, MIGRATED, ".toml");
			let wrote = fs::create_dir_all(&dropins).and_then(|_| fs::write(&to, text));
			if let Err(e) = wrote {
				eprintln!("links: can't write {} ({e}); the links added from the app stay in {}", to.display(), from.display());
				return;
			}
			Some(to)
		}
	};
	let aside = free_path(data, &format!("{FILE}.migrated"), "");
	if let Err(e) = fs::rename(&from, &aside) {
		eprintln!("links: can't move {} aside ({e})", from.display());
	}
	match written {
		Some(to) => eprintln!(
			"links: the {} link(s) added from the app are in the settings now, {}; links.json is {}",
			list.len() - notes.iter().filter(|n| n.contains("left out")).count(),
			to.display(),
			aside.display()
		),
		None => eprintln!("links: links.json held no link to keep; it is {}", aside.display()),
	}
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
	fn web_pages_only() {
		for bad in ["javascript:alert(1)", "intent://x#Intent;end", "file:///etc/passwd", "data:text/html,x", "ftp://{host}", "http:x", "mailto:a@b"] {
			assert!(!web_url(bad), "{bad}");
		}
		for good in ["HTTPS://{host}/", "{host}:3000", "localhost:8080/admin", "pc.lan:81", "192.0.2.10:8096", "[::1]:9090"] {
			assert!(web_url(good), "{good}");
		}
		for unit in ["-.slice", "-.service", "user.slice", "x.socket", ".service", "elamond.service", "a b.service", ""] {
			assert!(unit_shape(unit).is_err(), "{unit}");
		}
		assert!(unit_shape("getty@tty1.service").is_ok());
	}

	#[test]
	fn only_units_a_link_names_may_be_used() {
		let config = crate::settings::read_sources(&[crate::settings::Source::new(
			"a.toml",
			"[links.a]\nname = \"A\"\nurl = \"http://{host}:1\"\nunit = \"nginx.service\"\nscope = \"system\"\n[links.b]\nname = \"B\"\nurl = \"http://{host}:2\"\nunit = \"mine.service\"\n",
		)])
		.unwrap();
		let allowed = |unit: &str, scope: Option<&str>| unit_refusal(&config, unit, scope).is_none();
		assert!(allowed("nginx.service", Some("system")) && allowed("mine.service", None) && allowed("mine.service", Some("user")));
		// Each in its own scope only, and nothing else whatever it is.
		assert!(!allowed("nginx.service", None) && !allowed("mine.service", Some("system")));
		for unit in ["podman-kube@https:--evil.service", "-.slice", "user@1000.service", "x.service"] {
			assert!(!allowed(unit, None) && !allowed(unit, Some("system")), "{unit}");
		}
		assert!(unit_refusal(&config, "-.slice", None).unwrap().contains("isn't a service unit"));
		// Settings with a link whose unit could never be allowed, or whose url isn't a web page's,
		// don't load.
		for bad in ["[links.a]\nname = \"A\"\nurl = \"http://x\"\nunit = \"elamond.service\"\n", "[links.a]\nname = \"A\"\nurl = \"file:///etc/passwd\"\n"] {
			assert!(crate::settings::read_sources(&[crate::settings::Source::new("a.toml", bad)]).is_err(), "{bad}");
		}
		assert_eq!(all(&config).iter().map(|(id, _)| *id).collect::<Vec<_>>(), ["a", "b"]);
	}

	#[test]
	fn the_links_the_app_added_become_settings_once() {
		let dir = std::env::temp_dir().join(format!("elamond-links-{}", data_encoding::HEXLOWER.encode(&store::random_bytes::<6>())));
		let (data, config) = (dir.join("data"), dir.join("config"));
		fs::create_dir_all(&data).unwrap();
		let link = |name: &str, url: &str, unit: Option<&str>| LinkDef { id: "x".into(), name: name.into(), url: url.into(), unit: unit.map(Into::into), scope: Some("system".into()) };
		let list = [
			link("Jellyfin", "http://{host}:8096", Some("jellyfin.service")),
			link("Jellyfin", "{host}:8097", None),
			link("Web UI", "https://{host}/", Some("elamond.service")),
			link("Script", "javascript:alert(1)", None),
			link("Ünïcode ☃", "http://{host}:1", None),
		];
		save_in(&data, &list).unwrap();
		let settings = crate::settings::read_sources(&[crate::settings::Source::new("a.toml", "[links.web_ui]\nname = \"Mine\"\nurl = \"http://{host}:2\"\n")]).unwrap();
		migrate(&data, &config, Some(&settings));
		let text = fs::read_to_string(config.join("elamond.d/links-from-app.toml")).unwrap();
		let read = crate::settings::read_sources(&[crate::settings::Source::new("links-from-app.toml", text)]).unwrap();
		let got: Vec<(&str, &str, Option<&str>, &str)> = read.links.iter().map(|(id, l)| (id.as_str(), l.name.as_str(), l.unit.as_deref(), l.scope())).collect();
		assert_eq!(
			got,
			[
				("jellyfin", "Jellyfin", Some("jellyfin.service"), "system"),
				("jellyfin_2", "Jellyfin", None, "user"),
				("n_code", "Ünïcode ☃", None, "user"),
				// Not the settings' own id, and not a unit no link may have.
				("web_ui_2", "Web UI", None, "user"),
			]
		);
		assert!(!data.join(FILE).exists() && data.join("links.json.migrated").exists());
		// Once: another links.json (a backup of 0.5 restored) goes beside, over nothing.
		save_in(&data, &list[..1]).unwrap();
		migrate(&data, &config, None);
		assert!(config.join("elamond.d/links-from-app-2.toml").exists() && data.join("links.json.migrated-2").exists());
		assert_eq!(id_from("---", &BTreeSet::new()), "link");
		assert_eq!(id_from(&"a".repeat(40), &BTreeSet::new()).len(), 28);
		fs::remove_dir_all(dir).unwrap();
	}
}
