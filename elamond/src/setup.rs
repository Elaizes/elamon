//! `elamond setup`: installs the service for this user on any distribution that runs systemd, the
//! way the NixOS and Home Manager modules do elsewhere, and prints what needs root (lingering, the
//! firewall, the polkit rule) as commands to run. `elamond setup --check` tells whether the service
//! and what it uses work here.
//!
//! The init system is behind `InitSystem`, so another one (OpenRC, runit) can be added beside
//! systemd without touching the rest. The service itself needs none of it: `elamond serve` runs the
//! same under any of them, or by hand.
//!
//! Settings are read here as plain TOML, only for the few keys setup needs (`port`, `listen`,
//! `mdns`, `control`, and the units of `[links.<id>]` with `scope = "system"`).

use std::ffi::{CStr, OsString};
use std::fs;
use std::io::{self, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::consts::{protocol, timeout};
use crate::listen::{self, Target};
use crate::store;

/// The unit as shipped; `elamond setup` points its ExecStart at the binary that runs it.
const UNIT_TEMPLATE: &str = include_str!("../dist/elamond.service");
const UNIT_EXEC: &str = "ExecStart=/usr/bin/elamond serve";
const UNIT_NAME: &str = "elamond.service";
/// What setup writes at the top of the unit, so it knows the file for its own.
const UNIT_HEADER: &str = "# Written by `elamond setup`, which rewrites it when run again.\n";

/// The polkit rule's name, in the data directory and in /etc/polkit-1/rules.d.
const POLKIT_FILE: &str = "50-elamond.rules";
const POLKIT_DIR: &str = "/etc/polkit-1/rules.d";
/// In every rule for the service, the NixOS module's included, so `--check` can find one.
const POLKIT_MARK: &str = "Elamon";

/// UDP port of mDNS, which the app finds the computer by.
const MDNS_PORT: u16 = 5353;
/// Interface names a VPN goes by, where mDNS isn't opened (as the NixOS module guesses them).
const VPN_PREFIXES: [&str; 7] = ["tun", "tap", "wg", "nordlynx", "tailscale", "zt", "ppp"];

/// How long setup waits for a service it started to answer.
const START_WAIT: Duration = Duration::from_secs(15);
const START_POLL: Duration = Duration::from_millis(250);
/// A D-Bus question of `--check`, connecting included.
const BUS_WAIT: Duration = Duration::from_secs(3);

const USAGE: &str = "usage: elamond setup [--no-start] [--print] [--multiple-sessions] | elamond setup --check";

pub fn run(args: &[String]) -> Result<(), String> {
	let mut opts = Opts::default();
	for a in args {
		match a.as_str() {
			"--check" => opts.check = true,
			"--print" => opts.print = true,
			"--no-start" => opts.no_start = true,
			"--multiple-sessions" => opts.multiple_sessions = true,
			"-h" | "--help" => {
				println!("{USAGE}\n\n{HELP}");
				return Ok(());
			}
			other => return Err(format!("unknown option \"{other}\"\n{USAGE}")),
		}
	}
	let init = Systemd;
	if opts.check {
		if opts.print || opts.no_start {
			return Err(USAGE.into());
		}
		return check(&init);
	}
	setup(&init, &opts)
}

const HELP: &str = "\
  --no-start            install and enable the service, but don't start it now
  --print               show what it would write and run, and do nothing
  --multiple-sessions   the polkit rule also allows powering off, restarting and sleeping while
                        other users are logged in, ending their sessions
  --check               one line for each thing the service needs: OK, ! (wrong), ? (can't tell)
                        or - (not here, and not needed); exits 1 when the service can't run";

#[derive(Default)]
struct Opts {
	check: bool,
	print: bool,
	no_start: bool,
	multiple_sessions: bool,
}

// ---- the init system ---------------------------------------------------------------------

/// What setup needs of the init system that runs the service for the user.
trait InitSystem {
	/// Its name, for what setup says.
	fn name(&self) -> &'static str;
	/// Whether it runs this computer.
	fn running(&self) -> bool;
	/// Whether this user's service manager answers, and why not when it doesn't.
	fn user_manager(&self) -> Result<(), String>;
	/// Where this user's definition of the service goes.
	fn unit_path(&self) -> PathBuf;
	/// The definition, running `exe`.
	fn unit_text(&self, exe: &str) -> String;
	/// The program a definition runs.
	fn unit_exe(&self, text: &str) -> Option<String>;
	/// What the user's service manager knows of the service.
	fn status(&self) -> Status;
	/// The commands that take up the definition as written, enable the service and start it
	/// (`Start::Now`) or start it again (`Start::Again`).
	fn activate(&self, start: Start) -> Vec<Vec<String>>;
	/// Whether the user's services run while they are logged out, when that can be told.
	fn lingers(&self, user: &str) -> Option<bool>;
	/// The command, as root, that keeps them running.
	fn linger_command(&self, user: &str) -> String;
	/// Whether a system service runs, when that can be told.
	fn system_active(&self, name: &str) -> Option<bool>;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Start {
	Not,
	Now,
	Again,
}

/// The service as the user's service manager sees it; `None` where it couldn't say.
#[derive(Default, Debug, PartialEq)]
struct Status {
	loaded: Option<bool>,
	path: Option<PathBuf>,
	enabled: Option<String>,
	active: Option<String>,
	main_pid: Option<u32>,
}

struct Systemd;

impl InitSystem for Systemd {
	fn name(&self) -> &'static str {
		"systemd"
	}

	fn running(&self) -> bool {
		// What sd_booted() looks at.
		Path::new("/run/systemd/system").is_dir()
	}

	fn user_manager(&self) -> Result<(), String> {
		match Command::new("systemctl").args(["--user", "show", "--property=Version"]).output() {
			Ok(o) if o.status.success() => Ok(()),
			Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
			Err(e) => Err(format!("systemctl: {e}")),
		}
	}

	fn unit_path(&self) -> PathBuf {
		config_home().join("systemd/user").join(UNIT_NAME)
	}

	fn unit_text(&self, exe: &str) -> String {
		unit_text(exe)
	}

	fn unit_exe(&self, text: &str) -> Option<String> {
		unit_exe(text)
	}

	fn status(&self) -> Status {
		let out = Command::new("systemctl")
			.args(["--user", "show", UNIT_NAME, "--property=LoadState,FragmentPath,UnitFileState,ActiveState,MainPID"])
			.output();
		match out {
			Ok(o) if o.status.success() => parse_status(&String::from_utf8_lossy(&o.stdout)),
			_ => Status::default(),
		}
	}

	fn activate(&self, start: Start) -> Vec<Vec<String>> {
		let systemctl = |args: &[&str]| std::iter::once("systemctl").chain(std::iter::once("--user")).chain(args.iter().copied()).map(String::from).collect();
		let mut out = vec![systemctl(&["daemon-reload"])];
		match start {
			Start::Not => out.push(systemctl(&["enable", "elamond"])),
			Start::Now => out.push(systemctl(&["enable", "--now", "elamond"])),
			Start::Again => {
				out.push(systemctl(&["enable", "elamond"]));
				out.push(systemctl(&["restart", "elamond"]));
			}
		}
		out
	}

	fn lingers(&self, user: &str) -> Option<bool> {
		let dir = Path::new("/var/lib/systemd/linger");
		dir.is_dir().then(|| dir.join(user).exists())
	}

	fn linger_command(&self, user: &str) -> String {
		format!("sudo loginctl enable-linger {user}")
	}

	fn system_active(&self, name: &str) -> Option<bool> {
		if !self.running() {
			return None;
		}
		let out = Command::new("systemctl").args(["is-active", "--quiet", name]).stderr(Stdio::null()).status().ok()?;
		Some(out.success())
	}
}

/// The unit from dist/elamond.service, running `exe`.
fn unit_text(exe: &str) -> String {
	let exec = format!("ExecStart={} serve", exec_arg(exe));
	format!("{UNIT_HEADER}{}", UNIT_TEMPLATE.replacen(UNIT_EXEC, &exec, 1))
}

/// A path as the first word of an Exec line: `%` and `$` doubled so systemd takes them as they
/// are, and quoted when it has spaces or quotes.
fn exec_arg(path: &str) -> String {
	let s = path.replace('%', "%%").replace('$', "$$");
	if s.chars().any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '\\' | ';')) {
		format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
	} else {
		s
	}
}

