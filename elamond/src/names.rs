//! Names for the pairing link, after the addresses the service listens on: the computer's Tailscale
//! name (MagicDNS) when the service listens on its Tailscale address, and the names in the
//! settings. Addresses change with networks and leases; a name stays, so a phone keeps finding the
//! computer by it. The computer's mDNS name (host.local) is never added by itself: Android looks
//! such names up poorly, so it only goes in when the settings name it.

use std::net::IpAddr;

use serde::{Deserialize, Deserializer};

use crate::consts::{limit, protocol, timeout};
use crate::listen::{Iface, Plan};

/// The longest name the DNS carries, and the longest of its labels.
const NAME_MAX: usize = 253;
const LABEL_MAX: usize = 63;

/// A DNS name: letters, digits and hyphens in dot-separated labels, the last not all digits (that
/// would be an address); a trailing dot allowed. What the app takes as a name, too.
pub fn is_name(s: &str) -> bool {
	let name = s.strip_suffix('.').unwrap_or(s);
	if name.is_empty() || name.len() > NAME_MAX {
		return false;
	}
	let labels: Vec<&str> = name.split('.').collect();
	let label_ok = |l: &&str| {
		(1..=LABEL_MAX).contains(&l.len()) && !l.starts_with('-') && !l.ends_with('-') && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
	};
	labels.iter().all(label_ok) && !labels.last().is_some_and(|l| l.bytes().all(|b| b.is_ascii_digit()))
}

/// `names` as the settings give them: host names only, each without its trailing dot.
pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
	let names = Vec::<String>::deserialize(d)?;
	names.iter().map(String::as_str).map(check).collect::<Result<_, _>>().map_err(serde::de::Error::custom)
}

/// One name of `names`, as kept, or why it isn't one.
pub fn check(n: &str) -> Result<String, String> {
	let n = n.trim();
	if n.trim_start_matches('[').trim_end_matches(']').parse::<IpAddr>().is_ok() {
		Err(format!("\"{n}\" in names is an address; elamond pair offers the addresses the service listens on by itself"))
	} else if !is_name(n) {
		Err(format!("\"{n}\" in names isn't a host name"))
	} else {
		Ok(n.strip_suffix('.').unwrap_or(n).to_string())
	}
}

/// Tailscale's addresses: 100.64.0.0/10 and fd7a:115c:a1e0::/48.
fn tailscale_range(ip: &IpAddr) -> bool {
	match ip {
		IpAddr::V4(a) => a.octets()[0] == 100 && a.octets()[1] & 0xc0 == 64,
		IpAddr::V6(a) => a.segments()[..3] == [0xfd7a, 0x115c, 0xa1e0],
	}
}

/// The computer's MagicDNS name when the service listens on one of its Tailscale addresses, from
/// `tailscale status`. Nothing when Tailscale isn't installed or running, MagicDNS is off, or it
/// doesn't answer within `timeout::TAILSCALE`.
pub fn tailscale(plan: &Plan, ifaces: &[Iface]) -> Option<String> {
	// Not worth asking when no Tailscale address is listened on.
	if !ifaces.iter().flat_map(|i| &i.addrs).any(|a| tailscale_range(a) && plan.listens_on(a)) {
		return None;
	}
	let status = crate::control::run_within(&["tailscale", "status", "--json", "--peers=false"], timeout::TAILSCALE)?;
	magicdns_name(&status, |ip| plan.listens_on(ip))
}

/// The name in `tailscale status --json`, when Tailscale runs with MagicDNS on and the service
/// listens on one of the computer's Tailscale addresses (`listened`).
fn magicdns_name(status: &str, listened: impl Fn(&IpAddr) -> bool) -> Option<String> {
	let v: serde_json::Value = serde_json::from_str(status).ok()?;
	if v["BackendState"] != "Running" || v["CurrentTailnet"]["MagicDNSEnabled"] == false {
		return None;
	}
	let me = &v["Self"];
	let reached = me["TailscaleIPs"].as_array()?.iter().filter_map(|a| a.as_str()?.parse::<IpAddr>().ok()).any(|a| listened(&a));
	let name = me["DNSName"].as_str()?.trim_end_matches('.');
	(reached && is_name(name)).then(|| name.to_ascii_lowercase())
}

