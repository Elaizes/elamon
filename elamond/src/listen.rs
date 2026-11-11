//! Where the service listens. `listen` names IP addresses and network interfaces; an interface
//! stands for the addresses it has at the moment. Each address gets a socket of its own, so an
//! interface that has none yet (a VPN that comes up after the service) is listened on once it has
//! one, and an address that goes away stops being listened on.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener};
use std::path::Path;
use std::time::Duration;

use axum_server::tls_rustls::RustlsConfig;
use serde::{Deserialize, Deserializer};

use crate::consts::limit;

/// One entry of `listen`: an address, or an interface by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
	Addr(IpAddr),
	Interface(String),
}

/// The longest interface name Linux takes (IFNAMSIZ less its terminating zero).
const IFNAME_MAX: usize = 15;

impl Target {
	pub const ANY: Target = Target::Addr(IpAddr::V6(Ipv6Addr::UNSPECIFIED));

	pub fn parse(s: &str) -> Result<Target, String> {
		let s = s.trim();
		let bare = s.strip_prefix('[').and_then(|r| r.strip_suffix(']')).unwrap_or(s);
		if let Ok(ip) = bare.parse::<IpAddr>() {
			return Ok(Target::Addr(ip));
		}
		// What looks like an address and doesn't read as one is a typo, not an interface: interface
		// names have a dot or two at most (a VLAN's "eth0.100").
		let like_address = s.contains(':') || s.chars().all(|c| c.is_ascii_digit() || c == '.') || s.matches('.').count() >= 3;
		let name_ok = s.len() <= IFNAME_MAX && s != "." && s != ".." && !s.chars().any(|c| c == '/' || c.is_whitespace() || c.is_control());
		if s.is_empty() || like_address || !name_ok {
			return Err(format!("\"{s}\" is neither an IP address nor an interface name"));
		}
		Ok(Target::Interface(s.to_string()))
	}
}

impl fmt::Display for Target {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Target::Addr(ip) => write!(f, "{ip}"),
			Target::Interface(name) => write!(f, "{name}"),
		}
	}
}

/// `listen` as the settings give it: one entry, or a list of them.
pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Target>, D::Error> {
	#[derive(Deserialize)]
	#[serde(untagged)]
	enum OneOrMore {
		One(String),
		More(Vec<String>),
	}
	let entries = match OneOrMore::deserialize(d)? {
		OneOrMore::One(s) => vec![s],
		OneOrMore::More(v) => v,
	};
	if entries.is_empty() {
		return Err(serde::de::Error::custom("listen is empty; leave it out to listen on every address"));
	}
	entries.iter().map(|s| Target::parse(s).map_err(serde::de::Error::custom)).collect()
}

/// A network interface as the service sees it.
#[derive(Clone, Debug)]
pub struct Iface {
	pub name: String,
	/// "wired", "wireless", "vpn", "virtual" (bridges, containers) or "loopback".
	pub kind: &'static str,
	pub addrs: Vec<IpAddr>,
}

/// The interfaces and their addresses now.
pub fn interfaces() -> Vec<Iface> {
	let Ok(networks) = sysinfo::Networks::new_with_refreshed_list() else { return Vec::new() };
	let mut out: Vec<Iface> = networks
		.iter()
		.map(|(name, data)| {
			let name = name.to_string_lossy().into_owned();
			let kind = if name == "lo" { "loopback" } else { kind_of(&name) };
			Iface { kind, addrs: data.ip_networks().iter().map(|n| n.addr).collect(), name }
		})
		.collect();
	out.sort_by(|a, b| a.name.cmp(&b.name));
	out
}

/// What an interface is to the phones: what the samples call it, except that a bridge, bond or VLAN
/// over a wired or wireless interface is the local network it carries. (Samples keep calling it
/// virtual, so its traffic isn't counted twice.)
fn kind_of(name: &str) -> &'static str {
	match crate::collect::net_kind(name) {
		"virtual" => carried(Path::new(SYS_NET), name, LOWER_DEPTH).unwrap_or("virtual"),
		kind => kind,
	}
}

const SYS_NET: &str = "/sys/class/net";
/// A bridge over a bond over a cable is as deep as it is looked.
const LOWER_DEPTH: u8 = 3;