/// The program of a unit's ExecStart, unquoted.
fn unit_exe(text: &str) -> Option<String> {
	let line = text.lines().map(str::trim).find_map(|l| l.strip_prefix("ExecStart="))?;
	let line = line.trim_start_matches(['@', '-', ':', '+', '!']);
	let word = if let Some(rest) = line.strip_prefix('"') {
		let mut word = String::new();
		let mut chars = rest.chars();
		while let Some(c) = chars.next() {
			match c {
				'\\' => word.extend(chars.next()),
				'"' => break,
				c => word.push(c),
			}
		}
		word
	} else {
		line.split_whitespace().next()?.to_string()
	};
	Some(word.replace("%%", "%").replace("$$", "$"))
}

fn parse_status(text: &str) -> Status {
	let mut s = Status::default();
	for line in text.lines() {
		let Some((key, value)) = line.split_once('=') else { continue };
		let value = value.trim();
		let some = || (!value.is_empty()).then(|| value.to_string());
		match key {
			"LoadState" => s.loaded = Some(value == "loaded"),
			"FragmentPath" => s.path = some().map(PathBuf::from),
			"UnitFileState" => s.enabled = some(),
			"ActiveState" => s.active = some(),
			"MainPID" => s.main_pid = value.parse().ok().filter(|p| *p != 0),
			_ => {}
		}
	}
	s
}

fn config_home() -> PathBuf {
	std::env::var_os("XDG_CONFIG_HOME")
		.map(PathBuf::from)
		.filter(|p| p.is_absolute())
		.unwrap_or_else(|| home().join(".config"))
}