/// Where the computer is offered to phones: addresses first, so names never crowd them out, then
/// names. A pairing link holds `protocol::PAIR_HOSTS_MAX` of them, `endpoints`
/// `limit::ENDPOINTS_MAX`.
#[derive(Debug, Default, PartialEq)]
pub struct Offered {
	pub addrs: Vec<String>,
	pub names: Vec<String>,
	/// What didn't fit, addresses before names.
	pub left_out: Vec<String>,
}

impl Offered {
	/// What a pairing link offers: `addrs` in their order, then `names` from the settings, then the
	/// Tailscale name; each once, at most `protocol::PAIR_HOSTS_MAX` in all.
	pub fn of(addrs: &[IpAddr], names: &[String], tailscale: Option<&str>) -> Offered {
		Offered::up_to(protocol::PAIR_HOSTS_MAX, addrs, names, tailscale)
	}

	/// `addrs` in their order, then `names`, then the Tailscale name; each once, `max` at most.
	pub fn up_to(max: usize, addrs: &[IpAddr], names: &[String], tailscale: Option<&str>) -> Offered {
		let mut out = Offered::default();
		let mut seen: Vec<String> = Vec::new();
		let mut room = max;
		for a in addrs {
			let a = a.to_string();
			if seen.contains(&a) {
				continue;
			}
			seen.push(a.clone());
			if room > 0 {
				room -= 1;
				out.addrs.push(a);
			} else {
				out.left_out.push(a);
			}
		}
		for n in names.iter().map(String::as_str).chain(tailscale) {
			let key = n.to_ascii_lowercase();
			if seen.contains(&key) {
				continue;
			}
			seen.push(key);
			if room > 0 {
				room -= 1;
				out.names.push(n.to_string());
			} else {
				out.left_out.push(n.to_string());
			}
		}
		out
	}

	/// Everything in the link's `h`, in order.
	pub fn hosts(&self) -> Vec<String> {
		self.addrs.iter().chain(&self.names).cloned().collect()
	}
}