/// "wired" or "wireless" when a physical interface of that kind is under `name` in `root`
/// (sysfs's lower_* links: a bridge's ports, a bond's or a VLAN's links), `depth` levels down at most.
fn carried(root: &Path, name: &str, depth: u8) -> Option<&'static str> {
	if depth == 0 {
		return None;
	}
	let entries = std::fs::read_dir(root.join(name)).ok()?;
	let lowers: Vec<String> = entries.flatten().filter_map(|e| e.file_name().to_str()?.strip_prefix("lower_").map(str::to_string)).collect();
	for lower in lowers {
		let dir = root.join(&lower);
		if dir.join("wireless").exists() || dir.join("phy80211").exists() {
			return Some("wireless");
		}
		if dir.join("device").exists() {
			return Some("wired");
		}
		if let Some(kind) = carried(root, &lower, depth - 1) {
			return Some(kind);
		}
	}
	None
}

/// An address that can be listened on and reached without naming the interface: link-local ones
/// need that, and the app never gets them.
fn usable(ip: &IpAddr) -> bool {
	match ip {
		IpAddr::V4(a) => !a.is_link_local() && !a.is_unspecified() && !a.is_multicast() && !a.is_broadcast(),
		IpAddr::V6(a) => !a.is_unicast_link_local() && !a.is_unspecified() && !a.is_multicast(),
	}
}

/// One address to listen on, and the interface it is on when it is on one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spot {
	pub addr: IpAddr,
	pub iface: Option<String>,
}

impl Spot {
	/// " (eth0)" for the interface it is on, if any.
	fn on(&self) -> String {
		self.iface.as_ref().map(|i| format!(" ({i})")).unwrap_or_default()
	}
}

impl fmt::Display for Spot {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match &self.iface {
			Some(i) => write!(f, "{} ({i})", self.addr),
			None => write!(f, "{}", self.addr),
		}
	}
}

/// What `listen` comes to with the interfaces as they are.
#[derive(Debug, Default, PartialEq)]
pub struct Plan {
	/// The addresses to listen on, in the order `listen` gives them.
	pub spots: Vec<Spot>,
	/// Interfaces named in `listen` that have no address to listen on yet.
	pub waiting: Vec<String>,
	/// "::" is among them: every address, IPv4 too.
	pub any: bool,
	/// "0.0.0.0" is: every IPv4 address.
	pub any_v4: bool,
}

impl Plan {
	pub fn of(targets: &[Target], ifaces: &[Iface]) -> Plan {
		let any = targets.contains(&Target::ANY);
		let any_v4 = targets.contains(&Target::Addr(IpAddr::V4(Ipv4Addr::UNSPECIFIED)));
		// Beside "::" or "0.0.0.0", the addresses they cover would only collide with them.
		let covered = |a: IpAddr| match a {
			IpAddr::V6(v) if v.is_unspecified() => false,
			_ if any => true,
			IpAddr::V4(v) => any_v4 && !v.is_unspecified(),
			IpAddr::V6(_) => false,
		};
		let owner = |ip: IpAddr| ifaces.iter().find(|i| i.addrs.contains(&ip)).map(|i| i.name.clone());
		let mut plan = Plan { any, any_v4, ..Plan::default() };
		let add = |plan: &mut Plan, addr: IpAddr, iface: Option<String>| {
			if !covered(addr) && !plan.spots.iter().any(|s| s.addr == addr) {
				plan.spots.push(Spot { addr, iface });
			}
		};
		for t in targets {
			match t {
				Target::Addr(ip) => add(&mut plan, *ip, owner(*ip)),
				Target::Interface(name) => {
					let addrs: Vec<IpAddr> =
						ifaces.iter().filter(|i| &i.name == name).flat_map(|i| i.addrs.iter().copied()).filter(usable).collect();
					if addrs.is_empty() {
						plan.waiting.push(name.clone());
					}
					for a in addrs {
						add(&mut plan, a, Some(name.clone()));
					}
				}
			}
		}
		plan
	}

	/// Whether it listens on every address of an interface, as with "::" and "0.0.0.0".
	fn everywhere(&self, ip: &IpAddr) -> bool {
		self.any || (self.any_v4 && ip.is_ipv4())
	}

	/// Whether a phone could reach it at `ip`, an address of this computer's.
	pub fn listens_on(&self, ip: &IpAddr) -> bool {
		usable(ip) && (self.everywhere(ip) || self.spots.iter().any(|s| s.addr == *ip))
	}