fn home() -> PathBuf {
	std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

/// A path with the home folder written `~`, for reading.
fn short(path: &Path) -> String {
	match path.strip_prefix(home()) {
		Ok(rest) if home() != Path::new("/") => format!("~/{}", rest.display()),
		_ => path.display().to_string(),
	}
}

// ---- the settings setup reads -------------------------------------------------------------

/// The settings setup needs, from the files the service reads.
#[derive(Debug, PartialEq)]
struct Settings {
	port: u16,
	mdns: bool,
	control: bool,
	listen: Vec<Target>,
	/// System units the links name, for the polkit rule.
	system_units: Vec<String>,
}

impl Settings {
	fn read(files: &[PathBuf]) -> Settings {
		let mut merged = toml::Table::new();
		for path in files {
			let Ok(text) = fs::read_to_string(path) else { continue };
			// A file that doesn't parse is the service's to report (`--check` does); setup takes
			// what the others give.
			if let Ok(table) = text.parse::<toml::Table>() {
				merge(&mut merged, table);
			}
		}
		Settings::of(&merged)
	}

	fn of(t: &toml::Table) -> Settings {
		let port = t.get("port").and_then(toml::Value::as_integer).and_then(|p| u16::try_from(p).ok()).filter(|p| *p != 0);
		let listen = match t.get("listen") {
			Some(toml::Value::String(s)) => vec![s.clone()],
			Some(toml::Value::Array(a)) => a.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
			_ => Vec::new(),
		};
		let listen: Vec<Target> = listen.iter().filter_map(|s| Target::parse(s).ok()).collect();
		let mut system_units: Vec<String> = Vec::new();
		if let Some(links) = t.get("links").and_then(toml::Value::as_table) {
			for link in links.values().filter_map(toml::Value::as_table) {
				let scope = link.get("scope").and_then(toml::Value::as_str);
				if let (Some(unit), Some("system")) = (link.get("unit").and_then(toml::Value::as_str), scope) {
					let unit = unit.trim().to_string();
					if !unit.is_empty() && !system_units.contains(&unit) {
						system_units.push(unit);
					}
				}
			}
		}
		system_units.sort();
		Settings {
			port: port.unwrap_or(protocol::DEFAULT_PORT),
			mdns: t.get("mdns").and_then(toml::Value::as_bool).unwrap_or(true),
			control: t.get("control").and_then(toml::Value::as_bool).unwrap_or(false),
			listen: if listen.is_empty() { vec![Target::ANY] } else { listen },
			system_units,
		}
	}

	/// The interfaces `listen` confines the service to; empty when it listens on any address.
	fn interfaces(&self) -> Vec<String> {
		let mut names = Vec::new();
		for t in &self.listen {
			match t {
				Target::Interface(name) => names.push(name.clone()),
				// An address may be on any interface: the port is opened on all of them.
				Target::Addr(_) => return Vec::new(),
			}
		}
		names
	}
}

/// A later settings file's tables merge with the earlier ones key by key; anything else replaces.
fn merge(into: &mut toml::Table, from: toml::Table) {
	for (key, value) in from {
		match (into.get_mut(&key), value) {
			(Some(toml::Value::Table(a)), toml::Value::Table(b)) => merge(a, b),
			(_, v) => {
				into.insert(key, v);
			}
		}
	}
}

/// The settings file setup writes where there is none.
const STARTER: &str = r#"# Settings of the Elamon service (elamond). Every key is optional; all of them are in
# docs/config.md (https://github.com/Elaizes/elamon/blob/main/docs/config.md).
# The service rereads this file when it changes. Files in elamond.d/*.toml beside it add to it.
# Keep a secret (a password in a link, a token among a command's arguments) in a file only you
# can read: chmod 600.

# Let paired phones act on this computer (power, the power profile, units, media, the actions
# below), each as far as its role allows.
control = false

# port = 47823
# listen = ["eno1", "tailscale0"]      # addresses or interfaces; every address by default
# names = ["deskpc.example.net"]       # names that reach this computer, for the pairing code

# A web page this computer hosts, and the system unit behind it that phones may start and
# stop. Run `elamond setup` again after adding one: the polkit rule it prints names the unit.
# [links.jellyfin]
# name = "Jellyfin"
# url = "http://{host}:8096"
# unit = "jellyfin.service"
# scope = "system"

# A command phones may run, with a choice for whoever runs it.
# [actions.brightness]
# name = "Screen brightness"
# command = ["brightnessctl", "set", "{level}"]
# params.level = { choices = ["10%", "50%", "100%"], labels = { "10%" = "Dim", "50%" = "Half", "100%" = "Full" }, default = "50%" }
# role = "operate"

# How phones look that have no profile of their own.
# [profiles.default]
# theme = "dark"
# dashboards = ["overview", "processes", "sensors", "control"]
"#;

// ---- polkit and the firewall --------------------------------------------------------------

/// The polkit rule that lets `user` do without a password what phones may ask the service for,
/// as the NixOS module writes it: power off, restart and sleep (locking a session of one's own
/// needs no rule), the power profile, and starting, stopping and restarting `units`.
fn polkit_rule(user: &str, units: &[String], multiple_sessions: bool) -> String {
	let mut power = vec![
		"org.freedesktop.login1.power-off",
		"org.freedesktop.login1.reboot",
		"org.freedesktop.login1.suspend",
		"org.freedesktop.UPower.PowerProfiles.switch-profile",
		"net.hadess.PowerProfiles.switch-profile",
	];
	if multiple_sessions {
		power.extend([
			"org.freedesktop.login1.power-off-multiple-sessions",
			"org.freedesktop.login1.reboot-multiple-sessions",
			"org.freedesktop.login1.suspend-multiple-sessions",
		]);
	}
	let mut rule = String::new();
	rule.push_str(&format!(
		"// {POLKIT_MARK}: what paired phones may do through {user}'s Elamon service, and so any program of\n\
		 // {user}'s, without a password. Written by `elamond setup`; it goes in {POLKIT_DIR}.\n"
	));
	rule.push_str("polkit.addRule(function (action, subject) {\n");
	rule.push_str(&format!("\tif (subject.user != {}) {{\n\t\treturn polkit.Result.NOT_HANDLED;\n\t}}\n", json(&user)));
	rule.push_str(&format!("\tvar power = {};\n", json(&power)));
	rule.push_str("\tif (power.indexOf(action.id) >= 0) {\n\t\treturn polkit.Result.YES;\n\t}\n");
	if !units.is_empty() {
		rule.push_str(&format!(
			"\tif (action.id == \"org.freedesktop.systemd1.manage-units\"\n\
			 \t\t&& {}.indexOf(action.lookup(\"unit\")) >= 0\n\
			 \t\t&& [\"start\", \"stop\", \"restart\"].indexOf(action.lookup(\"verb\")) >= 0) {{\n\
			 \t\treturn polkit.Result.YES;\n\t}}\n",
			json(&units)
		));
	}
	rule.push_str("\treturn polkit.Result.NOT_HANDLED;\n});\n");
	rule
}

/// JSON, which JavaScript reads as it is: a name can't break out of its string in the rule.
fn json<T: serde::Serialize + ?Sized>(v: &T) -> String {
	serde_json::to_string(v).unwrap_or_else(|_| "[]".into())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Firewall {
	Firewalld,
	Ufw,
	Nftables,
	/// iptables rules kept by a service of their own; setup can't tell their layout.
	Iptables,
	/// NixOS's own, set in its configuration.
	Nixos,
	None,
}

impl Firewall {
	fn detect(init: &dyn InitSystem) -> Firewall {
		let active = |name: &str| init.system_active(name) == Some(true);
		if Path::new("/etc/NIXOS").exists() {
			Firewall::Nixos
		} else if active("firewalld.service") {
			Firewall::Firewalld
		} else if fs::read_to_string("/etc/ufw/ufw.conf").is_ok_and(|t| ufw_enabled(&t)) {
			Firewall::Ufw
		} else if active("nftables.service") {
			Firewall::Nftables
		} else if active("iptables.service") || active("ip6tables.service") || active("netfilter-persistent.service") {
			Firewall::Iptables
		} else {
			Firewall::None
		}
	}

	fn name(self) -> &'static str {
		match self {
			Firewall::Firewalld => "firewalld",
			Firewall::Ufw => "ufw",
			Firewall::Nftables => "nftables",
			Firewall::Iptables => "iptables",
			Firewall::Nixos => "NixOS",
			Firewall::None => "none",
		}
	}
}

fn ufw_enabled(conf: &str) -> bool {
	conf.lines().any(|l| matches!(l.trim().split_once('='), Some(("ENABLED", v)) if v.trim().trim_matches('"').eq_ignore_ascii_case("yes")))
}

fn vpn(name: &str) -> bool {
	VPN_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// One interface `listen` names, and the firewalld zone it is in (`None`: the default zone).
#[derive(Clone, Debug, PartialEq)]
struct Opening {
	iface: String,
	zone: Option<String>,
}

/// Root commands that open the service's port (TCP) and mDNS (UDP 5353, unless `mdns` is off) in
/// `fw`: on every interface when `on` is empty, otherwise on those only, mDNS only on those that
/// aren't a VPN, where the service never announces itself.
fn firewall_commands(fw: Firewall, port: u16, mdns: bool, on: &[Opening]) -> Vec<String> {
	let mut out = Vec::new();
	match fw {
		Firewall::Firewalld => {
			// firewalld opens ports by zone, not by interface: each interface's zone.
			let mut zones: Vec<(Option<String>, bool)> = Vec::new();
			if on.is_empty() {
				zones.push((None, mdns));
			}
			for o in on {
				let wants_mdns = mdns && !vpn(&o.iface);
				match zones.iter_mut().find(|z| z.0 == o.zone) {
					Some(z) => z.1 |= wants_mdns,
					None => zones.push((o.zone.clone(), wants_mdns)),
				}
			}
			for (zone, with_mdns) in zones {
				let zone = zone.map(|z| format!(" --zone={z}")).unwrap_or_default();
				out.push(format!("sudo firewall-cmd --permanent{zone} --add-port={port}/tcp"));
				if with_mdns {
					out.push(format!("sudo firewall-cmd --permanent{zone} --add-service=mdns"));
				}
			}
			out.push("sudo firewall-cmd --reload".into());
		}
		Firewall::Ufw => {
			if on.is_empty() {
				out.push(format!("sudo ufw allow {port}/tcp"));
				if mdns {
					out.push(format!("sudo ufw allow {MDNS_PORT}/udp"));
				}
			}
			for o in on {
				out.push(format!("sudo ufw allow in on {} to any port {port} proto tcp", o.iface));
				if mdns && !vpn(&o.iface) {
					out.push(format!("sudo ufw allow in on {} to any port {MDNS_PORT} proto udp", o.iface));
				}
			}
		}
		Firewall::Nftables => {
			// The table and chain of the distributions' /etc/nftables.conf (Arch, Debian).
			let rule = |iface: Option<&str>, proto: &str, p: u16| {
				let on = iface.map(|i| format!("iifname \"{i}\" ")).unwrap_or_default();
				format!("sudo nft insert rule inet filter input {on}{proto} dport {p} accept")
			};
			if on.is_empty() {
				out.push(rule(None, "tcp", port));
				if mdns {
					out.push(rule(None, "udp", MDNS_PORT));
				}
			}
			for o in on {
				out.push(rule(Some(&o.iface), "tcp", port));
				if mdns && !vpn(&o.iface) {
					out.push(rule(Some(&o.iface), "udp", MDNS_PORT));
				}
			}
		}
		Firewall::Iptables | Firewall::Nixos | Firewall::None => {}
	}
	out
}

/// The interfaces `listen` names, with their firewalld zones when firewalld runs.
fn openings(settings: &Settings, fw: Firewall) -> Vec<Opening> {
	settings
		.interfaces()
		.into_iter()
		.map(|iface| {
			let zone = (fw == Firewall::Firewalld).then(|| firewalld_zone(&iface)).flatten();
			Opening { iface, zone }
		})
		.collect()
}

fn firewalld_zone(iface: &str) -> Option<String> {
	let out = Command::new("firewall-cmd").arg(format!("--get-zone-of-interface={iface}")).output().ok()?;
	let zone = String::from_utf8_lossy(&out.stdout).trim().to_string();
	(out.status.success() && !zone.is_empty() && !zone.contains(' ')).then_some(zone)
}

// ---- setup -------------------------------------------------------------------------------

const NIXOS: &str = "on NixOS the modules set the service up (guide, section 1): services.elamond.enable, in the \
	NixOS module or Home Manager's; the NixOS module's openFirewall for the firewall, control.users and control.units \
	for polkit; users.users.<name>.linger to keep it running while you're logged out";

/// Where the unit comes from when it isn't setup's to write.
fn managed_elsewhere(init: &dyn InitSystem) -> Option<&'static str> {
	let path = init.unit_path();
	let is_link = fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink());
	(is_link && fs::canonicalize(&path).is_ok_and(|t| t.starts_with("/nix/store"))).then_some("Nix: Home Manager's module, or a Nix profile's unit linked there")
}

/// The binary running now, as the unit should name it.
fn this_exe() -> Result<String, String> {
	let exe = std::env::current_exe().map_err(|e| format!("can't tell where this elamond is: {e}"))?;
	let exe = exe.to_str().ok_or_else(|| format!("{} isn't UTF-8; move elamond to a plainer path", exe.display()))?;
	// The binary was replaced since it started: the new one at the same path is meant.
	Ok(exe.strip_suffix(" (deleted)").unwrap_or(exe).to_string())
}

