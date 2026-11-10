//! The numbers the service runs by, grouped so they can be found and changed in one place.

/// Names and sizes other programs rely on: changing one breaks paired phones or the app.
pub mod protocol {
	/// The API the app speaks. Fields are only ever added; a change that breaks clients raises it.
	pub const API_VERSION: u32 = 1;
	/// The oldest app that understands this service; an older one asks to be updated.
	pub const MIN_APP: &str = "0.3.0";
	/// The port the service listens on unless the settings say otherwise.
	pub const DEFAULT_PORT: u16 = 47823;
	/// What the service announces itself as over mDNS.
	pub const SERVICE_TYPE: &str = "_elamon._tcp.local.";
	/// Hex digits of the certificate's fingerprint in the mDNS record, enough to tell computers apart.
	pub const MDNS_FINGERPRINT: usize = 16;
	/// Bus names of media players start with this; nothing else may be asked to play or show art.
	pub const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";
	/// Random bytes in the service's id (shown as hex).
	pub const SERVICE_ID_BYTES: usize = 16;
	/// Random bytes in a phone's token (shown as base64).
	pub const TOKEN_BYTES: usize = 32;
	/// Random bytes in a paired phone's id (shown as hex).
	pub const CLIENT_ID_BYTES: usize = 4;
	/// Random bytes in the id of a link added from the app (shown as hex).
	pub const LINK_ID_BYTES: usize = 6;
	/// Hex digits in the ids of configured actions and links, taken from their content.
	pub const CONTENT_ID_LEN: usize = 12;
	/// Digits in a pairing code.
	pub const PAIRING_DIGITS: u32 = 6;
}

/// How often things are read, as time rather than ticks: `Ticks` turns them into ticks of the
/// service's interval, so the same settings mean the same thing at 250 ms and at 10 s.
pub mod cadence {
	use std::time::Duration;

	/// Disk usage, display modes, user names and network addresses; sensors and the battery too
	/// while no app is connected.
	pub const SLOW: Duration = Duration::from_secs(5);
	/// A look for new sensors (a plugged-in drive, a GPU driver loaded late).
	pub const RESCAN: Duration = Duration::from_secs(60);
	/// Links and power profiles while an app is connected.
	pub const LINKS: Duration = Duration::from_secs(5);
	/// The volume, which costs running a command.
	pub const VOLUME: Duration = Duration::from_secs(2);
	/// `info` is rebuilt: GPU names improve once NVML has been read.
	pub const INFO: Duration = Duration::from_secs(10);
	/// The settings files are looked at for edits; SIGHUP rereads them at once.
	pub const SETTINGS_POLL: Duration = Duration::from_secs(5);
	/// `elamond pair` looks for the new phone.
	pub const PAIR_POLL: Duration = Duration::from_millis(500);
	/// A short command (wpctl, pactl) is checked for having finished.
	pub const COMMAND_POLL: Duration = Duration::from_millis(10);
	/// The service's own interval is kept within these, whatever the settings say.
	pub const INTERVAL_MIN: Duration = Duration::from_millis(250);
	pub const INTERVAL_MAX: Duration = Duration::from_secs(10);
	/// The slowest a stream may ask to be.
	pub const STREAM_INTERVAL_MAX: Duration = Duration::from_secs(60);
	/// Ticks a snapshot, or a stream's first sample, waits for a reading made for it.
	pub const FIRST_FRAME_TICKS: u32 = 3;
	/// What `elamond snapshot` measures rates over.
	pub const CLI_SNAPSHOT: Duration = Duration::from_secs(1);

	/// The cadences above in ticks of one interval, rounded, and at least every tick.
	#[derive(Clone, Copy, Debug, PartialEq, Eq)]
	pub struct Ticks {
		pub slow: u32,
		pub rescan: u32,
		pub links: u32,
		pub volume: u32,
		pub info: u32,
	}

	impl Ticks {
		pub fn new(interval: Duration) -> Self {
			let step = interval.as_millis().max(1);
			let n = |d: Duration| ((d.as_millis() + step / 2) / step).clamp(1, u32::MAX as u128) as u32;
			Ticks { slow: n(SLOW), rescan: n(RESCAN), links: n(LINKS), volume: n(VOLUME), info: n(INFO) }
		}
	}
}

/// How long things may take before the service gives up on them.
pub mod timeout {
	use std::time::Duration;