	/// The interfaces it listens on, with the addresses of each it listens on.
	fn by_interface<'a>(&self, ifaces: &'a [Iface]) -> Vec<(&'a Iface, Vec<IpAddr>)> {
		ifaces
			.iter()
			.map(|i| {
				let addrs = i.addrs.iter().copied().filter(|a| self.listens_on(a)).collect();
				(i, addrs)
			})
			.filter(|(_, a): &(&Iface, Vec<IpAddr>)| !a.is_empty())
			.collect()
	}

	/// The addresses for a pairing link: those a phone could reach the service at, the local
	/// network first, then VPNs. Of each interface its IPv4 addresses, or its IPv6 ones when it
	/// has no IPv4 one listened on. Listening everywhere, bridges and containers are left out as
	/// no phone is behind them; named, they come last. Never loopback. All of them: `names::Offered`
	/// keeps what fits in the link.
	pub fn pairing_hosts(&self, ifaces: &[Iface]) -> Vec<IpAddr> {
		let wildcard = self.any || self.any_v4;
		let mut found: Vec<(u8, IpAddr)> = Vec::new();
		for (iface, addrs) in self.by_interface(ifaces) {
			let rank = match iface.kind {
				"wired" => 0,
				"wireless" => 1,
				"vpn" => 2,
				"virtual" if !wildcard || self.spots.iter().any(|s| addrs.contains(&s.addr)) => 3,
				_ => continue,
			};
			let v4: Vec<IpAddr> = addrs.iter().copied().filter(IpAddr::is_ipv4).collect();
			let chosen = if v4.is_empty() { addrs } else { v4 };
			for a in chosen {
				if !a.is_loopback() && !found.iter().any(|(_, f)| *f == a) {
					found.push((rank, a));
				}
			}
		}
		found.sort_by_key(|(rank, _)| *rank);
		found.into_iter().map(|(_, a)| a).collect()
	}

	/// Where `elamond pair` looks for the service: this computer's own address when it listens
	/// everywhere, or else the addresses it listens on.
	pub fn probe_addrs(&self) -> Vec<IpAddr> {
		let mut out = Vec::new();
		if self.any || self.any_v4 {
			out.push(IpAddr::V4(Ipv4Addr::LOCALHOST));
		}
		if self.any {
			out.push(IpAddr::V6(Ipv6Addr::LOCALHOST));
		}
		out.extend(self.spots.iter().map(|s| s.addr).filter(|a| !a.is_unspecified()));
		out.truncate(limit::SERVICE_CHECK_ADDRS);
		out
	}

	/// The interfaces to announce the service on by mDNS: only the local network, wired and
	/// wireless, never a VPN or a bridge, and only the addresses the service listens on there.
	pub fn mdns(&self, ifaces: &[Iface]) -> MdnsScope {
		let mut scope = MdnsScope::default();
		for (iface, addrs) in self.by_interface(ifaces) {
			if iface.kind == "wired" || iface.kind == "wireless" {
				scope.interfaces.insert(iface.name.clone());
				scope.addrs.extend(addrs);
			}
		}
		scope
	}
}

/// Where the service is announced: these interfaces, and on them these addresses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MdnsScope {
	pub interfaces: BTreeSet<String>,
	pub addrs: BTreeSet<IpAddr>,
}

impl MdnsScope {
	pub fn matches(&self, name: &str, ip: IpAddr) -> bool {
		self.interfaces.contains(name) && self.addrs.contains(&ip)
	}
}

/// A listening socket on `addr`, ready for the server. "::" falls back to "0.0.0.0" where the
/// kernel has IPv6 off.
pub fn bind(addr: IpAddr, port: u16) -> io::Result<TcpListener> {
	let listener = match TcpListener::bind(SocketAddr::new(addr, port)) {
		Err(e) if addr == IpAddr::V6(Ipv6Addr::UNSPECIFIED) => TcpListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port))).map_err(|_| e)?,
		other => other?,
	};
	listener.set_nonblocking(true)?;
	if let Err(e) = crate::server::keep_alive(&listener) {
		eprintln!("keepalive: {e}; phones that drop off the network count as watching until TCP gives up");
	}
	Ok(listener)
}

/// An address that isn't this computer's (yet): waited for, not an error.
pub fn not_here(e: &io::Error) -> bool {
	e.kind() == io::ErrorKind::AddrNotAvailable
}