fn setup(init: &dyn InitSystem, opts: &Opts) -> Result<(), String> {
	// The firewall, polkit and lingering are the system's configuration there, not commands.
	if Path::new("/etc/NIXOS").exists() {
		return Err(NIXOS.into());
	}
	if !init.running() {
		return Err(format!(
			"{} isn't running here, and it's the only init system elamond setup knows so far.\n  \
			 Have your init system run `{} serve` as your user at login; it needs nothing else.",
			init.name(),
			this_exe().unwrap_or_else(|_| "elamond".into())
		));
	}
	// SAFETY: geteuid only reads the process's user id.
	if unsafe { libc::geteuid() } == 0 {
		return Err("run elamond setup as the user the service is for, not root: the service runs as that user, \
			 and setup prints the few commands that need root"
			.into());
	}
	let user = user_name().ok_or("can't tell which user this is")?;
	let settings = Settings::read(&store::config_files());
	let exe = this_exe()?;
	let managed = managed_elsewhere(init);
	if managed.is_none() && exe.starts_with("/nix/store/") {
		return Err("this elamond is from the Nix store, whose paths change with each update. Use Home Manager's module \
			 (services.elamond.enable = true), or link the unit of the profile it is installed in:\n  \
			 systemctl --user link ~/.nix-profile/lib/systemd/user/elamond.service && systemctl --user enable --now elamond"
			.into());
	}
	if opts.print {
		println!("elamond setup --print: what it would do, without doing it.\n");
	}
	match managed {
		Some(by) => println!("The unit comes from {by}, which keeps it: elamond setup leaves it, and the settings, alone."),
		None => install(init, opts, &exe, &settings)?,
	}
	let steps = root_steps(init, opts, &user, &settings)?;
	if !steps.is_empty() {
		println!("\nWhat needs root, for you to run:");
		for s in &steps {
			println!("\n  {}", s.why);
			for c in &s.commands {
				println!("      {c}");
			}
			if let Some(a) = &s.after {
				println!("  {a}");
			}
		}
	}
	println!("\nThen pair a phone: elamond pair");
	if !opts.print && managed.is_none() && !exe_stays(&exe) {
		println!("\n  ! The unit runs {exe}. Keep elamond where it stays, like ~/.local/bin, and run elamond setup from there.");
	}
	Ok(())
}

/// Writes the unit, and the settings to start from where there are none, and has the service
/// manager enable and start the service; with `--print`, says so instead.
fn install(init: &dyn InitSystem, opts: &Opts, exe: &str, settings: &Settings) -> Result<(), String> {
	let print = opts.print;
	if !print && let Err(e) = init.user_manager() {
		return Err(format!(
			"{}'s user manager doesn't answer ({e}).\n  \
			 Run elamond setup in a session of your own: logged in at the computer or over ssh, not through su or sudo.",
			init.name()
		));
	}
	let path = init.unit_path();
	let text = init.unit_text(exe);
	let before = fs::read_to_string(&path).ok();
	let changed = before.as_deref() != Some(text.as_str());
	let status = if print { Status::default() } else { init.status() };
	let start = if opts.no_start {
		Start::Not
	} else if status.active.as_deref() == Some("active") && (changed || replaced(status.main_pid, exe)) {
		Start::Again
	} else {
		Start::Now
	};
	if changed {
		let what = match before.as_deref().and_then(|t| init.unit_exe(t)) {
			None => format!("{} for {exe}", short(&path)),
			Some(old) if old != exe => format!("{}: it ran {old}, now {exe}", short(&path)),
			Some(_) => format!("{}, updated", short(&path)),
		};
		if print {
			println!("Would write {what}:\n\n{}", indent(&text));
		} else {
			write_file(&path, &text, 0o644).map_err(|e| format!("can't write {}: {e}", path.display()))?;
			done(&format!("wrote {what}"));
		}
	} else {
		done(&format!("{} is up to date", short(&path)));
	}
	let config = store::config_files().into_iter().next().filter(|_| std::env::var_os("ELAMON_CONFIG").is_none());
	if let Some(config) = config.filter(|p| fs::symlink_metadata(p).is_err()) {
		if print {
			println!("Would write {}, settings to start from:\n\n{}", short(&config), indent(STARTER));
		} else {
			write_new(&config, STARTER).map_err(|e| format!("can't write {}: {e}", config.display()))?;
			done(&format!("wrote {}, settings to start from (control is off)", short(&config)));
		}
	}
	let commands = init.activate(start);
	if print {
		println!("Would run:\n");
		for c in &commands {
			println!("    {}", c.join(" "));
		}
		return Ok(());
	}
	for c in &commands {
		run_command(c)?;
		done(&c.join(" "));
	}
	if start != Start::Not {
		let probe = probe_addrs(settings);
		let started = Instant::now();
		while !answers(&probe, settings.port) && started.elapsed() < START_WAIT {
			std::thread::sleep(START_POLL);
		}
		if answers(&probe, settings.port) {
			done(&format!("the service answers on port {}", settings.port));
		} else {
			println!(
				"  !   the service doesn't answer on port {} yet: systemctl --user status elamond, journalctl --user -u elamond -e",
				settings.port
			);
		}
	}
	Ok(())
}

fn done(what: &str) {
	println!("  OK  {what}");
}

/// Something that needs root, with the commands that do it.
struct RootStep {
	why: String,
	commands: Vec<String>,
	after: Option<String>,
}

/// What needs root: lingering, the firewall and the polkit rule, where they aren't done yet. The
/// polkit rule is written to the data directory for a command to install.
fn root_steps(init: &dyn InitSystem, opts: &Opts, user: &str, settings: &Settings) -> Result<Vec<RootStep>, String> {
	let mut steps = Vec::new();
	if init.lingers(user) != Some(true) {
		steps.push(RootStep { why: "Keep the service running while you're logged out:".into(), commands: vec![init.linger_command(user)], after: None });
	}

	let fw = Firewall::detect(init);
	let opened = openings(settings, fw);
	let mut what = format!("TCP {} for the app", settings.port);
	if settings.mdns {
		what.push_str(&format!(", UDP {MDNS_PORT} (mDNS) for finding the computer"));
	}
	if !opened.is_empty() {
		what.push_str(&format!(", on {}", opened.iter().map(|o| o.iface.as_str()).collect::<Vec<_>>().join(", ")));
	}
	let commands = firewall_commands(fw, settings.port, settings.mdns, &opened);
	match fw {
		Firewall::None => println!("\nNo firewall seems to be running: nothing to open ({what})."),
		Firewall::Iptables => steps.push(RootStep {
			why: format!("Let phones through the firewall: {what}, in the rules of the iptables service that runs."),
			commands,
			after: None,
		}),
		Firewall::Nftables => steps.push(RootStep {
			why: format!("Let phones through the firewall (nftables): {what}."),
			commands,
			after: Some("These last until the next boot; to keep them, add the same rules (from `iifname` or the protocol on) \
				 to the input chain in /etc/nftables.conf."
				.into()),
		}),
		Firewall::Firewalld | Firewall::Ufw | Firewall::Nixos => {
			steps.push(RootStep { why: format!("Let phones through the firewall ({}): {what}.", fw.name()), commands, after: None })
		}
	}

	let rule = polkit_rule(user, &settings.system_units, opts.multiple_sessions);
	let installed = Path::new(POLKIT_DIR).join(POLKIT_FILE);
	if fs::read_to_string(&installed).ok().as_deref() != Some(rule.as_str()) {
		let saved = store::data_dir().join("setup").join(POLKIT_FILE);
		if opts.print {
			println!("\nWould write {}:\n\n{}", short(&saved), indent(&rule));
		} else {
			store::private_dir(&store::data_dir())
				.and_then(|_| write_file(&saved, &rule, 0o644))
				.map_err(|e| format!("can't write {}: {e}", saved.display()))?;
		}
		let units = if settings.system_units.is_empty() {
			String::new()
		} else {
			format!(", and start, stop and restart {}", settings.system_units.join(", "))
		};
		let when = if settings.control { "" } else { " Needed once control is on (control = true)." };
		steps.push(RootStep {
			why: format!("Let the service power off, restart and sleep the computer and switch the power profile{units}, without a password.{when}"),
			commands: vec![format!("sudo install -m 0644 {} {}", saved.display(), installed.display())],
			after: Some("polkit allows that every program you run, not only the service. Run elamond setup again after changing the links' units.".into()),
		});
	}
	Ok(steps)
}