	/// One D-Bus round trip, and a whole read of logind, profiles, a unit or the players.
	pub const CALL: Duration = Duration::from_secs(3);
	/// First wait before connecting to a bus again after it failed; it doubles up to the next.
	pub const BUS_RETRY_MIN: Duration = Duration::from_secs(1);
	pub const BUS_RETRY_MAX: Duration = Duration::from_secs(60);
	/// A short command run for the volume.
	pub const COMMAND: Duration = Duration::from_secs(1);
	/// An action's own `timeout` in the settings is kept within these.
	pub const ACTION_MIN: Duration = Duration::from_secs(1);
	pub const ACTION_MAX: Duration = Duration::from_secs(3600);
	/// An action's timeout when the settings don't give one, in seconds.
	pub const ACTION_DEFAULT_SECS: u64 = 60;
	/// After an action's command exits, how long to keep reading what it left running.
	pub const ACTION_DRAIN: Duration = Duration::from_secs(1);
	/// A link's port is tried for this long to tell whether it answers.
	pub const PROBE: Duration = Duration::from_millis(250);
	/// `elamond pair` tries the service's port for this long to tell whether it runs.
	pub const SERVICE_CHECK: Duration = Duration::from_millis(300);
	/// Open connections get this long to finish when the service stops.
	pub const SHUTDOWN: Duration = Duration::from_secs(2);
	/// A pairing attempt without a pending code is answered after this, a wrong code after the next.
	pub const NO_PAIRING_DELAY: Duration = Duration::from_millis(500);
	pub const WRONG_CODE_DELAY: Duration = Duration::from_secs(1);
	/// A pairing code is good for this long after `elamond pair` prints it, in seconds.
	pub const PAIRING_TTL_SECS: u64 = 300;
	/// An idle connection is probed after this, then every `KEEPALIVE_INTERVAL`, and dropped
	/// after `KEEPALIVE_PROBES` unanswered probes: a phone that vanished stops counting as
	/// watching within about a minute.
	pub const KEEPALIVE_IDLE: Duration = Duration::from_secs(30);
	pub const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);
	pub const KEEPALIVE_PROBES: u32 = 3;
	/// The same bound for data sent and never acknowledged, which keepalive doesn't cover.
	pub const UNACKED: Duration = Duration::from_secs(60);
}

/// Sizes and counts that keep one phone, process or file from costing too much.
pub mod limit {
	/// Output kept from an action: its start and its end, so a chatty command can't flood the phone.
	pub const OUTPUT_MAX: usize = 16 * 1024;
	/// Of `OUTPUT_MAX`, the part kept from the start; the rest is the end.
	pub const OUTPUT_HEAD: usize = OUTPUT_MAX / 4;
	/// The largest cover image sent.
	pub const ART_MAX: usize = 8 * 1024 * 1024;
	/// A process's command line is cut to this many bytes.
	pub const CMD_MAX: usize = 400;
	/// The most processes one request may ask for.
	pub const PROCS_MAX: usize = 10_000;
	/// History is kept within these many readings, whatever the settings say.
	pub const HISTORY_MIN: usize = 10;
	pub const HISTORY_MAX: usize = 7200;
	/// Characters in a phone's or a link's name.
	pub const NAME_MAX: usize = 64;
	/// Bytes in a link's URL.
	pub const URL_MAX: usize = 512;
	/// Bytes in a systemd unit's name.
	pub const UNIT_MAX: usize = 256;
	/// Wrong guesses before a pairing code is thrown away.
	pub const PAIRING_ATTEMPTS: u32 = 5;
	/// Processes shown by `elamond snapshot`.
	pub const SNAPSHOT_TOP: usize = 10;
}

#[cfg(test)]
mod tests {
	use super::cadence::Ticks;
	use std::time::Duration;

	#[test]
	fn cadences_follow_the_interval() {
		let t = Ticks::new(Duration::from_secs(1));
		assert_eq!((t.slow, t.rescan, t.links, t.volume, t.info), (5, 60, 5, 2, 10));
		let t = Ticks::new(Duration::from_millis(250));
		assert_eq!((t.slow, t.volume), (20, 8));
		// Slower than a cadence: every tick.
		let t = Ticks::new(Duration::from_secs(10));
		assert_eq!((t.slow, t.rescan, t.volume, t.info), (1, 6, 1, 1));
	}
}
