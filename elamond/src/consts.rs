//! The numbers the service runs by, grouped so they can be found and changed in one place.

/// Names and sizes other programs rely on: changing one breaks paired phones or the app.
pub mod protocol {
	/// The API the app speaks. Fields are only ever added; a change that breaks clients raises it.
	pub const API_VERSION: u32 = 1;
	/// The oldest app that understands this service; an older one asks to be updated.
	pub const MIN_APP: &str = "0.4.0";
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
	/// Addresses and names in a pairing link: the app reads no more than these.
	pub const PAIR_HOSTS_MAX: usize = 8;
	/// What a statement that one certificate follows another starts with, and what its `format`
	/// says: the version of the text signed. The app follows only those it knows.
	pub const ROTATION_FORMAT: &str = "elamon-rotate-v1";
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
	/// The settings files, and `elamond control off`'s file, are looked at for edits; SIGHUP
	/// rereads them at once. Acting requests look at `control off`'s file themselves.
	pub const SETTINGS_POLL: Duration = Duration::from_secs(5);
	/// The network interfaces are looked at for addresses to listen and announce on, and the
	/// data directory for a certificate from `elamond rotate-cert`; SIGHUP looks at once.
	pub const NETWORK_POLL: Duration = Duration::from_secs(5);
	/// `elamond pair` looks for the new phone.
	pub const PAIR_POLL: Duration = Duration::from_millis(500);
	/// A phone that used up `limit::ACTS_BURST` may make one more acting request per this.
	pub const ACT_REFILL: Duration = Duration::from_secs(1);
	/// One stream of a phone is logged per this, as it opens and as it closes; those the phone
	/// opens in between are only counted, in the next line about it.
	pub const STREAM_LOG_QUIET: Duration = Duration::from_secs(10);
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
	/// `elamond pair` waits this long for `tailscale status`, for the computer's Tailscale name.
	pub const TAILSCALE: Duration = Duration::from_millis(1500);
	/// Open connections get this long to finish when the service stops.
	pub const SHUTDOWN: Duration = Duration::from_secs(2);
	/// The service waits this long for the data directory while `elamond restore` writes to it,
	/// looking every `DATA_DIR_LOCK_POLL`, and then gives up: another service holds it.
	pub const DATA_DIR_LOCK: Duration = Duration::from_secs(10);
	pub const DATA_DIR_LOCK_POLL: Duration = Duration::from_millis(100);
	/// A pairing attempt without a pending code is answered after this, a wrong code after the next.
	pub const NO_PAIRING_DELAY: Duration = Duration::from_millis(500);
	pub const WRONG_CODE_DELAY: Duration = Duration::from_secs(1);
	/// A pairing code is good for this long after `elamond pair` prints it, in seconds.
	pub const PAIRING_TTL_SECS: u64 = 300;
	/// A phone that gave the right code waits this long, in seconds, for the person at
	/// `elamond pair` to let it in; then it is turned away. The app waits a little longer.
	pub const PAIR_APPROVAL_SECS: u64 = 90;
	/// A new `elamond pair` waits this long for one it replaced to let go of its lock.
	pub const PAIR_TAKE_OVER: Duration = Duration::from_secs(5);
	/// An idle connection is probed after this, then every `KEEPALIVE_INTERVAL`, and dropped
	/// after `KEEPALIVE_PROBES` unanswered probes: a phone that vanished stops counting as
	/// watching within about a minute.
	pub const KEEPALIVE_IDLE: Duration = Duration::from_secs(30);
	pub const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);
	pub const KEEPALIVE_PROBES: u32 = 3;
	/// The same bound for data sent and never acknowledged, which keepalive doesn't cover.
	pub const UNACKED: Duration = Duration::from_secs(60);
	/// A new connection's TLS handshake, then its first request, then the headers of each request
	/// (an idle connection between requests included): a client that says nothing, or says it a
	/// byte at a time, doesn't hold a connection for longer.
	pub const TLS_HANDSHAKE: Duration = Duration::from_secs(10);
	pub const FIRST_REQUEST: Duration = Duration::from_secs(15);
	pub const HEADER_READ: Duration = Duration::from_secs(15);
}