fn indent(text: &str) -> String {
	text.lines().map(|l| if l.is_empty() { String::new() } else { format!("    {l}") }).collect::<Vec<_>>().join("\n") + "\n"
}

/// Whether the running service's binary is no longer `exe`: replaced by an update, or another.
fn replaced(pid: Option<u32>, exe: &str) -> bool {
	let Some(pid) = pid else { return false };
	match fs::read_link(format!("/proc/{pid}/exe")) {
		Ok(running) => {
			let running = running.to_string_lossy();
			running.ends_with(" (deleted)") || running != exe
		}
		Err(_) => false,
	}
}

/// A binary in a `bin` folder stays there; one run from a download or a build folder may not.
fn exe_stays(exe: &str) -> bool {
	Path::new(exe).parent().and_then(Path::file_name).is_some_and(|d| d == "bin")
}

fn write_file(path: &Path, text: &str, mode: u32) -> io::Result<()> {
	let dir = path.parent().unwrap_or(Path::new("."));
	fs::create_dir_all(dir)?;
	let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
	let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
	let written = (|| {
		let mut f = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(mode).open(&tmp)?;
		f.write_all(text.as_bytes())?;
		f.sync_all()?;
		fs::rename(&tmp, path)
	})();
	if written.is_err() {
		let _ = fs::remove_file(&tmp);
	}
	written
}

/// Writes a file only its owner reads, never over one that is there.
fn write_new(path: &Path, text: &str) -> io::Result<()> {
	if let Some(dir) = path.parent() {
		fs::create_dir_all(dir)?;
	}
	let mut f = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
	f.write_all(text.as_bytes())
}

fn run_command(c: &[String]) -> Result<(), String> {
	let out = Command::new(&c[0]).args(&c[1..]).output().map_err(|e| format!("{}: {e}", c[0]))?;
	if out.status.success() {
		Ok(())
	} else {
		Err(format!("`{}` failed: {}", c.join(" "), String::from_utf8_lossy(&out.stderr).trim()))
	}
}

/// The user this runs as, by the password database.
fn user_name() -> Option<String> {
	// SAFETY: getpwuid_r fills `pwd` with pointers into `buf`, which outlives their use here.
	unsafe {
		let mut pwd: libc::passwd = std::mem::zeroed();
		let mut buf = vec![0 as libc::c_char; 16 * 1024];
		let mut found: *mut libc::passwd = std::ptr::null_mut();
		let rc = libc::getpwuid_r(libc::geteuid(), &mut pwd, buf.as_mut_ptr(), buf.len(), &mut found);
		if rc == 0 && !found.is_null() && !pwd.pw_name.is_null() {
			return CStr::from_ptr(pwd.pw_name).to_str().ok().map(String::from);
		}
	}
	std::env::var("USER").ok().filter(|u| !u.is_empty())
}

/// Where to look for the service: loopback when it listens on every address, else its own.
fn probe_addrs(settings: &Settings) -> Vec<IpAddr> {
	listen::Plan::of(&settings.listen, &listen::interfaces()).probe_addrs()
}

fn answers(addrs: &[IpAddr], port: u16) -> bool {
	addrs.iter().any(|a| TcpStream::connect_timeout(&SocketAddr::new(*a, port), timeout::SERVICE_CHECK).is_ok())
}

// ---- --check -----------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mark {
	Ok,
	/// Wrong, and worth fixing.
	Bad,
	/// Can't tell.
	Unknown,
	/// Not here, and not needed.
	Absent,
}

impl Mark {
	fn sign(self) -> &'static str {
		match self {
			Mark::Ok => "OK",
			Mark::Bad => "!",
			Mark::Unknown => "?",
			Mark::Absent => "-",
		}
	}
}

/// One line of `--check`, with what to do about it under it.
#[derive(Clone, Debug, PartialEq)]
struct Item {
	mark: Mark,
	what: String,
	hint: Option<String>,
	/// Without it the service doesn't run or can't be reached.
	essential: bool,
}

impl Item {
	fn new(mark: Mark, what: impl Into<String>) -> Item {
		Item { mark, what: what.into(), hint: None, essential: false }
	}

	fn hint(mut self, hint: impl Into<String>) -> Item {
		self.hint = Some(hint.into());
		self
	}

	fn essential(mut self) -> Item {
		self.essential = true;
		self
	}
}

fn format_items(items: &[Item]) -> String {
	let mut out = String::new();
	for i in items {
		// A line of its own for each line of it, as a parse error has several.
		let mut lines = i.what.lines().filter(|l| !l.trim().is_empty());
		out.push_str(&format!("  {:<3} {}\n", i.mark.sign(), lines.next().unwrap_or_default()));
		for line in lines.chain(i.hint.iter().flat_map(|h| h.lines())) {
			out.push_str(&format!("      {line}\n"));
		}
	}
	out
}

/// Whether something the service can't do without is wrong.
fn failed(items: &[Item]) -> bool {
	items.iter().any(|i| i.essential && i.mark == Mark::Bad)
}

fn check(init: &dyn InitSystem) -> Result<(), String> {
	let user = user_name().unwrap_or_else(|| "YOUR_USER".into());
	let settings = Settings::read(&store::config_files());
	let mut items = Vec::new();

	items.extend(check_unit(init));
	items.push(match store::load_config() {
		Ok(_) => Item::new(Mark::Ok, format!("settings read ({})", short(&store::config_dir()))),
		Err(e) => Item::new(Mark::Bad, format!("settings don't parse: {e}")).hint("elamond check names the line").essential(),
	});
	let probe = probe_addrs(&settings);
	let start = if init.running() {
		"systemctl --user start elamond; journalctl --user -u elamond -e says why it stopped"
	} else {
		"start elamond serve"
	};
	items.push(if answers(&probe, settings.port) {
		Item::new(Mark::Ok, format!("the service answers on port {}", settings.port))
	} else if probe.is_empty() {
		Item::new(Mark::Unknown, format!("the service listens on none of this computer's addresses yet (port {})", settings.port))
	} else {
		Item::new(Mark::Bad, format!("nothing answers on port {}", settings.port)).hint(start).essential()
	});
	if init.running() {
		items.push(match init.lingers(&user) {
			Some(true) => Item::new(Mark::Ok, "it runs while you're logged out (lingering)"),
			Some(false) => Item::new(Mark::Bad, "it runs only while you're logged in").hint(init.linger_command(&user)),
			None => Item::new(Mark::Unknown, "can't tell whether it runs while you're logged out"),
		});
	}
	items.push(check_firewall(init, &settings));
	items.push(check_polkit(&user, &settings));
	items.extend(check_buses());
	items.push(check_nvml());

	print!("{}", format_items(&items));
	if failed(&items) {
		return Err("the service can't run or be reached as it is; see the lines marked !".into());
	}
	Ok(())
}

fn check_unit(init: &dyn InitSystem) -> Vec<Item> {
	if !init.running() {
		return vec![Item::new(Mark::Unknown, format!("{} isn't running here", init.name())).hint(
			"elamond setup only knows systemd so far; have your init system run `elamond serve` as your user",
		)];
	}
	let mut items = Vec::new();
	if let Err(e) = init.user_manager() {
		items.push(
			Item::new(Mark::Bad, format!("{}'s user manager doesn't answer: {e}", init.name()))
				.hint("log in as this user (at the computer or over ssh, not through su or sudo)")
				.essential(),
		);
		let path = init.unit_path();
		items.push(if path.exists() {
			Item::new(Mark::Ok, format!("unit {}", short(&path)))
		} else {
			Item::new(Mark::Bad, "the service isn't installed").hint("elamond setup").essential()
		});
		return items;
	}
	items.push(Item::new(Mark::Ok, format!("{} --user answers", init.name())));
	let status = init.status();
	let Some(path) = status.path.clone().filter(|_| status.loaded == Some(true)) else {
		items.push(Item::new(Mark::Bad, "the service isn't installed").hint("elamond setup").essential());
		return items;
	};
	let runs = fs::read_to_string(&path).ok().and_then(|t| init.unit_exe(&t));
	let mut unit = Item::new(Mark::Ok, format!("unit {}{}", short(&path), runs.as_deref().map(|r| format!(", runs {r}")).unwrap_or_default()));
	if let (Some(runs), Ok(exe)) = (&runs, this_exe())
		&& *runs != exe
		&& !runs.starts_with("/nix/store/")
	{
		unit = unit.hint(format!("this is {exe}; elamond setup from it points the unit here"));
	}
	items.push(unit);
	items.push(match status.enabled.as_deref() {
		Some(s) if s.starts_with("enabled") || s == "linked" => Item::new(Mark::Ok, "enabled: it starts when you log in"),
		Some(s) => Item::new(Mark::Bad, format!("not enabled ({s}): it won't start at the next login")).hint("systemctl --user enable elamond").essential(),
		None => Item::new(Mark::Unknown, "can't tell whether it is enabled"),
	});
	items.push(match status.active.as_deref() {
		Some("active") => Item::new(Mark::Ok, "running"),
		Some(s) => Item::new(Mark::Bad, format!("not running ({s})"))
			.hint("systemctl --user start elamond; journalctl --user -u elamond -e says why it stopped")
			.essential(),
		None => Item::new(Mark::Unknown, "can't tell whether it runs"),
	});
	items
}

