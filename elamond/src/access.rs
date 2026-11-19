//! Who may do what: each paired phone's role.
//!
//! Roles go `glance` < `monitor` < `operate` < `admin` < `owner`, each allowing what the one
//! before does and more: what each sees is decided value by value (`values::address_role`), what each may run
//! action by action (`actions::Spec::role`). Acting on the computer also needs control:
//! `control = true` in the settings and no `elamond control off`, whatever the role. `owner`
//! (from `[web] role` up) also has the admin API: the settings, the clients, the dashboard editor.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Role {
	/// The basics: CPU, memory, GPUs, temperatures, fans, disks, network, battery, power.
	Glance,
	/// Everything there is to see, nothing to change.
	Monitor,
	/// And links, their units, media, the power profile and the configured actions.
	Operate,
	/// And power and signals to processes. A phone with no role in clients.json is this.
	#[default]
	Admin,
	/// And the admin API: changing the settings (and so adding commands), the clients and their
	/// roles. No client gets it by itself: the computer gives it, or a client that has it.
	Owner,
}

impl Role {
	pub const ALL: [Role; 5] = [Role::Glance, Role::Monitor, Role::Operate, Role::Admin, Role::Owner];

	pub fn as_str(self) -> &'static str {
		match self {
			Role::Glance => "glance",
			Role::Monitor => "monitor",
			Role::Operate => "operate",
			Role::Admin => "admin",
			Role::Owner => "owner",
		}
	}

	pub fn parse(s: &str) -> Option<Role> {
		Role::ALL.into_iter().find(|r| r.as_str() == s)
	}

	/// What the role allows, for people at the terminal.
	pub fn describe(self) -> &'static str {
		match self {
			Role::Glance => "the basics: CPU, memory, GPUs, temperatures, fans, disks, network, battery and power; no processes, monitors, links or media",
			Role::Monitor => "sees everything, changes nothing",
			Role::Operate => "also the units of links, media and the volume, the power profile and actions",
			Role::Admin => "everything: powering off, restarting, sleeping, locking and ending processes too",
			Role::Owner => "and the settings too, from the web page's admin panel and the dashboard editor: whoever has it may add a command and run it",
		}
	}

	/// The roles as the CLI lists them: "glance|monitor|operate|admin|owner".
	pub fn choices() -> String {
		Role::ALL.map(Role::as_str).join("|")
	}
}

/// The paired phone a request comes from.
#[derive(Clone, Debug, PartialEq)]
pub struct Caller {
	pub id: String,
	pub name: String,
	pub role: Role,
	/// Its profile in clients.json; `default_profile` when none, or one the settings don't have.
	pub profile: Option<String>,
	/// The app it said it is last, as clients.json has it ("android/0.6.0").
	pub app: Option<String>,
	/// The address it came from last, as clients.json has it.
	pub address: Option<String>,
	/// The entry its key came from (`store::family_of`), its own id for a phone paired with a
	/// code: the share of the activity log are the family's.
	pub family: String,
	/// A browser, let in by its cookie (`store::Client::web`).
	pub web: bool,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn roles_are_ordered_and_named() {
		assert!(Role::Glance < Role::Monitor && Role::Monitor < Role::Operate && Role::Operate < Role::Admin && Role::Admin < Role::Owner);
		for r in Role::ALL {
			assert_eq!(Role::parse(r.as_str()), Some(r));
			assert_eq!(serde_json::to_value(r).unwrap(), r.as_str());
		}
		assert_eq!(Role::parse("Admin"), None);
		assert_eq!(Role::default(), Role::Admin);
		assert_eq!(Role::choices(), "glance|monitor|operate|admin|owner");
		assert_eq!(Role::parse("owner"), Some(Role::Owner));
	}
}
