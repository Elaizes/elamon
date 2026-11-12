//! Links to the web pages this computer hosts: what a client opens, whether they answer, their units.
//!
//! Links come from the settings alone (`[links.<id>]`). links.json, where 0.5 kept the links added
//! from the app, is carried by backups, and turned into a drop-in of the settings once (`migrate`).

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
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

/// A link's id from its name: `app-`, then `a-z`, `0-9`, `_` and `-` of the name, at most 32 in
/// all; `app-link` when nothing of the name is left; with `_2`, `_3`… when `taken` has it. The
/// prefix keeps them apart from the ids an owner gives links, which a drop-in's would merge into.
fn id_from(name: &str, taken: &BTreeSet<String>) -> String {
	let mut base = String::new();
	for c in name.to_lowercase().chars() {
		match c {
			'a'..='z' | '0'..='9' | '_' | '-' => base.push(c),
			_ if !base.is_empty() && !base.ends_with('_') => base.push('_'),
			_ => {}
		}
	}
	let mut base: String = base.trim_matches(|c| c == '_' || c == '-').chars().take(22).collect();
	if base.is_empty() {
		base = "link".into();
	}
	let base = format!("app-{base}");
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
/// a unit no link could have (with the reason). A unit stays the link's only where the owner let
/// it be, as 0.5 checked at each use: `approved` has it, with its scope. Any other is written
/// commented out, for the owner to confirm by taking the `#` off (said in a note). `None` when
/// nothing is left; with the text, how many links it has.
pub fn migrated_toml(list: &[LinkDef], taken: &BTreeSet<String>, approved: &BTreeSet<(String, String)>) -> (Option<(String, usize)>, Vec<String>) {
	let mut taken = taken.clone();
	let mut notes = Vec::new();
	let mut body = String::new();
	let mut written = 0;
	for l in list {
		let name = store::sanitize_name(&l.name);
		if !web_url(&l.url) {
			notes.push(format!("\"{name}\" left out: {} isn't a web page's address", l.url));
			continue;
		}
		let id = id_from(&name, &taken);
		taken.insert(id.clone());
		let mut link = toml::Table::new();
		link.insert("name".into(), name.clone().into());
		link.insert("url".into(), l.url.clone().into());
		let scope = if l.scope.as_deref() == Some("system") { "system" } else { "user" };
		let unit = l.unit.as_deref().filter(|u| !u.is_empty());
		let shaped = unit.map(|u| unit_shape(u).map(|()| u));
		let owners = shaped.and_then(Result::ok).filter(|u| approved.contains(&(u.to_string(), scope.to_string())));
		if let Some(u) = owners {
			link.insert("unit".into(), u.into());
			link.insert("scope".into(), scope.into());
		}
		let mut top = toml::Table::new();
		top.insert("links".into(), toml::Value::Table(toml::Table::from_iter([(id.clone(), toml::Value::Table(link))])));
		body.push('\n');
		body.push_str(&toml::to_string(&top).unwrap_or_default());
		written += 1;
		if let (Some(u), None) = (unit, owners) {
			match unit_shape(u) {
				Ok(()) => {
					body.push_str(&format!(
						"# The 0.5 app gave it this unit, for phones to start, stop and restart. They may once you\n\
						 # take the # off these two lines; a system unit also needs `elamond setup` run again.\n\
						 # unit = {}\n# scope = \"{scope}\"\n",
						toml::Value::String(u.to_string())
					));
					notes.push(format!("\"{name}\" ({id}) has its unit {u} ({scope}) commented out, until you take the # off its lines"));
				}
				Err(why) => notes.push(format!("\"{name}\" keeps no unit: {u} {why}")),
			}
		}
	}
	if written == 0 {
		return (None, notes);
	}
	let text = format!(
		"# The links Elamon 0.5's app added, written here once by elamond 0.6 from links.json (which\n\
		 # is now links.json.migrated beside it). Change or remove them here; the app no longer edits\n\
		 # links. docs/config.md, Links.\n{body}"
	);
	(Some((text, written)), notes)
}

/// The units 0.5's settings let a link added from the app use, with their scopes: those in `units`
/// and on a `[[links]]` entry. Read as plain TOML, since 0.6 doesn't load settings that still have
/// them.
fn approved_in(files: &[PathBuf]) -> BTreeSet<(String, String)> {
	let text = |v: &toml::Value, key: &str| v.get(key).and_then(toml::Value::as_str).map(str::to_string);
	let mut out = BTreeSet::new();
	for t in files.iter().filter_map(|f| fs::read_to_string(f).ok()?.parse::<toml::Table>().ok()) {
		for u in t.get("units").and_then(toml::Value::as_array).into_iter().flatten() {
			let named = match u {
				toml::Value::String(name) => Some(name.clone()),
				other => text(other, "name"),
			};
			if let Some(name) = named {
				out.insert((name, text(u, "scope").unwrap_or_else(|| "user".into())));
			}
		}
		for l in t.get("links").and_then(toml::Value::as_array).into_iter().flatten() {
			if let Some(unit) = text(l, "unit") {
				out.insert((unit, text(l, "scope").unwrap_or_else(|| "user".into())));
			}
		}
	}
	out
}

/// The ids of the links the settings files `files` give, read as plain TOML: from settings that
/// don't load as a whole too (0.5's), so a link written from links.json never takes the id of one
/// the owner gave, which it would merge into.
fn link_ids_in(files: &[PathBuf]) -> BTreeSet<String> {
	files
		.iter()
		.filter_map(|f| fs::read_to_string(f).ok()?.parse::<toml::Table>().ok())
		.filter_map(|t| Some(t.get("links")?.as_table()?.keys().cloned().collect::<Vec<_>>()))
		.flatten()
		.collect()
}

/// Turns links.json in the data directory `data`, where 0.5 kept the links added from the app, into
/// the settings drop-in elamond.d/links-from-app.toml in the config directory `config`, once:
/// links.json becomes links.json.migrated. The ids are none the settings files `files` give a link,
/// and a unit stays a link's where they let links added from the app use it, as 0.5's did.
/// No file that is there is written over; a drop-in that wouldn't read as settings isn't written,
/// and links.json stays. Says what it did in the journal.
pub fn migrate(data: &Path, config: &Path, files: &[PathBuf]) {
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
	let (text, notes) = migrated_toml(&list, &link_ids_in(files), &approved_in(files));
	for n in &notes {
		eprintln!("links: from links.json, {n}");
	}
	let dropins = config.join(store::DROPIN_DIR);
	let written = match text {
		None => None,
		Some((text, n)) => {
			if let Err(e) = crate::settings::read_sources(&[crate::settings::Source::new(format!("{MIGRATED}.toml"), text.clone())]) {
				eprintln!("links: the links of links.json don't make sound settings, so they stay there: {}", crate::settings::join_mistakes(&e));
				return;
			}
			let to = free_path(&dropins, MIGRATED, ".toml");
			// Only the owner's to read, as links.json was: a link's address may hold a password.
			let wrote = fs::create_dir_all(&dropins).and_then(|_| {
				let mut f = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&to)?;
				f.write_all(text.as_bytes())
			});
			if let Err(e) = wrote {
				eprintln!("links: can't write {} ({e}); the links added from the app stay in {}", to.display(), from.display());
				let _ = fs::remove_file(&to);
				return;
			}
			Some((to, n))
		}
	};
	let aside = free_path(data, &format!("{FILE}.migrated"), "");
	if let Err(e) = fs::rename(&from, &aside) {
		eprintln!("links: can't move {} aside ({e})", from.display());
	}
	match written {
		Some((to, n)) => eprintln!("links: the {n} link(s) added from the app are in the settings now, {}; links.json is {}", to.display(), aside.display()),
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
	fn the_links_the_app_added_become_settings_once_their_units_left_to_the_owner() {
		use std::os::unix::fs::PermissionsExt;
		let dir = std::env::temp_dir().join(format!("elamond-links-{}", data_encoding::HEXLOWER.encode(&store::random_bytes::<6>())));
		let (data, config) = (dir.join("data"), dir.join("config"));
		fs::create_dir_all(&data).unwrap();
		let link = |name: &str, url: &str, unit: Option<&str>| LinkDef { id: "x".into(), name: name.into(), url: url.into(), unit: unit.map(Into::into), scope: Some("system".into()) };
		let list = [
			link("Jellyfin", "http://{host}:8096", Some("jellyfin.service")),
			link("Jellyfin", "{host}:8097", None),
			link("Web UI", "https://{host}/", Some("elamond.service")),
			link("Script", "javascript:alert(1)", None),
			link("Ünïcode ☃", "http://user:secret@{host}:1", None),
			link("Web", "http://{host}:80", Some("nginx.service")),
			link("Sync", "http://{host}:8384", Some("syncthing.service")),
			LinkDef { scope: Some("user".into()), ..link("Hole", "http://{host}:81", Some("pihole.service")) },
			link("Hole", "http://{host}:82", Some("pihole.service")),
		];
		save_in(&data, &list).unwrap();
		// The owner's settings, which don't load in 0.6 (a key it doesn't know) and give a link an
		// id the first link's would be: the migrated one takes another rather than merge into it.
		fs::create_dir_all(&config).unwrap();
		let main = config.join("elamond.toml");
		fs::write(&main, "no_such_key = 1\n[links.app-jellyfin]\nname = \"Mine\"\nurl = \"http://{host}:2\"\nunit = \"mine.service\"\n").unwrap();
		// 0.5's settings, in a drop-in of their own: the units they let links added from the app use.
		let old = config.join("old.toml");
		fs::write(&old, "units = [\"syncthing.service\", { name = \"nginx.service\", scope = \"system\" }]\n[[links]]\nname = \"Pi\"\nurl = \"http://{host}:3\"\nunit = \"pihole.service\"\nscope = \"system\"\n").unwrap();
		migrate(&data, &config, &[main.clone(), old.clone()]);
		let path = config.join("elamond.d/links-from-app.toml");
		// Only the owner's to read: an address may hold a password.
		assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
		let text = fs::read_to_string(&path).unwrap();
		let read = crate::settings::read_sources(&[crate::settings::Source::new("links-from-app.toml", text.clone())]).unwrap();
		let got: Vec<(&str, &str, Option<&str>)> = read.links.iter().map(|(id, l)| (id.as_str(), l.name.as_str(), l.unit.as_deref())).collect();
		// No unit but one the owner let them use, in its scope; never one no link may have.
		assert_eq!(
			got,
			[
				("app-hole", "Hole", None),
				("app-hole_2", "Hole", Some("pihole.service")),
				("app-jellyfin_2", "Jellyfin", None),
				("app-jellyfin_3", "Jellyfin", None),
				("app-n_code", "Ünïcode ☃", None),
				("app-sync", "Sync", None),
				("app-web", "Web", Some("nginx.service")),
				("app-web_ui", "Web UI", None),
			]
		);
		assert_eq!(read.links["app-web"].scope(), "system");
		assert!(text.contains("# unit = \"jellyfin.service\"\n# scope = \"system\"\n") && !text.contains("elamond.service"), "{text}");
		// The # taken off, it is the link's.
		let confirmed = text.replace("# unit = ", "unit = ").replace("# scope = ", "scope = ");
		let read = crate::settings::read_sources(&[crate::settings::Source::new("links-from-app.toml", confirmed)]).unwrap();
		assert_eq!((read.links["app-jellyfin_2"].unit.as_deref(), read.links["app-jellyfin_2"].scope()), (Some("jellyfin.service"), "system"));
		assert!(!data.join(FILE).exists() && data.join("links.json.migrated").exists());
		// What the journal is told of each.
		let (_, notes) = migrated_toml(&list[..5], &BTreeSet::new(), &BTreeSet::new());
		assert_eq!(
			notes,
			[
				"\"Jellyfin\" (app-jellyfin) has its unit jellyfin.service (system) commented out, until you take the # off its lines",
				"\"Web UI\" keeps no unit: elamond.service is the Elamon service's own unit",
				"\"Script\" left out: javascript:alert(1) isn't a web page's address",
			]
		);
		// Once: another links.json (a backup of 0.5 restored) goes beside, over nothing, its ids
		// none of those the settings files give.
		save_in(&data, &list[..1]).unwrap();
		migrate(&data, &config, &[main, old, path]);
		let again = fs::read_to_string(config.join("elamond.d/links-from-app-2.toml")).unwrap();
		assert!(again.contains("[links.app-jellyfin_4]") && data.join("links.json.migrated-2").exists(), "{again}");
		assert_eq!(id_from("---", &BTreeSet::new()), "app-link");
		assert_eq!(id_from(&"a".repeat(40), &BTreeSet::new()).len(), 26);
		assert!(crate::settings::is_id(&id_from(&"a".repeat(40), &(2..10_000).map(|n| format!("app-{}_{n}", "a".repeat(22))).chain([format!("app-{}", "a".repeat(22))]).collect())));
		fs::remove_dir_all(dir).unwrap();
	}
}