/// Sizes and counts that keep one phone, process or file from costing too much.
pub mod limit {
	/// Output kept from an action: its start and its end, so a chatty command can't flood the phone.
	pub const OUTPUT_MAX: usize = 16 * 1024;
	/// Of `OUTPUT_MAX`, the part kept from the start; the rest is the end.
	pub const OUTPUT_HEAD: usize = OUTPUT_MAX / 4;
	/// The largest cover image sent: the app takes no larger.
	pub const ART_MAX: usize = 4 * 1024 * 1024;
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
	/// Wrong guesses, from anywhere, before a pairing code is thrown away.
	pub const PAIRING_ATTEMPTS: u32 = 20;
	/// Wrong guesses from one address before it gets 429 for the rest of the code's life. An
	/// IPv6 address counts by its first `PAIRING_V6_PREFIX` bits: one network hands out many.
	pub const PAIRING_ATTEMPTS_PER_ADDRESS: u32 = 5;
	pub const PAIRING_V6_PREFIX: u32 = 64;
	/// Acting requests one phone may make in a row; then one more for each `cadence::ACT_REFILL`.
	pub const ACTS_BURST: u32 = 10;
	/// Entries kept in the activity log, in memory and in its file.
	pub const ACTIVITY_MAX: usize = 500;
	/// Of those, the most one phone's family (it and the keys of its own it asked for) keeps:
	/// its newest push out its oldest, never another phone's.
	pub const ACTIVITY_PER_FAMILY: usize = 100;
	/// Phones paired at once, keys of their own included; pairing and `rekey` refuse more.
	pub const PHONES_MAX: usize = 64;
	/// Lines the activity log's file grows to before it is rewritten with the last `ACTIVITY_MAX`.
	pub const ACTIVITY_FILE_LINES: usize = 2 * ACTIVITY_MAX;
	/// Characters in what an activity log entry says was asked, and in its message.
	pub const WHAT_MAX: usize = 200;
	/// Processes shown by `elamond snapshot`.
	pub const SNAPSHOT_TOP: usize = 10;
	/// Certificate rotations kept, the newest last: a phone that missed more than these pairs again.
	pub const ROTATIONS_KEPT: usize = 5;
	/// Addresses `elamond pair` tries, to tell whether the service runs.
	pub const SERVICE_CHECK_ADDRS: usize = 3;
	/// Bytes in the body of a request that may come before a phone is known (`pair`), or that
	/// only names one (`rekey`).
	pub const PAIR_BODY_MAX: usize = 4 * 1024;
	/// Connections the service holds at once; more are closed as they come. The systemd units give
	/// it far more descriptors than these take.
	pub const CONNECTIONS_MAX: usize = 256;
	/// Connections from one address (an IPv6 one by its /64) that haven't shown a paired phone's
	/// key yet: pairing, `hello`, or nothing at all.
	pub const STRANGERS_PER_ADDRESS: usize = 16;
}

/// `elamond backup`: how a passphrase becomes the key a backup is sealed with, and how much a
/// backup may ask of the computer that opens it.
pub mod backup {
	/// Argon2id's cost: memory in KiB, passes and lanes. RFC 9106's second recommended setting,
	/// for when 2 GiB of memory is too much: 64 MiB, 3 passes, 4 lanes.
	pub const ARGON2_M_KIB: u32 = 64 * 1024;
	pub const ARGON2_T: u32 = 3;
	pub const ARGON2_P: u32 = 4;
	/// Argon2's version, 1.3.
	pub const ARGON2_VERSION: u32 = 0x13;
	/// Random bytes of salt for the key, and of nonce for XChaCha20-Poly1305.
	pub const SALT_BYTES: usize = 16;
	pub const NONCE_BYTES: usize = 24;
	/// The most a backup may ask for to be opened, so a file can't make a restore take all the
	/// memory or run for long: 256 MiB, 10 passes, 8 lanes, well above what `elamond backup` asks.
	pub const ARGON2_M_KIB_MAX: u32 = 256 * 1024;
	pub const ARGON2_T_MAX: u32 = 10;
	pub const ARGON2_P_MAX: u32 = 8;
	/// The shortest passphrase a backup is made with, in characters.
	pub const PASSPHRASE_MIN: usize = 8;
	/// Tries at typing a passphrase twice alike before giving up.
	pub const PASSPHRASE_TRIES: usize = 3;
	/// The largest file `elamond restore` reads: far more than the state ever takes.
	pub const FILE_MAX: u64 = 64 * 1024 * 1024;
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
