//! The service's announcement over mDNS, so the app finds it on the local network. Only there:
//! on wired and wireless interfaces the service listens on, never over a VPN or on a bridge for
//! containers, where no phone nearby hears it. With none of those, nothing is announced.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use mdns_sd::{IfKind, IfPredicate, ServiceDaemon, ServiceInfo};

use crate::consts::protocol;
use crate::listen::MdnsScope;

pub struct Announcer {
	name: String,
	id: String,
	fingerprint: String,
	port: u16,
	/// Where to announce, read by the daemon each time it looks at the interfaces (every few
	/// seconds), so a change applies without starting over.
	scope: Arc<RwLock<MdnsScope>>,
	/// Started once there is somewhere to announce.
	daemon: Option<ServiceDaemon>,
	/// The daemon couldn't be started; not tried again.
	failed: bool,
	/// The interfaces last said in the log.
	told: Option<Vec<String>>,
}

impl Announcer {
	pub fn new(name: &str, id: &str, fingerprint: &str, port: u16) -> Self {
		Announcer {
			name: name.into(),
			id: id.into(),
			fingerprint: fingerprint.into(),
			port,
			scope: Arc::new(RwLock::new(MdnsScope::default())),
			daemon: None,
			failed: false,
			told: None,
		}
	}

	/// Announces on `scope` from now on; on nothing, the announcement stops.
	pub fn update(&mut self, scope: MdnsScope) {
		let names: Vec<String> = scope.interfaces.iter().cloned().collect();
		if self.told.as_ref() != Some(&names) {
			if names.is_empty() {
				eprintln!("mdns: not announced: the service listens on no wired or wireless interface");
			} else {
				eprintln!("mdns: announced on {}", names.join(", "));
			}
			self.told = Some(names);
		}
		let empty = scope.interfaces.is_empty();
		if let Ok(mut s) = self.scope.write() {
			*s = scope;
		}
		if self.daemon.is_none() && !empty && !self.failed {
			self.daemon = self.start();
			self.failed = self.daemon.is_none();
		}
	}

	/// A new certificate: its fingerprint goes into the record.
	pub fn set_fingerprint(&mut self, fingerprint: &str) {
		self.fingerprint = fingerprint.into();
		if let Some(d) = &self.daemon {
			// Registering under the same name replaces the record.
			if let Some(info) = self.info() {
				let _ = d.register(info).map_err(|e| eprintln!("mdns: {e}"));
			}
		}
	}

	fn info(&self) -> Option<ServiceInfo> {
		let host = sysinfo::System::host_name().unwrap_or_else(|_| "elamon".into());
		let host: String = host.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' }).collect();
		let props: HashMap<String, String> = [
			("id".to_string(), self.id.clone()),
			("api".to_string(), protocol::API_VERSION.to_string()),
			("fp".to_string(), self.fingerprint[..protocol::MDNS_FINGERPRINT.min(self.fingerprint.len())].to_string()),
		]
		.into();
		ServiceInfo::new(protocol::SERVICE_TYPE, &self.name, &format!("{host}.local."), (), self.port, props)
			.map_err(|e| eprintln!("mdns: {e}"))
			.ok()
			.map(ServiceInfo::enable_addr_auto)
	}

	fn start(&self) -> Option<ServiceDaemon> {
		let daemon = ServiceDaemon::new().map_err(|e| eprintln!("mdns: {e}")).ok()?;
		let scope = self.scope.clone();
		let chosen = IfPredicate::new(move |i| scope.read().is_ok_and(|s| s.matches(&i.name, i.ip())));
		// The last selection that matches an address wins: none, then those in scope.
		daemon.disable_interface(IfKind::All).map_err(|e| eprintln!("mdns: {e}")).ok()?;
		daemon.enable_interface(IfKind::Predicate(chosen)).map_err(|e| eprintln!("mdns: {e}")).ok()?;
		daemon.register(self.info()?).map_err(|e| eprintln!("mdns: {e}")).ok()?;
		Some(daemon)
	}
}
