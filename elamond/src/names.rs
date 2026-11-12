//! Names for the pairing link, after the addresses the service listens on: the computer's Tailscale
//! name (MagicDNS) when the service listens on its Tailscale address, and the names in the
//! settings. Addresses change with networks and leases; a name stays, so a phone keeps finding the
//! computer by it. The computer's mDNS name (host.local) is never added by itself: Android looks
//! such names up poorly, so it only goes in when the settings name it.

use std::net::IpAddr;

use serde::{Deserialize, Deserializer};

use crate::consts::{protocol, timeout};
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

/// What a pairing link offers: addresses first, so names never crowd them out, then names, at most
/// `protocol::PAIR_HOSTS_MAX` in all.
#[derive(Debug, Default, PartialEq)]
pub struct Offered {
	pub addrs: Vec<String>,
	pub names: Vec<String>,
	/// What didn't fit, addresses before names.
	pub left_out: Vec<String>,
}

impl Offered {
	/// `addrs` in their order, then `names` from the settings, then the Tailscale name; each once.
	pub fn of(addrs: &[IpAddr], names: &[String], tailscale: Option<&str>) -> Offered {
		let mut out = Offered::default();
		let mut seen: Vec<String> = Vec::new();
		let mut room = protocol::PAIR_HOSTS_MAX;
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
}
