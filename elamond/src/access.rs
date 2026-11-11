//! Who may do what: each paired phone's role, the things a role allows, and how often a phone
//! may act.
//!
//! Roles go `glance` < `monitor` < `operate` < `admin`, each allowing what the one before does
//! and more. Acting on the computer also needs control: `control = true` in the settings and
//! no `elamond control off`, whatever the role.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::consts::{cadence, limit};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Role {
	/// The basics: CPU, memory, GPUs, temperatures, fans, disks, network, battery, power.
	Glance,
	/// Everything there is to see, nothing to change.
	Monitor,
	/// And links, their units, media, the power profile and the configured actions.
	Operate,
	/// And power and signals to processes. Phones paired before 0.5 have no role and are this.
	#[default]
	Admin,
}

impl Role {
	pub const ALL: [Role; 4] = [Role::Glance, Role::Monitor, Role::Operate, Role::Admin];

	pub fn as_str(self) -> &'static str {
		match self {
			Role::Glance => "glance",
			Role::Monitor => "monitor",
			Role::Operate => "operate",
			Role::Admin => "admin",
		}
	}

	pub fn parse(s: &str) -> Option<Role> {
		Role::ALL.into_iter().find(|r| r.as_str() == s)
	}

	/// What the role allows, for people at the terminal.
	pub fn describe(self) -> &'static str {
		match self {
			Role::Glance => "the basics: CPU, memory, temperatures, disks, network and battery; no processes, monitors or links",
			Role::Monitor => "sees everything, changes nothing",
			Role::Operate => "also links and their units, media, the power profile and actions",
			Role::Admin => "everything, power and ending processes included",
		}
	}

	/// The roles as the CLI lists them: "glance|monitor|operate|admin".
	pub fn choices() -> String {
		Role::ALL.map(Role::as_str).join("|")
	}

	pub fn may(self, can: Can) -> bool {
		self >= can.role()
	}
}

/// One thing a phone may be allowed. `GET /api/v1/control` lists those this phone may do now
/// by these names (`allowed`), so the app can leave out the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Can {
	/// Processes in samples, with their commands and users.
	Processes,
	/// Monitors in samples.
	Displays,
	/// Links in samples and in `GET /api/v1/links`, the ports in use and the units to pick from.
	Links,
	/// Media players in samples, and their covers.
	MediaRead,
	/// The activity log.
	Log,
	/// Adding, changing and removing links. It only changes what the app lists, so it never
	/// needed control.
	LinksEdit,
	/// Starting, stopping and restarting the links' units.
	Units,
	/// Playing, pausing, skipping and seeking.
	Media,
	/// The computer's volume.
	Volume,
	/// The power profile.
	Profile,
	/// The actions in the settings.
	Actions,
	/// Shutting down, restarting, sleeping and locking.
	Power,
	/// Quitting and killing processes.
	Signal,
}

impl Can {
	pub const ALL: [Can; 13] = [
		Can::Processes,
		Can::Displays,
		Can::Links,
		Can::MediaRead,
		Can::Log,
		Can::LinksEdit,
		Can::Units,
		Can::Media,
		Can::Volume,
		Can::Profile,
		Can::Actions,
		Can::Power,
		Can::Signal,
	];

	pub fn name(self) -> &'static str {
		match self {
			Can::Processes => "processes",
			Can::Displays => "displays",
			Can::Links => "links",
			Can::MediaRead => "media.read",
			Can::Log => "log",
			Can::LinksEdit => "links.edit",
			Can::Units => "units",
			Can::Media => "media",
			Can::Volume => "volume",
			Can::Profile => "profile",
			Can::Actions => "actions",
			Can::Power => "power",
			Can::Signal => "signal",
		}
	}

	/// The lowest role that may.
	pub fn role(self) -> Role {
		match self {
			Can::Processes | Can::Displays | Can::Links | Can::MediaRead | Can::Log => Role::Monitor,
			Can::LinksEdit | Can::Units | Can::Media | Can::Volume | Can::Profile | Can::Actions => Role::Operate,
			Can::Power | Can::Signal => Role::Admin,
		}
	}

	/// Acts on the computer, so only while control is on.
	pub fn needs_control(self) -> bool {
		!matches!(self, Can::Processes | Can::Displays | Can::Links | Can::MediaRead | Can::Log | Can::LinksEdit)
	}
}

/// What a phone with `role` may do while control is `control`, by name.
pub fn allowed(role: Role, control: bool) -> Vec<&'static str> {
	Can::ALL.into_iter().filter(|c| role.may(*c) && (control || !c.needs_control())).map(Can::name).collect()
}

/// The paired phone a request comes from.
#[derive(Clone, Debug, PartialEq)]
pub struct Caller {
	pub id: String,
	pub name: String,
	pub role: Role,
	/// The entry its key came from (`store::family_of`), its own id for a phone paired with a
	/// code: the rate of acting and the share of the activity log are the family's.
	pub family: String,
}