fn check_firewall(init: &dyn InitSystem, settings: &Settings) -> Item {
	let fw = Firewall::detect(init);
	let port = settings.port;
	let what = format!("TCP {port}");
	let fix = || firewall_commands(fw, port, settings.mdns, &openings(settings, fw)).join("\n");
	match fw {
		Firewall::None if init.running() => Item::new(Mark::Ok, "no firewall running: nothing to open"),
		Firewall::None => Item::new(Mark::Unknown, "can't tell which firewall runs, if one does"),
		Firewall::Firewalld => {
			let zones: Vec<Option<String>> = {
				let o = openings(settings, fw);
				if o.is_empty() { vec![None] } else { o.into_iter().map(|o| o.zone).collect() }
			};
			let mut answers = Vec::new();
			for zone in &zones {
				let mut c = Command::new("firewall-cmd");
				if let Some(z) = zone {
					c.arg(format!("--zone={z}"));
				}
				answers.push(c.arg(format!("--query-port={port}/tcp")).output().ok().and_then(|o| match o.status.code() {
					Some(0) => Some(true),
					Some(1) => Some(false),
					_ => None,
				}));
			}
			if answers.iter().all(|a| *a == Some(true)) {
				Item::new(Mark::Ok, format!("firewalld lets {what} through"))
			} else if answers.contains(&Some(false)) {
				Item::new(Mark::Bad, format!("firewalld doesn't let {what} through")).hint(fix())
			} else {
				Item::new(Mark::Unknown, format!("firewalld runs; can't tell whether it lets {what} through without root")).hint(fix())
			}
		}
		Firewall::Ufw => Item::new(Mark::Unknown, format!("ufw is on; can't tell whether it lets {what} through without root"))
			.hint(format!("sudo ufw status shows it; to open it:\n{}", fix())),
		Firewall::Nftables => Item::new(Mark::Unknown, format!("nftables runs; can't tell whether it lets {what} through without root"))
			.hint(format!("sudo nft list ruleset shows it; to open it:\n{}", fix())),
		Firewall::Iptables => Item::new(Mark::Unknown, format!("iptables rules are kept by a service; can't tell whether they let {what} through without root")),
		Firewall::Nixos => Item::new(Mark::Unknown, format!("NixOS's firewall: can't tell whether it lets {what} through"))
			.hint("services.elamond.openFirewall = true opens it, unless an interface the firewall trusts carries it"),
	}
}

fn check_polkit(user: &str, settings: &Settings) -> Item {
	if !settings.control {
		return Item::new(Mark::Absent, "polkit rule: not needed while control is off");
	}
	let installed = Path::new(POLKIT_DIR).join(POLKIT_FILE);
	let fix = || format!("elamond setup writes it and prints the command that installs it ({})", installed.display());
	match fs::read_to_string(&installed) {
		Ok(t) if t == polkit_rule(user, &settings.system_units, false) || t == polkit_rule(user, &settings.system_units, true) => {
			Item::new(Mark::Ok, format!("polkit rule {}", installed.display()))
		}
		Ok(_) => Item::new(Mark::Bad, format!("polkit rule {} isn't what the settings need now (their system units changed?)", installed.display())).hint(fix()),
		Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
			Item::new(Mark::Unknown, format!("can't read {POLKIT_DIR} without root")).hint(format!("sudo ls {POLKIT_DIR}"))
		}
		Err(_) => {
			// The NixOS module's rule is among the system's own.
			let other = fs::read_dir(POLKIT_DIR).into_iter().flatten().flatten().map(|e| e.path()).find(|p| {
				p.extension().is_some_and(|x| x == "rules") && fs::read_to_string(p).is_ok_and(|t| t.contains(&format!("// {POLKIT_MARK}")))
			});
			match other {
				Some(p) => Item::new(Mark::Ok, format!("polkit rule for Elamon in {}", p.display())),
				None => Item::new(Mark::Bad, "no polkit rule: power, the profile and system units ask for a password the service can't give").hint(fix()),
			}
		}
	}
}

/// logind and power-profiles-daemon on the system bus, media players on the session bus.
fn check_buses() -> Vec<Item> {
	let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
		return vec![Item::new(Mark::Unknown, "can't ask D-Bus")];
	};
	let names = |system: bool| {
		rt.block_on(async {
			tokio::time::timeout(BUS_WAIT, async {
				let conn = if system { zbus::Connection::system().await } else { zbus::Connection::session().await }.map_err(|e| e.to_string())?;
				let dbus = zbus::fdo::DBusProxy::new(&conn).await.map_err(|e| e.to_string())?;
				let mut names: Vec<String> = dbus.list_names().await.map_err(|e| e.to_string())?.iter().map(|n| n.to_string()).collect();
				if system {
					names.extend(dbus.list_activatable_names().await.map_err(|e| e.to_string())?.iter().map(|n| n.to_string()));
				}
				Ok::<_, String>(names)
			})
			.await
			.unwrap_or_else(|_| Err("no answer".into()))
		})
	};
	let mut items = Vec::new();
	match names(true) {
		Ok(names) => {
			items.push(if names.iter().any(|n| n == "org.freedesktop.login1") {
				Item::new(Mark::Ok, "logind: power off, restart, sleep and lock")
			} else {
				Item::new(Mark::Bad, "no logind on the system bus: no power actions")
			});
			items.push(if has_power_profiles(&names) {
				Item::new(Mark::Ok, "power profiles (power-profiles-daemon or tuned-ppd)")
			} else {
				Item::new(Mark::Absent, "no power profiles").hint("power-profiles-daemon (or tuned-ppd) lets phones switch them")
			});
		}
		Err(e) => items.push(Item::new(Mark::Bad, format!("no system bus ({e}): no power actions or power profiles"))),
	}
	items.push(match names(false) {
		Ok(_) => Item::new(Mark::Ok, "session bus: media players"),
		Err(e) => Item::new(Mark::Bad, format!("no session bus ({e}): no media players"))
			.hint("a login session brings one; on Debian and Ubuntu, with the dbus-user-session package"),
	});
	items
}

fn has_power_profiles(names: &[String]) -> bool {
	names.iter().any(|n| n == "org.freedesktop.UPower.PowerProfiles" || n == "net.hadess.PowerProfiles")
}

/// NVIDIA's management library, which the service reads NVIDIA graphics cards through.
fn check_nvml() -> Item {
	let mut candidates: Vec<OsString> = std::env::var_os("ELAMON_NVML_LIB").into_iter().collect();
	candidates.extend(["libnvidia-ml.so.1", "libnvidia-ml.so"].map(OsString::from));
	for dir in ["/run/opengl-driver/lib", "/usr/lib64", "/usr/lib/x86_64-linux-gnu", "/usr/lib/aarch64-linux-gnu", "/usr/lib"] {
		candidates.push(Path::new(dir).join("libnvidia-ml.so.1").into_os_string());
	}
	if candidates.iter().any(loadable) {
		return Item::new(Mark::Ok, "NVIDIA's library (NVML) loads");
	}
	if nvidia_present() {
		Item::new(Mark::Bad, "an NVIDIA graphics card, and NVIDIA's library (libnvidia-ml.so.1) doesn't load: no readings of it")
			.hint("install NVIDIA's driver from the distribution, or name the library in ELAMON_NVML_LIB")
	} else {
		Item::new(Mark::Absent, "NVIDIA's library: not needed, no NVIDIA graphics card")
	}
}