pub type App = axum::extract::connect_info::IntoMakeServiceWithConnectInfo<axum::Router, SocketAddr>;

/// The service's listening sockets, one server on each, kept in step with `listen` as the
/// interfaces' addresses change.
pub struct Listeners {
	port: u16,
	tls: RustlsConfig,
	app: App,
	open: Vec<Open>,
	/// What was said about addresses that can't be listened on, so each is said once.
	told: BTreeMap<String, String>,
}

struct Open {
	spot: Spot,
	local: SocketAddr,
	handle: axum_server::Handle<SocketAddr>,
	task: tokio::task::JoinHandle<io::Result<()>>,
}

impl Listeners {
	pub fn new(port: u16, tls: RustlsConfig, app: App) -> Self {
		Listeners { port, tls, app, open: Vec::new(), told: BTreeMap::new() }
	}

	/// Serves on a socket bound for `spot`. Needs the runtime.
	pub fn serve(&mut self, spot: Spot, listener: TcpListener) {
		let local = listener.local_addr().unwrap_or(SocketAddr::new(spot.addr, self.port));
		let server = match axum_server::from_tcp_rustls(listener, self.tls.clone()) {
			Ok(s) => s,
			Err(e) => {
				eprintln!("listen: can't serve on {local}: {e}");
				return;
			}
		};
		let handle = axum_server::Handle::new();
		let task = tokio::spawn(server.handle(handle.clone()).serve(self.app.clone()));
		eprintln!("listen: on {}{}", local, spot.on());
		self.open.push(Open { spot, local, handle, task });
	}

	/// Says `what` about `key` unless it was just said; keys not said this round are forgotten,
	/// so the same trouble coming back is told again.
	fn tell(&mut self, seen: &mut BTreeSet<String>, key: String, what: String) {
		seen.insert(key.clone());
		if self.told.get(&key) != Some(&what) {
			eprintln!("listen: {what}");
			self.told.insert(key, what);
		}
	}

	/// Listens on what `listen` comes to now: new addresses are taken, those gone are let go.
	/// Connections on an address let go stay until they end.
	pub fn update(&mut self, plan: &Plan) {
		let mut seen = BTreeSet::new();
		self.open.retain(|o| {
			if o.task.is_finished() {
				eprintln!("listen: the server on {} stopped; starting it again", o.local);
				return false;
			}
			if plan.spots.iter().any(|s| s.addr == o.spot.addr) {
				return true;
			}
			o.handle.graceful_shutdown(None);
			eprintln!("listen: no longer on {}{}: the address is gone", o.local, o.spot.on());
			false
		});
		for spot in &plan.spots {
			if self.open.iter().any(|o| o.spot.addr == spot.addr) {
				continue;
			}
			match bind(spot.addr, self.port) {
				Ok(l) => self.serve(spot.clone(), l),
				Err(e) if not_here(&e) => {
					self.tell(&mut seen, format!("wait {}", spot.addr), format!("{} isn't this computer's address yet; listening on it once it is", spot.addr))
				}
				Err(e) => self.tell(&mut seen, format!("fail {}", spot.addr), format!("can't listen on {}: {e}; trying again", SocketAddr::new(spot.addr, self.port))),
			}
		}
		for name in &plan.waiting {
			self.tell(&mut seen, format!("wait {name}"), format!("{name} has no address yet; listening on it once it has one"));
		}
		self.told.retain(|k, _| seen.contains(k));
	}