/// A token bucket per phone for acting requests: `burst` in a row, then one per `refill`.
pub struct RateLimiter {
	burst: u32,
	refill: Duration,
	buckets: HashMap<String, Bucket>,
}

struct Bucket {
	tokens: f64,
	at: Instant,
	/// Refused since it last had a token, so only the first refusal of a run is logged.
	refused: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Rate {
	Allowed,
	/// Refused; `first` for the first refusal since the phone was last let through.
	Refused { first: bool },
}

impl Default for RateLimiter {
	fn default() -> Self {
		RateLimiter::new(limit::ACTS_BURST, cadence::ACT_REFILL)
	}
}

impl RateLimiter {
	pub fn new(burst: u32, refill: Duration) -> Self {
		RateLimiter { burst: burst.max(1), refill, buckets: HashMap::new() }
	}

	/// Takes a token for the phone `who` at `now`.
	pub fn take(&mut self, who: &str, now: Instant) -> Rate {
		let burst = self.burst as f64;
		let per_sec = 1.0 / self.refill.as_secs_f64().max(1e-3);
		// Phones whose buckets have filled up again are as good as new; forget them.
		self.buckets.retain(|id, b| id == who || b.tokens + now.saturating_duration_since(b.at).as_secs_f64() * per_sec < burst);
		let b = self.buckets.entry(who.to_string()).or_insert(Bucket { tokens: burst, at: now, refused: false });
		b.tokens = (b.tokens + now.saturating_duration_since(b.at).as_secs_f64() * per_sec).min(burst);
		b.at = now;
		if b.tokens >= 1.0 {
			b.tokens -= 1.0;
			b.refused = false;
			Rate::Allowed
		} else {
			let first = !b.refused;
			b.refused = true;
			Rate::Refused { first }
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn roles_are_ordered_and_named() {
		assert!(Role::Glance < Role::Monitor && Role::Monitor < Role::Operate && Role::Operate < Role::Admin);
		for r in Role::ALL {
			assert_eq!(Role::parse(r.as_str()), Some(r));
			assert_eq!(serde_json::to_value(r).unwrap(), r.as_str());
		}
		assert_eq!(Role::parse("Admin"), None);
		assert_eq!(Role::default(), Role::Admin);
		assert_eq!(Role::choices(), "glance|monitor|operate|admin");
	}

	#[test]
	fn what_each_role_is_allowed() {
		assert!(allowed(Role::Glance, true).is_empty());
		assert_eq!(allowed(Role::Monitor, true), ["processes", "displays", "links", "media.read", "log"]);
		assert_eq!(allowed(Role::Monitor, false), allowed(Role::Monitor, true));
		assert_eq!(
			allowed(Role::Operate, true),
			["processes", "displays", "links", "media.read", "log", "links.edit", "units", "media", "volume", "profile", "actions"]
		);
		// Without control only reading, and the list of links.
		assert_eq!(allowed(Role::Operate, false), ["processes", "displays", "links", "media.read", "log", "links.edit"]);
		assert_eq!(allowed(Role::Admin, false), allowed(Role::Operate, false));
		assert_eq!(allowed(Role::Admin, true).len(), Can::ALL.len());
		let names: Vec<&str> = Can::ALL.iter().map(|c| c.name()).collect();
		for (i, n) in names.iter().enumerate() {
			assert!(!names[..i].contains(n), "{n} twice");
		}
	}

	#[test]
	fn the_rate_limit_lets_a_burst_through_then_one_a_second() {
		let mut r = RateLimiter::new(10, Duration::from_secs(1));
		let t0 = Instant::now();
		for _ in 0..10 {
			assert_eq!(r.take("a", t0), Rate::Allowed);
		}
		assert_eq!(r.take("a", t0), Rate::Refused { first: true });
		assert_eq!(r.take("a", t0 + Duration::from_millis(500)), Rate::Refused { first: false });
		// Another phone has a bucket of its own.
		assert_eq!(r.take("b", t0), Rate::Allowed);
		// A second later, one more; then refused again, which counts as a new run.
		assert_eq!(r.take("a", t0 + Duration::from_millis(1000)), Rate::Allowed);
		assert_eq!(r.take("a", t0 + Duration::from_millis(1100)), Rate::Refused { first: true });
		// Idle long enough, the whole burst again, and never more than it.
		let later = t0 + Duration::from_secs(60);
		for _ in 0..10 {
			assert_eq!(r.take("a", later), Rate::Allowed);
		}
		assert!(matches!(r.take("a", later), Rate::Refused { .. }));
		// Full buckets are forgotten.
		r.take("c", later);
		assert!(!r.buckets.contains_key("b"));
	}
}