fn loadable(name: &OsString) -> bool {
	let Ok(c) = std::ffi::CString::new(name.clone().into_vec()) else { return false };
	// SAFETY: a library opened only to see that it loads, and closed again.
	unsafe {
		let h = libc::dlopen(c.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL);
		if h.is_null() {
			return false;
		}
		libc::dlclose(h);
	}
	true
}

fn nvidia_present() -> bool {
	let Ok(devices) = fs::read_dir("/sys/bus/pci/devices") else { return false };
	devices.flatten().any(|d| {
		let read = |f: &str| fs::read_to_string(d.path().join(f)).unwrap_or_default();
		read("vendor").trim() == "0x10de" && read("class").trim().starts_with("0x03")
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	fn table(text: &str) -> toml::Table {
		text.parse().unwrap()
	}

	/// The starter's examples, uncommented.
	fn starter_uncommented() -> String {
		STARTER
			.lines()
			.filter_map(|l| l.strip_prefix("# "))
			.filter(|l| l.starts_with('[') || l.contains(" = "))
			.filter(|l| !l.starts_with("port") && !l.starts_with("listen") && !l.starts_with("names"))
			.collect::<Vec<_>>()
			.join("\n")
	}

	#[test]
	fn the_unit_runs_this_binary() {
		assert_eq!(UNIT_TEMPLATE.matches(UNIT_EXEC).count(), 1, "dist/elamond.service has one ExecStart setup replaces");
		let text = unit_text("/home/alice/.local/bin/elamond");
		assert!(text.starts_with(UNIT_HEADER));
		assert!(text.contains("\nExecStart=/home/alice/.local/bin/elamond serve\n"));
		assert!(!text.contains("/usr/bin/elamond"));
		assert!(text.contains("ConditionUser=!@system") && text.contains("WantedBy=default.target"));
		assert_eq!(unit_exe(&text).as_deref(), Some("/home/alice/.local/bin/elamond"));
		assert_eq!(unit_exe(UNIT_TEMPLATE).as_deref(), Some("/usr/bin/elamond"));
	}

	#[test]
	fn odd_paths_are_quoted_for_systemd() {
		assert_eq!(exec_arg("/opt/elamond"), "/opt/elamond");
		assert_eq!(exec_arg("/home/alice/My Apps/elamond"), "\"/home/alice/My Apps/elamond\"");
		assert_eq!(exec_arg("/home/alice/100%/elamond"), "/home/alice/100%%/elamond");
		assert_eq!(exec_arg("/home/alice/$x/elamond"), "/home/alice/$$x/elamond");
		assert_eq!(exec_arg("/a \"b\"/elamond"), "\"/a \\\"b\\\"/elamond\"");
		for path in ["/home/alice/My Apps/elamond", "/home/alice/100% $x/elamond", "/a \"b\"\\c/elamond"] {
			assert_eq!(unit_exe(&unit_text(path)).as_deref(), Some(path), "{path}");
		}
	}

	#[test]
	fn systemctl_show_is_read() {
		let s = parse_status("LoadState=loaded\nFragmentPath=/home/alice/.config/systemd/user/elamond.service\nUnitFileState=enabled\nActiveState=active\nMainPID=4242\n");
		assert_eq!(
			s,
			Status {
				loaded: Some(true),
				path: Some("/home/alice/.config/systemd/user/elamond.service".into()),
				enabled: Some("enabled".into()),
				active: Some("active".into()),
				main_pid: Some(4242),
			}
		);
		let missing = parse_status("MainPID=0\nLoadState=not-found\nActiveState=inactive\nFragmentPath=\nUnitFileState=\n");
		assert_eq!(missing, Status { loaded: Some(false), active: Some("inactive".into()), ..Status::default() });
	}

	#[test]
	fn activating_enables_and_starts_as_asked() {
		let lines = |s| Systemd.activate(s).iter().map(|c| c.join(" ")).collect::<Vec<_>>();
		assert_eq!(lines(Start::Now), ["systemctl --user daemon-reload", "systemctl --user enable --now elamond"]);
		assert_eq!(lines(Start::Not), ["systemctl --user daemon-reload", "systemctl --user enable elamond"]);
		assert_eq!(lines(Start::Again), ["systemctl --user daemon-reload", "systemctl --user enable elamond", "systemctl --user restart elamond"]);
	}

	#[test]
	fn the_settings_setup_needs() {
		let s = Settings::of(&toml::Table::new());
		assert_eq!(s, Settings { port: 47823, mdns: true, control: false, listen: vec![Target::ANY], system_units: Vec::new() });
		assert!(s.interfaces().is_empty());

		let s = Settings::of(&table(
			r#"
			port = 48000
			mdns = false
			control = true
			listen = ["eno1", "tailscale0"]
			[links.jellyfin]
			name = "Jellyfin"
			url = "http://{host}:8096"
			unit = "jellyfin.service"
			scope = "system"
			[links.syncthing]
			name = "Syncthing"
			url = "http://{host}:8384"
			unit = "syncthing.service"
			[links.web]
			name = "Web"
			url = "http://{host}:80"
			unit = "nginx.service"
			scope = "system"
			[links.docs]
			name = "Docs"
			url = "http://{host}:8080"
			"#,
		));
		assert_eq!((s.port, s.mdns, s.control), (48000, false, true));
		assert_eq!(s.interfaces(), ["eno1", "tailscale0"]);
		assert_eq!(s.system_units, ["jellyfin.service", "nginx.service"], "only system units, sorted");

		// An address among them: it may be on any interface.
		assert!(Settings::of(&table(r#"listen = ["eno1", "192.0.2.10"]"#)).interfaces().is_empty());
		assert_eq!(Settings::of(&table(r#"listen = "wlan0""#)).interfaces(), ["wlan0"]);
		// Nonsense falls back to the defaults, which the service would refuse anyway.
		let s = Settings::of(&table("port = 70000\nmdns = \"yes\"\nlisten = []"));
		assert_eq!((s.port, s.mdns, s.listen.clone()), (47823, true, vec![Target::ANY]));
	}

	#[test]
	fn later_files_merge_into_earlier_ones() {
		let mut t = table("port = 48000\n[links.a]\nname = \"A\"\nunit = \"a.service\"\nscope = \"system\"\n");
		merge(&mut t, table("[links.a]\nscope = \"user\"\n[links.b]\nname = \"B\"\nunit = \"b.service\"\nscope = \"system\"\n"));
		merge(&mut t, table("listen = \"eno1\"\n"));
		let s = Settings::of(&t);
		assert_eq!(s.port, 48000);
		assert_eq!(s.system_units, ["b.service"]);
		assert_eq!(s.interfaces(), ["eno1"]);
	}

	#[test]
	fn the_starter_settings_parse_and_so_do_their_examples() {
		let t = table(STARTER);
		assert_eq!(t.get("control"), Some(&toml::Value::Boolean(false)));
		assert_eq!(t.len(), 1, "only control is set");
		// The service reads it as it is.
		toml::from_str::<store::Config>(STARTER).expect("the service takes the starter settings");

		let examples = table(&starter_uncommented());
		let link = &examples["links"]["jellyfin"];
		assert_eq!(link["scope"].as_str(), Some("system"));
		let action = &examples["actions"]["brightness"];
		assert_eq!(action["command"][2].as_str(), Some("{level}"));
		assert_eq!(action["params"]["level"]["choices"].as_array().map(Vec::len), Some(3));
		assert_eq!(action["params"]["level"]["labels"]["50%"].as_str(), Some("Half"));
		assert_eq!(examples["profiles"]["default"]["theme"].as_str(), Some("dark"));
		assert_eq!(Settings::of(&examples).system_units, ["jellyfin.service"]);
	}

	#[test]
	fn the_polkit_rule_is_the_modules() {
		let rule = polkit_rule("alice", &[], false);
		assert!(rule.starts_with(&format!("// {POLKIT_MARK}: ")));
		assert!(rule.contains("\tif (subject.user != \"alice\") {\n\t\treturn polkit.Result.NOT_HANDLED;\n\t}\n"));
		assert!(rule.contains(
			"var power = [\"org.freedesktop.login1.power-off\",\"org.freedesktop.login1.reboot\",\"org.freedesktop.login1.suspend\",\
			 \"org.freedesktop.UPower.PowerProfiles.switch-profile\",\"net.hadess.PowerProfiles.switch-profile\"];"
		));
		assert!(!rule.contains("multiple-sessions"));
		assert!(!rule.contains("manage-units"), "no units, no unit clause");
		assert!(rule.ends_with("\treturn polkit.Result.NOT_HANDLED;\n});\n"));

		let rule = polkit_rule("alice", &["jellyfin.service".into(), "nginx.service".into()], true);
		assert!(rule.contains("org.freedesktop.login1.power-off-multiple-sessions"));
		assert!(rule.contains("org.freedesktop.login1.suspend-multiple-sessions"));
		assert!(rule.contains(
			"\tif (action.id == \"org.freedesktop.systemd1.manage-units\"\n\
			 \t\t&& [\"jellyfin.service\",\"nginx.service\"].indexOf(action.lookup(\"unit\")) >= 0\n\
			 \t\t&& [\"start\", \"stop\", \"restart\"].indexOf(action.lookup(\"verb\")) >= 0) {\n\
			 \t\treturn polkit.Result.YES;\n\t}\n"
		));
		// Names are JSON strings: one can't end the string and add code.
		let rule = polkit_rule("alice", &["x\"]) || true || ([\"".into()], false);
		assert!(rule.contains("[\"x\\\"]) || true || ([\\\"\"]"));
	}

	#[test]
	fn the_polkit_rule_from_a_settings_table() {
		let s = Settings::of(&table("control = true\n[links.media]\nname = \"Media\"\nurl = \"http://{host}:8096\"\nunit = \"jellyfin.service\"\nscope = \"system\"\n"));
		let rule = polkit_rule("alice", &s.system_units, false);
		assert!(rule.contains("[\"jellyfin.service\"].indexOf(action.lookup(\"unit\"))"));
	}

	#[test]
	fn firewall_commands_per_backend() {
		let every: &[Opening] = &[];
		assert_eq!(
			firewall_commands(Firewall::Firewalld, 47823, true, every),
			["sudo firewall-cmd --permanent --add-port=47823/tcp", "sudo firewall-cmd --permanent --add-service=mdns", "sudo firewall-cmd --reload"]
		);
		assert_eq!(firewall_commands(Firewall::Firewalld, 48000, false, every), ["sudo firewall-cmd --permanent --add-port=48000/tcp", "sudo firewall-cmd --reload"]);
		assert_eq!(firewall_commands(Firewall::Ufw, 47823, true, every), ["sudo ufw allow 47823/tcp", "sudo ufw allow 5353/udp"]);
		assert_eq!(firewall_commands(Firewall::Ufw, 47823, false, every), ["sudo ufw allow 47823/tcp"]);
		assert_eq!(
			firewall_commands(Firewall::Nftables, 47823, true, every),
			["sudo nft insert rule inet filter input tcp dport 47823 accept", "sudo nft insert rule inet filter input udp dport 5353 accept"]
		);
		assert!(firewall_commands(Firewall::None, 47823, true, every).is_empty());
		assert!(firewall_commands(Firewall::Iptables, 47823, true, every).is_empty());

		// On chosen interfaces: mDNS never on a VPN.
		let on = [
			Opening { iface: "eno1".into(), zone: Some("home".into()) },
			Opening { iface: "tailscale0".into(), zone: Some("trusted".into()) },
			Opening { iface: "wlan0".into(), zone: Some("home".into()) },
		];
		assert_eq!(
			firewall_commands(Firewall::Firewalld, 47823, true, &on),
			[
				"sudo firewall-cmd --permanent --zone=home --add-port=47823/tcp",
				"sudo firewall-cmd --permanent --zone=home --add-service=mdns",
				"sudo firewall-cmd --permanent --zone=trusted --add-port=47823/tcp",
				"sudo firewall-cmd --reload",
			]
		);
		let unzoned = [Opening { iface: "wg0".into(), zone: None }];
		assert_eq!(firewall_commands(Firewall::Firewalld, 47823, true, &unzoned), ["sudo firewall-cmd --permanent --add-port=47823/tcp", "sudo firewall-cmd --reload"]);
		assert_eq!(
			firewall_commands(Firewall::Ufw, 47823, true, &on[..2]),
			["sudo ufw allow in on eno1 to any port 47823 proto tcp", "sudo ufw allow in on eno1 to any port 5353 proto udp", "sudo ufw allow in on tailscale0 to any port 47823 proto tcp"]
		);
		assert_eq!(
			firewall_commands(Firewall::Nftables, 47823, true, &on[1..]),
			[
				"sudo nft insert rule inet filter input iifname \"tailscale0\" tcp dport 47823 accept",
				"sudo nft insert rule inet filter input iifname \"wlan0\" tcp dport 47823 accept",
				"sudo nft insert rule inet filter input iifname \"wlan0\" udp dport 5353 accept",
			]
		);
	}

	#[test]
	fn ufw_is_on_when_its_settings_say_so() {
		assert!(ufw_enabled("# comment\nENABLED=yes\nLOGLEVEL=low\n"));
		assert!(ufw_enabled("ENABLED=\"yes\"\n"));
		assert!(!ufw_enabled("ENABLED=no\n"));
		assert!(!ufw_enabled("#ENABLED=yes\n"));
	}

	#[test]
	fn check_lines() {
		let items = [
			Item::new(Mark::Ok, "running"),
			Item::new(Mark::Bad, "it runs only while you're logged in").hint("sudo loginctl enable-linger alice"),
			Item::new(Mark::Unknown, "ufw is on").hint("sudo ufw status\nsudo ufw allow 47823/tcp"),
			Item::new(Mark::Absent, "no power profiles"),
			Item::new(Mark::Bad, "settings don't parse: elamond.toml: TOML parse error at line 2\n  |\n2 | [links.web]\n\n").hint("elamond check names the line"),
		];
		assert_eq!(
			format_items(&items),
			"  OK  running\n  !   it runs only while you're logged in\n      sudo loginctl enable-linger alice\n  ?   ufw is on\n      sudo ufw status\n      sudo ufw allow 47823/tcp\n  -   no power profiles\n  !   settings don't parse: elamond.toml: TOML parse error at line 2\n        |\n      2 | [links.web]\n      elamond check names the line\n"
		);
		assert!(!failed(&items), "nothing essential is wrong");
		let mut broken = items.to_vec();
		broken.push(Item::new(Mark::Bad, "nothing answers on port 47823").essential());
		assert!(failed(&broken));
		assert!(!failed(&[Item::new(Mark::Unknown, "systemd isn't running here").essential()]));
	}

	#[test]
	fn power_profiles_by_either_name() {
		assert!(has_power_profiles(&["org.freedesktop.UPower.PowerProfiles".into()]));
		assert!(has_power_profiles(&["net.hadess.PowerProfiles".into()]));
		assert!(!has_power_profiles(&["org.freedesktop.login1".into()]));
	}

	#[test]
	fn where_a_binary_stays() {
		assert!(exe_stays("/home/alice/.local/bin/elamond"));
		assert!(exe_stays("/usr/local/bin/elamond"));
		assert!(!exe_stays("/home/alice/Downloads/elamond-0.6.0-x86_64-linux/elamond"));
		assert!(!exe_stays("/home/alice/elamon/elamond/target/release/elamond"));
	}
}