	/// Stops every server: new connections at once, open ones within `grace`.
	pub async fn shutdown(self, grace: Duration) {
		for o in &self.open {
			o.handle.graceful_shutdown(Some(grace));
		}
		for o in self.open {
			let _ = tokio::time::timeout(grace + Duration::from_secs(1), o.task).await;
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn ip(s: &str) -> IpAddr {
		s.parse().unwrap()
	}

	fn iface(name: &str, kind: &'static str, addrs: &[&str]) -> Iface {
		Iface { name: name.into(), kind, addrs: addrs.iter().map(|a| ip(a)).collect() }
	}

	fn targets(list: &[&str]) -> Vec<Target> {
		list.iter().map(|s| Target::parse(s).unwrap()).collect()
	}

	/// A laptop on Wi-Fi and a cable, with Tailscale, Docker and loopback.
	fn laptop() -> Vec<Iface> {
		vec![
			iface("lo", "loopback", &["127.0.0.1", "::1"]),
			iface("eno1", "wired", &["192.168.200.101", "fe80::1", "2001:db8::101"]),
			iface("wlan0", "wireless", &["fe80::2", "2001:db8::202"]),
			iface("tailscale0", "vpn", &["100.64.0.1", "fd7a:115c:a1e0::1"]),
			iface("docker0", "virtual", &["172.17.0.1"]),
		]
	}

	#[test]
	fn entries_are_addresses_or_interface_names() {
		assert_eq!(Target::parse("::").unwrap(), Target::ANY);
		assert_eq!(Target::parse(" 192.168.200.101 ").unwrap(), Target::Addr(ip("192.168.200.101")));
		assert_eq!(Target::parse("[fd7a::1]").unwrap(), Target::Addr(ip("fd7a::1")));
		assert_eq!(Target::parse("tailscale0").unwrap(), Target::Interface("tailscale0".into()));
		assert_eq!(Target::parse("eth0.100").unwrap(), Target::Interface("eth0.100".into()));
		for bad in ["192.168.200.1o1", "192.168.200", "fd7a::g", "", "a/b", "has space", "much-too-long-name0"] {
			assert!(Target::parse(bad).is_err(), "{bad}");
		}
	}

	#[test]
	fn listen_takes_one_entry_or_a_list() {
		#[derive(Deserialize, Debug)]
		struct S {
			#[serde(deserialize_with = "deserialize")]
			listen: Vec<Target>,
		}
		let read = |t: &str| toml::from_str::<S>(t).map(|s| s.listen);
		assert_eq!(read("listen = \"::\"").unwrap(), [Target::ANY]);
		assert_eq!(read("listen = [\"tailscale0\", \"192.168.200.101\"]").unwrap(), targets(&["tailscale0", "192.168.200.101"]));
		assert!(read("listen = []").unwrap_err().to_string().contains("empty"));
		assert!(read("listen = [\"1.2.3\"]").unwrap_err().to_string().contains("neither"));
		assert!(read("listen = 5").is_err());
	}

	#[test]
	fn interfaces_come_to_their_addresses() {
		let plan = Plan::of(&targets(&["tailscale0", "192.168.200.101", "wg0"]), &laptop());
		let spots: Vec<String> = plan.spots.iter().map(Spot::to_string).collect();
		assert_eq!(spots, ["100.64.0.1 (tailscale0)", "fd7a:115c:a1e0::1 (tailscale0)", "192.168.200.101 (eno1)"]);
		assert_eq!(plan.waiting, ["wg0"]);
		// Link-local addresses aren't enough: wlan0 without its global one waits.
		let mut ifaces = laptop();
		ifaces[2].addrs.pop();
		assert_eq!(Plan::of(&targets(&["wlan0"]), &ifaces).waiting, ["wlan0"]);
		// An address no interface has is still tried: binding tells whether it's there.
		let plan = Plan::of(&targets(&["10.9.9.9"]), &laptop());
		assert_eq!(plan.spots, [Spot { addr: ip("10.9.9.9"), iface: None }]);
	}

	#[test]
	fn everywhere_takes_the_place_of_what_it_covers() {
		let plan = Plan::of(&targets(&["192.168.200.101", "::", "tailscale0"]), &laptop());
		assert_eq!(plan.spots, [Spot { addr: ip("::"), iface: None }]);
		let plan = Plan::of(&targets(&["0.0.0.0", "eno1", "::"]), &laptop());
		assert_eq!(plan.spots.iter().map(|s| s.addr).collect::<Vec<_>>(), [ip("::")]);
		// "0.0.0.0" covers IPv4 only.
		let plan = Plan::of(&targets(&["0.0.0.0", "eno1"]), &laptop());
		assert_eq!(plan.spots.iter().map(|s| s.addr).collect::<Vec<_>>(), [ip("0.0.0.0"), ip("2001:db8::101")]);
	}

	#[test]
	fn pairing_names_what_is_listened_on_the_local_network_first() {
		let hosts = |list: &[&str]| -> Vec<String> {
			Plan::of(&targets(list), &laptop()).pairing_hosts(&laptop()).iter().map(|a| a.to_string()).collect()
		};
		// Everywhere: wired, then Wi-Fi (no IPv4, so its IPv6), then the VPN; no bridge, no loopback.
		assert_eq!(hosts(&["::"]), ["192.168.200.101", "2001:db8::202", "100.64.0.1"]);
		assert_eq!(hosts(&["0.0.0.0"]), ["192.168.200.101", "100.64.0.1"]);
		// Named: only those, the local network first whatever the order in `listen`.
		assert_eq!(hosts(&["tailscale0", "192.168.200.101"]), ["192.168.200.101", "100.64.0.1"]);
		assert_eq!(hosts(&["fd7a:115c:a1e0::1"]), ["fd7a:115c:a1e0::1"]);
		// A bridge named outright counts, last; loopback never.
		assert_eq!(hosts(&["docker0", "tailscale0", "lo"]), ["100.64.0.1", "172.17.0.1"]);
		assert!(hosts(&["127.0.0.1"]).is_empty());
	}

	#[test]
	fn pair_looks_for_the_service_where_it_listens() {
		let probe = |list: &[&str]| -> Vec<String> { Plan::of(&targets(list), &laptop()).probe_addrs().iter().map(|a| a.to_string()).collect() };
		assert_eq!(probe(&["::"]), ["127.0.0.1", "::1"]);
		assert_eq!(probe(&["0.0.0.0", "fd7a:115c:a1e0::1"]), ["127.0.0.1", "fd7a:115c:a1e0::1"]);
		assert_eq!(probe(&["tailscale0"]), ["100.64.0.1", "fd7a:115c:a1e0::1"]);
		assert!(probe(&["wg0"]).is_empty());
	}

	#[test]
	fn mdns_only_where_phones_nearby_can_hear_it() {
		let scope = |list: &[&str]| Plan::of(&targets(list), &laptop()).mdns(&laptop());
		let everywhere = scope(&["::"]);
		assert_eq!(everywhere.interfaces, BTreeSet::from(["eno1".to_string(), "wlan0".to_string()]));
		assert!(everywhere.matches("eno1", ip("192.168.200.101")) && everywhere.matches("wlan0", ip("2001:db8::202")));
		assert!(!everywhere.matches("tailscale0", ip("100.64.0.1")) && !everywhere.matches("docker0", ip("172.17.0.1")));
		// Only IPv4 under "0.0.0.0".
		assert!(!scope(&["0.0.0.0"]).matches("eno1", ip("2001:db8::101")));
		// Named: only the local network among them, and only the addresses listened on.
		let named = scope(&["192.168.200.101", "tailscale0", "docker0"]);
		assert_eq!(named.interfaces, BTreeSet::from(["eno1".to_string()]));
		assert!(named.matches("eno1", ip("192.168.200.101")) && !named.matches("eno1", ip("2001:db8::101")));
		// Over a VPN alone: nothing to announce on.
		assert_eq!(scope(&["tailscale0"]), MdnsScope::default());
	}

	#[test]
	fn a_bridge_over_a_cable_is_the_local_network() {
		let root = std::env::temp_dir().join(format!("elamond-sysnet-{}", std::process::id()));
		let dir = |path: &str| std::fs::create_dir_all(root.join(path)).unwrap();
		// br0 over bond0 over eno1; wlbr over Wi-Fi; podman0 over a container's veth.
		dir("eno1/device");
		dir("bond0/lower_eno1");
		dir("br0/lower_bond0");
		dir("wlp4s0/phy80211");
		dir("wlbr/lower_wlp4s0");
		dir("veth0");
		dir("podman0/lower_veth0");
		assert_eq!(carried(&root, "br0", LOWER_DEPTH), Some("wired"));
		assert_eq!(carried(&root, "wlbr", LOWER_DEPTH), Some("wireless"));
		assert_eq!(carried(&root, "podman0", LOWER_DEPTH), None);
		assert_eq!(carried(&root, "br0", 1), None, "only so deep");
		std::fs::remove_dir_all(root).unwrap();
	}

	#[test]
	fn binding_an_address_that_isnt_here_is_waited_for() {
		// 192.0.2.0/24 is for documentation: no computer has it.
		let e = bind(ip("192.0.2.1"), 0).unwrap_err();
		assert!(not_here(&e), "{e}");
		let l = bind(ip("127.0.0.1"), 0).unwrap();
		let port = l.local_addr().unwrap().port();
		assert!(!not_here(&bind(ip("127.0.0.1"), port).unwrap_err()));
	}
}