/// Where paired phones find the computer now (`endpoints`): what a pairing link would offer, the
/// addresses of `addrs` (`Plan::pairing_hosts`), then `names`, then the Tailscale name, each once,
/// `limit::ENDPOINTS_MAX` at most.
pub fn endpoints(addrs: &[IpAddr], names: &[String], tailscale: Option<&str>) -> Vec<String> {
	Offered::up_to(limit::ENDPOINTS_MAX, addrs, names, tailscale).hosts()
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::listen::Target;

	fn ip(s: &str) -> IpAddr {
		s.parse().unwrap()
	}

	#[test]
	fn host_names() {
		for good in ["deskpc", "deskpc.tail1234.ts.net", "pc-1.example.net.", "DESKPC.lan", "1pc.example", "a.b.c.d.e"] {
			assert!(is_name(good), "{good}");
		}
		for bad in ["", ".", "deskpc..lan", "-pc.lan", "pc-.lan", "pc_1.lan", "my pc", "pc.lan/", "192.168.200.1", "pc.123", &"a".repeat(64), &["a"; 128].join(".")] {
			assert!(!is_name(bad), "{bad}");
		}
	}

	#[test]
	fn names_in_the_settings() {
		#[derive(Deserialize, Debug)]
		struct S {
			#[serde(deserialize_with = "deserialize")]
			names: Vec<String>,
		}
		let read = |t: &str| toml::from_str::<S>(t).map(|s| s.names);
		assert_eq!(read("names = [\"deskpc.example.net.\", \" pc.local \"]").unwrap(), ["deskpc.example.net", "pc.local"]);
		assert!(read("names = []").unwrap().is_empty());
		assert!(read("names = [\"192.168.200.10\"]").unwrap_err().to_string().contains("is an address"));
		assert!(read("names = [\"[fd7a::1]\"]").unwrap_err().to_string().contains("is an address"));
		assert!(read("names = [\"my pc\"]").unwrap_err().to_string().contains("isn't a host name"));
		assert!(read("names = \"deskpc.example.net\"").is_err());
	}

	#[test]
	fn tailscale_addresses() {
		for yes in ["100.64.0.1", "100.100.0.1", "100.127.255.254", "fd7a:115c:a1e0::1"] {
			assert!(tailscale_range(&ip(yes)), "{yes}");
		}
		for no in ["100.63.255.255", "100.128.0.1", "192.168.200.101", "fd7a:115c:a1e1::1", "fd00::1"] {
			assert!(!tailscale_range(&ip(no)), "{no}");
		}
	}

	/// What `tailscale status --json --peers=false` prints, the parts read.
	const STATUS: &str = r#"{
		"Version": "1.104.1",
		"BackendState": "Running",
		"TailscaleIPs": ["100.64.0.1", "fd7a:115c:a1e0::1"],
		"Self": {
			"HostName": "deskpc",
			"DNSName": "Deskpc.tail1234.ts.net.",
			"TailscaleIPs": ["100.64.0.1", "fd7a:115c:a1e0::1"]
		},
		"CurrentTailnet": { "Name": "someone@example.com", "MagicDNSSuffix": "tail1234.ts.net", "MagicDNSEnabled": true },
		"MagicDNSSuffix": "tail1234.ts.net"
	}"#;

	#[test]
	fn the_magicdns_name() {
		let all = |_: &IpAddr| true;
		assert_eq!(magicdns_name(STATUS, all).as_deref(), Some("deskpc.tail1234.ts.net"));
		// Only when the service listens on a Tailscale address of this computer: its IPv6 one will do.
		assert_eq!(magicdns_name(STATUS, |a| *a == ip("fd7a:115c:a1e0::1")).as_deref(), Some("deskpc.tail1234.ts.net"));
		assert_eq!(magicdns_name(STATUS, |a| *a == ip("192.168.200.101")), None);
		// Tailscale stopped or logged out, MagicDNS off, no name, or not Tailscale's answer at all.
		assert_eq!(magicdns_name(&STATUS.replace("\"Running\"", "\"Stopped\""), all), None);
		assert_eq!(magicdns_name(&STATUS.replace("\"MagicDNSEnabled\": true", "\"MagicDNSEnabled\": false"), all), None);
		assert_eq!(magicdns_name(&STATUS.replace("Deskpc.tail1234.ts.net.", ""), all), None);
		assert_eq!(magicdns_name("not json", all), None);
		assert_eq!(magicdns_name("{}", all), None);
		// A Tailscale from before CurrentTailnet: the name is taken.
		let old = STATUS.replace("\"CurrentTailnet\"", "\"Elsewhere\"");
		assert_eq!(magicdns_name(&old, all).as_deref(), Some("deskpc.tail1234.ts.net"));
	}

	#[test]
	fn tailscale_is_only_asked_where_the_service_listens_on_it() {
		let ifaces = vec![
			Iface { name: "eno1".into(), kind: "wired", addrs: vec![ip("192.168.200.101")] },
			Iface { name: "tailscale0".into(), kind: "vpn", addrs: vec![ip("100.64.0.1")] },
		];
		// Listening on the cable alone, no command is run: there would be nothing to add.
		let plan = Plan::of(&[Target::parse("eno1").unwrap()], &ifaces);
		assert_eq!(tailscale(&plan, &ifaces), None);
	}

	#[test]
	fn a_slow_or_missing_tailscale_costs_no_more_than_the_timeout() {
		use crate::control::run_within;
		use std::time::{Duration, Instant};
		let started = Instant::now();
		assert_eq!(run_within(&["sleep", "10"], Duration::from_millis(300)), None);
		assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
		assert_eq!(run_within(&["elamond-test-no-such-command"], Duration::from_secs(1)), None);
		assert_eq!(run_within(&["false"], Duration::from_secs(5)), None);
		assert_eq!(run_within(&["echo", "hi"], Duration::from_secs(5)).as_deref(), Some("hi\n"));
	}

	#[test]
	fn addresses_first_then_names() {
		let addrs = [ip("192.168.200.101"), ip("100.64.0.1")];
		let names = ["deskpc.example.net".to_string(), "Deskpc.tail1234.ts.net".to_string()];
		let o = Offered::of(&addrs, &names, Some("deskpc.tail1234.ts.net"));
		// The Tailscale name already in the settings is offered once.
		assert_eq!(o.hosts(), ["192.168.200.101", "100.64.0.1", "deskpc.example.net", "Deskpc.tail1234.ts.net"]);
		assert!(o.left_out.is_empty());
		assert_eq!(Offered::of(&[], &[], Some("deskpc.tail1234.ts.net")).hosts(), ["deskpc.tail1234.ts.net"]);
		// Past what the app reads, names give way to addresses, and what is left out is said.
		let many: Vec<IpAddr> = (1..=7).map(|n| ip(&format!("10.0.0.{n}"))).collect();
		let o = Offered::of(&many, &names[..1], Some("deskpc.tail1234.ts.net"));
		assert_eq!(o.hosts().len(), protocol::PAIR_HOSTS_MAX);
		assert_eq!(o.names, ["deskpc.example.net"]);
		assert_eq!(o.left_out, ["deskpc.tail1234.ts.net"]);
		let lots: Vec<IpAddr> = (1..=10).map(|n| ip(&format!("10.0.0.{n}"))).collect();
		let o = Offered::of(&lots, &names[..1], None);
		assert_eq!((o.addrs.len(), o.names.len()), (protocol::PAIR_HOSTS_MAX, 0));
		assert_eq!(o.left_out, ["10.0.0.9", "10.0.0.10", "deskpc.example.net"]);
	}

	/// A computer on a cable and on Wi-Fi (with only a link-local IPv4 address there), with
	/// Tailscale, Docker, libvirt, a container's veth and loopback.
	fn desk() -> Vec<Iface> {
		let iface = |name: &str, kind: &'static str, addrs: &[&str]| Iface { name: name.into(), kind, addrs: addrs.iter().map(|a| ip(a)).collect() };
		vec![
			iface("lo", "loopback", &["127.0.0.1", "::1"]),
			iface("eno1", "wired", &["192.0.2.10", "fe80::1", "2001:db8::10"]),
			iface("wlan0", "wireless", &["169.254.7.7", "fe80::2", "2001:db8::20"]),
			iface("tailscale0", "vpn", &["100.64.0.7", "fd7a:115c:a1e0::7"]),
			iface("docker0", "virtual", &["172.17.0.1"]),
			iface("virbr0", "virtual", &["192.168.122.1"]),
			iface("veth1a2b3c", "virtual", &["fe80::3", "10.88.0.1"]),
		]
	}

	#[test]
	fn endpoints_are_where_a_phone_could_reach_the_computer() {
		let ifaces = desk();
		let names = ["desk.example.net".to_string(), "Desk.tail0000.ts.net".to_string(), "desk.example.net".to_string()];
		let at = |listen: &[&str], names: &[String], tailscale: Option<&str>| {
			let targets: Vec<Target> = listen.iter().map(|t| Target::parse(t).unwrap()).collect();
			endpoints(&Plan::of(&targets, &ifaces).pairing_hosts(&ifaces), names, tailscale)
		};
		// Everywhere: the cable, Wi-Fi by its IPv6 address (its IPv4 one is link-local), then
		// Tailscale; no bridge for containers or machines, no loopback, nothing link-local. Then the
		// names, the Tailscale one once whatever its case.
		assert_eq!(
			at(&["::"], &names, Some("desk.tail0000.ts.net")),
			["192.0.2.10", "2001:db8::20", "100.64.0.7", "desk.example.net", "Desk.tail0000.ts.net"]
		);
		assert_eq!(at(&["::"], &[], Some("desk.tail0000.ts.net")), ["192.0.2.10", "2001:db8::20", "100.64.0.7", "desk.tail0000.ts.net"]);
		// Interfaces by name: the local network first whatever the order in `listen`.
		assert_eq!(at(&["tailscale0", "eno1"], &names[..1], None), ["192.0.2.10", "100.64.0.7", "desk.example.net"]);
		// An address alone; loopback is never one a phone could reach.
		assert_eq!(at(&["fd7a:115c:a1e0::7"], &[], None), ["fd7a:115c:a1e0::7"]);
		assert_eq!(at(&["127.0.0.1", "::1"], &names[..1], None), ["desk.example.net"]);
		assert!(at(&["lo"], &[], None).is_empty());
		// More than a phone keeps: addresses first, the names give way.
		let many: Vec<IpAddr> = (1..=20).map(|n| ip(&format!("192.0.2.{n}"))).collect();
		let kept = endpoints(&many, &names, Some("desk.tail0000.ts.net"));
		assert_eq!(kept.len(), limit::ENDPOINTS_MAX);
		assert_eq!(kept.last().map(String::as_str), Some("192.0.2.16"));
		let few: Vec<IpAddr> = many[..14].to_vec();
		assert_eq!(&endpoints(&few, &names, Some("desk.tail0000.ts.net"))[14..], ["desk.example.net", "Desk.tail0000.ts.net"]);
		// The same address twice is there once.
		assert_eq!(endpoints(&[ip("192.0.2.10"), ip("192.0.2.10")], &[], None), ["192.0.2.10"]);
	}
}
