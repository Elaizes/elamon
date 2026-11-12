//! elamond: serves this computer's dashboards to the Elamon app, and runs what it is allowed to.

mod access;
mod actions;
mod activity;
mod backup;
mod bus;
mod collect;
mod commands;
mod consts;
mod control;
mod errors;
#[cfg(test)]
mod fixtures;
mod format;
mod history;
mod identity;
mod links;
mod listen;
mod mdns;
mod model;
mod names;
mod server;
mod settings;
mod setup;
mod states;
mod store;
mod stream;
mod ui;
mod values;

use std::io::IsTerminal;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum_server::tls_rustls::RustlsConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::sync::watch;

use consts::cadence::{self, Ticks};
use access::Role;
use consts::{limit, timeout};

const HELP: &str = "\
elamond — serves this computer's metrics to the Elamon app

Usage:
  elamond serve                        run the service (what the systemd unit does)
  elamond pair [--role ROLE] [--profile ID] [--yes]
                                       show a code and QR to pair a phone, and ask before the phone
                                       that gives it is let in (--yes: without asking)
  elamond clients                      list paired phones: role, profile and app
  elamond clients role <id|all> ROLE   change what a phone may do; --with-added: and the phones
                                       that got a key of their own with its token
  elamond clients profile <id|all> ID  change how a phone looks and behaves (a profile of the
                                       settings); --with-added as above
  elamond revoke <id|all>              unpair a phone; --with-added: and those phones too
  elamond control [off|on|status]      hold control off for every phone, or let go
  elamond rotate-cert                  a new certificate, which paired phones follow (routine renewal)
  elamond rotate-cert --no-statement   a new certificate nothing vouches for, after a leaked key:
                                       phones pair again
  elamond backup FILE                  save the identity, the paired phones and the rest to FILE,
                                       encrypted with a passphrase (--plain: not encrypted)
  elamond restore FILE                 put a backup back, the service stopped (--force: running);
                                       --with-settings puts elamond.toml and elamond.d back too
  elamond snapshot                     print one reading as JSON
  elamond check [FILE]                 read the settings as the service would (with FILE in its
                                       place) and name the file and line of each mistake
  elamond edit [FILE]                  edit elamond.toml (or FILE) in $VISUAL or $EDITOR; a broken
                                       one isn't kept, and the service takes up a sound one
  elamond schema                       print the JSON Schema of the settings, for editors
  elamond defaults                     print the built-in dashboards and profile, to start from
  elamond values                       list the values this computer has now, and the collections
  elamond setup                        install the service for this user (systemd user unit, starter
                                       settings) and print what needs root: lingering, the firewall,
                                       the polkit rule (--print: only show it; --no-start)
  elamond setup --check                tell whether the service, and what it uses, works here
  elamond --version

Roles, each allowing what the one before does and more:
  glance   the basics: CPU, memory, temperatures, disks, network and battery
  monitor  everything there is to see: processes, monitors, links, media, the log
  operate  links and their units, media, the power profile and actions
  admin    power and ending processes

Settings: ~/.config/elamon/elamond.toml   State: ~/.local/share/elamon/
";

fn main() {
	let args: Vec<String> = std::env::args().skip(1).collect();
	let command = args.first().map(String::as_str).unwrap_or("serve");
	// Rust ignores SIGPIPE, so `elamond clients | head` would panic once head has had
	// enough. The commands end quietly like other tools instead; the service keeps it
	// ignored, a phone that hangs up mid-write is an error to handle, not a reason to die.
	if command != "serve" {
		// SAFETY: restoring a signal's default disposition before any thread starts.
		unsafe {
			libc::signal(libc::SIGPIPE, libc::SIG_DFL);
		}
	}
	let result = match command {
		"serve" => serve(),
		"pair" => pair(&args[1..]),
		"clients" => clients(&args[1..]),
		"control" => control(args.get(1).map(String::as_str)),
		"rotate-cert" => rotate_cert(&args[1..]),
		"backup" => backup::backup(&args[1..]),
		"restore" => backup::restore(&args[1..]),
		"revoke" => revoke(&args[1..]),
		"snapshot" => snapshot(),
		"check" => commands::check(&args[1..]),
		"edit" => commands::edit(&args[1..]),
		"schema" => commands::schema(),
		"defaults" => commands::defaults(),
		"values" => commands::values(),
		"setup" => setup::run(&args[1..]),
		"-V" | "--version" | "version" => {
			println!("elamond {}", env!("CARGO_PKG_VERSION"));
			Ok(())
		}
		"-h" | "--help" | "help" => {
			print!("{HELP}");
			Ok(())
		}
		other => Err(format!("unknown command \"{other}\"\n\n{HELP}")),
	};
	if let Err(e) = result {
		eprintln!("elamond: {e}");
		std::process::exit(1);
	}
}

fn machine_name(config: &store::Config) -> String {
	let name = config.name.trim();
	if !name.is_empty() {
		return name.to_string();
	}
	sysinfo::System::host_name().unwrap_or_else(|_| "Computer".into())
}

/// The tick and the points of history kept, from the settings within sane bounds, with a
/// note for each setting that had to be changed to fit them.
fn timing(config: &store::Config) -> (Duration, usize, Vec<String>) {
	let mut notes = Vec::new();
	let asked = Duration::from_millis(config.interval_ms);
	let interval = asked.clamp(cadence::INTERVAL_MIN, cadence::INTERVAL_MAX);
	if interval != asked {
		notes.push(format!("interval_ms = {} is out of range; using {}", config.interval_ms, interval.as_millis()));
	}
	let points = config.history_minutes.saturating_mul(60_000) / interval.as_millis() as u64;
	let history_len = points.clamp(limit::HISTORY_MIN as u64, limit::HISTORY_MAX as u64) as usize;
	if history_len as u64 != points {
		let kept = interval * history_len as u32;
		notes.push(format!(
			"history_minutes = {} makes {points} readings at {} ms, outside {}–{}; keeping {history_len}, {} s",
			config.history_minutes,
			interval.as_millis(),
			limit::HISTORY_MIN,
			limit::HISTORY_MAX,
			kept.as_secs()
		));
	}
	(interval, history_len, notes)
}

fn serve() -> Result<(), String> {
	let dir = store::data_dir();
	// Links the 0.5 app added go into the settings before they are read, once.
	links::migrate(&dir, &store::config_dir(), store::load_config().ok().as_ref());
	let config = store::load_config()?;
	// Held until the service stops: `elamond restore` sees it, and the identity isn't read while
	// a restore writes it.
	let _held = hold_data_dir(&dir)?;
	let identity = identity::Identity::load_or_create(&dir)?;
	let name = machine_name(&config);
	let (interval, history_len, notes) = timing(&config);
	for n in notes {
		eprintln!("settings: {n}");
	}
	let tls = tls_config(&identity, &dir)?;

	// The port first: if it's taken, nothing else is started or announced. An address that isn't
	// the computer's yet, like an interface without one, is waited for.
	let ifaces = listen::interfaces();
	let plan = listen::Plan::of(&config.listen, &ifaces);
	let mut ready = Vec::new();
	for spot in &plan.spots {
		match listen::bind(spot.addr, config.port) {
			Ok(l) => ready.push((spot.clone(), l)),
			Err(e) if listen::not_here(&e) => {}
			Err(e) => return Err(format!("can't listen on {}: {e}", SocketAddr::new(spot.addr, config.port))),
		}
	}
	eprintln!(
		"elamond {} on port {}, certificate {}",
		env!("CARGO_PKG_VERSION"),
		config.port,
		store::pretty_fingerprint(&identity.fingerprint)
	);
	let held_off = store::control_held_off(&dir);
	if config.control && held_off {
		eprintln!("control is held off by `elamond control off`; `elamond control on` lets phones act again");
	} else if config.control {
		eprintln!("control is on: paired phones may act on this computer, as far as their roles allow");
	}

	let every = Ticks::new(interval);
	let mut collector = collect::Collector::new(every);
	// This reading only primes the CPU counters. sysinfo ignores a second one within its
	// minimum interval, which would leave the first published usage at zero, so wait that out.
	collector.read(false, ui::Sources::default());
	std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
	let (tx, rx) = watch::channel(Arc::new(server::Frame::empty()));
	let hub = Arc::new(server::Hub::new(
		identity.id.clone(),
		name.clone(),
		rx,
		history::Ring::new(history_len),
		interval,
		Arc::new(config.clone()),
		dir.clone(),
	));
	hub.set_rotations(identity::chain_to(&served_rotations(&dir), &identity.fingerprint));

	let runtime = tokio::runtime::Builder::new_multi_thread()
		.worker_threads(2)
		.enable_all()
		.build()
		.map_err(|e| e.to_string())?;

	{
		let hub = hub.clone();
		let id = identity.id.clone();
		let name = name.clone();
		let rt = runtime.handle().clone();
		std::thread::Builder::new()
			.name("collector".into())
			.spawn(move || collect_loop(collector, hub, tx, every, id, name, history_len, rt))
			.map_err(|e| e.to_string())?;
	}

	runtime.block_on(async move {
		tokio::spawn(watch_settings(hub.clone()));
		let app = server::router(hub.clone()).into_make_service_with_connect_info::<SocketAddr>();
		let tls = RustlsConfig::from_config(Arc::new(tls));
		let mut listeners = listen::Listeners::new(config.port, tls.clone(), app, listen::Limits::SERVICE);
		for (spot, l) in ready {
			listeners.serve(spot, l);
		}
		let mut upkeep = Upkeep {
			listeners,
			announcer: config.mdns.then(|| mdns::Announcer::new(&name, &identity.id, &identity.fingerprint, config.port)),
			targets: config.listen.clone(),
			tls,
			hub,
			stamp: identity::stamp(&dir),
			failed: None,
			dir,
		};
		upkeep.network(&ifaces);
		tokio::select! {
			_ = shutdown_signal() => {}
			_ = upkeep.run() => {}
		}
		upkeep.listeners.shutdown(timeout::SHUTDOWN).await;
		Ok(())
	})
}

/// The data directory's lock, waited for while `elamond restore` writes there; refused when
/// another service holds it. Where locks don't work, the service goes on without, and says so.
fn hold_data_dir(dir: &std::path::Path) -> Result<Option<std::fs::File>, String> {
	let started = std::time::Instant::now();
	loop {
		match store::lock_data_dir(dir) {
			Ok(Some(lock)) => return Ok(Some(lock)),
			Ok(None) if started.elapsed() < timeout::DATA_DIR_LOCK => std::thread::sleep(timeout::DATA_DIR_LOCK_POLL),
			Ok(None) => return Err(format!("another elamond serve uses {}; stop that one first", dir.display())),
			Err(e) => {
				eprintln!("can't lock {} ({e}); elamond restore won't see that the service runs", dir.display());
				return Ok(None);
			}
		}
	}
}

/// TLS with the certificate and key of `identity`, from the data directory `dir`.
fn tls_config(identity: &identity::Identity, dir: &std::path::Path) -> Result<rustls::ServerConfig, String> {
	tls_with(&identity.cert_der, &identity.key_der).map_err(|e| {
		format!(
			"the TLS identity in {} doesn't load ({e}); remove cert.der and key.der to make a new one, then pair the phones again",
			dir.display()
		)
	})
}

/// TLS presenting the certificate `cert` with its key `key`, both DER: refused when the key isn't
/// the certificate's.
fn tls_with(cert: &[u8], key: &[u8]) -> Result<rustls::ServerConfig, String> {
	let provider = Arc::new(rustls::crypto::ring::default_provider());
	let cert = CertificateDer::from(cert.to_vec());
	let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.to_vec()));
	let mut tls = rustls::ServerConfig::builder_with_provider(provider)
		.with_safe_default_protocol_versions()
		.map_err(|e| e.to_string())?
		.with_no_client_auth()
		.with_single_cert(vec![cert], key)
		.map_err(|e| e.to_string())?;
	tls.alpn_protocols = vec![b"http/1.1".to_vec()];
	Ok(tls)
}

/// The rotations kept, or none, said so, when their file can't be read.
fn served_rotations(dir: &std::path::Path) -> Vec<identity::Rotation> {
	identity::rotations(dir).unwrap_or_else(|e| {
		eprintln!("certificate: {e}\n  phones that meet a new certificate can't follow it and pair again");
		Vec::new()
	})
}

/// What the service keeps in step with the computer while it runs: the addresses it listens on
/// and where it is announced, as interfaces come and go, and the certificate it presents, after
/// `elamond rotate-cert`.
struct Upkeep {
	listeners: listen::Listeners,
	announcer: Option<mdns::Announcer>,
	targets: Vec<listen::Target>,
	tls: RustlsConfig,
	hub: Arc<server::Hub>,
	dir: PathBuf,
	/// cert.der as last taken up, and as last found not to load.
	stamp: Option<identity::Stamp>,
	failed: Option<identity::Stamp>,
}

impl Upkeep {
	/// Every `cadence::NETWORK_POLL`, and at once on SIGHUP.
	async fn run(&mut self) {
		use tokio::signal::unix::{signal, SignalKind};
		let mut hangup = signal(SignalKind::hangup()).ok();
		loop {
			let hup = async {
				match hangup.as_mut() {
					Some(h) => drop(h.recv().await),
					None => std::future::pending().await,
				}
			};
			tokio::select! {
				_ = tokio::time::sleep(cadence::NETWORK_POLL) => {}
				_ = hup => {}
			}
			self.certificate();
			self.network(&listen::interfaces());
		}
	}

	fn network(&mut self, ifaces: &[listen::Iface]) {
		let plan = listen::Plan::of(&self.targets, ifaces);
		self.listeners.update(&plan);
		if let Some(a) = &mut self.announcer {
			a.update(plan.mdns(ifaces));
		}
	}

	/// Presents a new certificate from the next connection on; those open keep theirs.
	fn certificate(&mut self) {
		let now = identity::stamp(&self.dir);
		if now == self.stamp || now == self.failed {
			return;
		}
		let loaded = identity::Identity::load(&self.dir).and_then(|id| Ok((tls_config(&id, &self.dir)?, id)));
		match loaded {
			Ok((tls, id)) => {
				self.tls.reload_from_config(Arc::new(tls));
				self.hub.set_rotations(identity::chain_to(&served_rotations(&self.dir), &id.fingerprint));
				if let Some(a) = &mut self.announcer {
					a.set_fingerprint(&id.fingerprint);
				}
				eprintln!("certificate: now {}", store::pretty_fingerprint(&id.fingerprint));
				self.stamp = now;
				self.failed = None;
			}
			Err(e) => {
				eprintln!("certificate: the one in {} now doesn't load ({e}); presenting the one before", self.dir.display());
				self.failed = now;
			}
		}
	}
}

/// Rereads the settings on SIGHUP, or when elamond.toml or a drop-in changes. Files that
/// don't parse leave the running settings as they were, but control off until they parse again.
/// Looks at `elamond control off`'s file as often.
async fn watch_settings(hub: Arc<server::Hub>) {
	use tokio::signal::unix::{signal, SignalKind};
	// Without SIGHUP, `systemctl reload` does nothing, but edits are still found by looking.
	let mut hangup = match signal(SignalKind::hangup()) {
		Ok(h) => Some(h),
		Err(e) => {
			eprintln!("settings: SIGHUP can't be caught here ({e}); edits are found within {} s", cadence::SETTINGS_POLL.as_secs());
			None
		}
	};
	let mut stamp = store::config_stamp();
	loop {
		let hup = async {
			match hangup.as_mut() {
				Some(h) => h.recv().await.is_some(),
				None => std::future::pending().await,
			}
		};
		let asked = tokio::select! {
			got = hup => got,
			_ = tokio::time::sleep(cadence::SETTINGS_POLL) => false,
		};
		hub.sync_control_off();
		let now = store::config_stamp();
		if !asked && now == stamp {
			continue;
		}
		stamp = now;
		match store::load_config() {
			Ok(c) => {
				eprintln!("settings: reread");
				hub.set_config(c);
			}
			Err(e) => hub.settings_failed(e),
		}
	}
}

async fn shutdown_signal() {
	use tokio::signal::unix::{signal, SignalKind};
	let mut term = signal(SignalKind::terminate()).expect("signal handler");
	tokio::select! {
		_ = tokio::signal::ctrl_c() => {}
		_ = term.recv() => {}
	}
}

/// Ends the service if the collector thread panics: without it the service would answer with
/// stale data forever, and exiting lets systemd start it again.
struct ExitOnPanic;

impl Drop for ExitOnPanic {
	fn drop(&mut self) {
		if std::thread::panicking() {
			eprintln!("collector: stopped by the panic above; exiting");
			std::process::exit(1);
		}
	}
}

#[allow(clippy::too_many_arguments)]
fn collect_loop(
	mut collector: collect::Collector,
	hub: Arc<server::Hub>,
	tx: watch::Sender<Arc<server::Frame>>,
	every: Ticks,
	id: String,
	name: String,
	history_len: usize,
	rt: tokio::runtime::Handle,
) {
	let _exit = ExitOnPanic;
	let mut info = collector.info(&id, &name, hub.interval, history_len);
	let mut extras = control::Extras::new(every);
	let mut seq: u64 = 0;
	// What the history ring keeps, and the settings and shape it was worked out for.
	let mut tracked: (Option<(u64, u64)>, std::collections::BTreeSet<String>) = (None, Default::default());
	loop {
		let started = std::time::Instant::now();
		let wanted = hub.wanted();
		let need = wanted.unwrap_or_default();
		let mut reading = collector.read(wanted.is_some(), need);
		let got = extras.apply(&hub, &rt, wanted.is_some(), need, &mut reading.sample);
		// GPU names improve once NVML has been read, so the facts are read again now and then.
		if reading.cards_changed || seq % u64::from(every.info) == 1 % u64::from(every.info) {
			info = collector.info(&id, &name, hub.interval, history_len);
		}
		let config = hub.config();
		let activity = hub.activity.newest(limit::ACTIVITY_MAX);
		let values = values::read(&values::Reading {
			sample: &reading.sample,
			info: &info,
			media: got.media.as_ref(),
			processes: reading.processes.as_deref(),
			activity: &activity,
			actions: &config.actions,
		});
		let shape = server::shape_of(&values, &got.caps);
		let stamp = Some((hub.config_gen(), shape));
		if tracked.0 != stamp {
			tracked = (stamp, ui::tracked(&config, &values));
		}
		if let Ok(mut h) = hub.history.lock() {
			h.push(&values, &tracked.1);
		}
		let read = ui::Sources {
			processes: reading.processes.is_some(),
			gpus: need.gpus,
			displays: reading.displays,
			..got.read
		};
		let mut features = vec!["processes"];
		features.extend([
			(got.caps.poweroff || got.caps.reboot || got.caps.suspend || got.caps.lock, "power"),
			(!got.caps.power_profiles.is_empty(), "power_profiles"),
			(got.caps.units, "units"),
			(got.caps.media, "media"),
			(collector.has_cards(), "displays"),
			(collector.nvml(), "nvml"),
		]
		.into_iter()
		.filter_map(|(on, f)| on.then_some(f)));
		seq = seq.wrapping_add(1);
		let frame = server::Frame { seq, values: Arc::new(values), shape, caps: Arc::new(got.caps), read, features: Arc::new(features) };
		tx.send_replace(Arc::new(frame));
		std::thread::sleep(hub.interval.saturating_sub(started.elapsed()));
	}
}

/// Set by Ctrl+C, SIGTERM or a closed terminal while `elamond pair` waits.
static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_stop(_: libc::c_int) {
	STOP.store(true, Ordering::SeqCst);
}

/// Whether someone can answer `elamond pair` on its standard input: a terminal, or what a person
/// or a script deliberately gives it (a pipe, a FIFO, a file); not /dev/null or nothing, as under
/// systemd or cron.
fn can_answer() -> bool {
	if std::io::stdin().is_terminal() {
		return true;
	}
	let mut st = std::mem::MaybeUninit::<libc::stat>::uninit();
	// SAFETY: fstat fills the stat it is given, or fails and it is left unread.
	if unsafe { libc::fstat(0, st.as_mut_ptr()) } != 0 {
		return false;
	}
	// SAFETY: filled by fstat above.
	let kind = unsafe { st.assume_init() }.st_mode & libc::S_IFMT;
	matches!(kind, libc::S_IFIFO | libc::S_IFSOCK | libc::S_IFREG)
}

/// Lines from standard input as they are typed, and `None` at its end, read on a thread of their
/// own so waiting for the phone goes on meanwhile.
fn answers() -> std::sync::mpsc::Receiver<Option<String>> {
	let (tx, rx) = std::sync::mpsc::channel();
	let _ = std::thread::Builder::new().name("answers".into()).spawn(move || {
		use std::io::BufRead;
		let mut line = String::new();
		loop {
			line.clear();
			let got = match std::io::stdin().lock().read_line(&mut line) {
				Ok(0) | Err(_) => None,
				Ok(_) => Some(line.trim().to_string()),
			};
			let end = got.is_none();
			if tx.send(got).is_err() || end {
				return;
			}
		}
	});
	rx
}

/// What the person at `elamond pair` said to a phone that gave the right code.
enum Answer {
	Allow,
	Deny,
	/// Not in time: the service turns it away by itself.
	Late,
	/// The phone stopped waiting.
	Gone,
	/// `elamond pair` was stopped.
	Stopped,
}

/// Shows the phone `ask` to the person at the terminal and waits for a yes or a no, until the
/// service stops waiting for it. Anything but yes is no, the end of the input too.
fn ask_person(dir: &std::path::Path, ask: &store::Ask, role: Role, code: &str, lines: &std::sync::mpsc::Receiver<Option<String>>) -> Answer {
	use std::io::Write;
	use std::sync::mpsc::RecvTimeoutError;
	// What was typed before the question isn't an answer to it.
	while lines.try_recv().is_ok() {}
	print!(
		"  Phone \"{}\" from {} asks to pair as {}. Allow? [y/N] ",
		store::sanitize_name(&ask.name),
		store::sanitize_name(&ask.address),
		role.as_str()
	);
	let _ = std::io::stdout().flush();
	let shown = !std::io::stdin().is_terminal();
	let left = (ask.at + timeout::PAIR_APPROVAL_SECS).saturating_sub(store::now());
	let deadline = std::time::Instant::now() + Duration::from_secs(left);
	loop {
		match lines.recv_timeout(Duration::from_millis(250)) {
			Ok(Some(line)) => {
				if shown {
					println!("{line}");
				}
				return if matches!(line.to_ascii_lowercase().as_str(), "y" | "yes") { Answer::Allow } else { Answer::Deny };
			}
			Ok(None) | Err(RecvTimeoutError::Disconnected) => {
				println!();
				return Answer::Deny;
			}
			Err(RecvTimeoutError::Timeout) => {}
		}
		if STOP.load(Ordering::SeqCst) {
			println!();
			return Answer::Stopped;
		}
		if std::time::Instant::now() >= deadline {
			println!();
			return Answer::Late;
		}
		if !store::read_pairing(dir).is_some_and(|p| p.code == code && p.ask.is_some_and(|a| a.id == ask.id)) {
			println!();
			return Answer::Gone;
		}
	}
}

fn pair(args: &[String]) -> Result<(), String> {
	let config = store::load_config()?;
	let usage = || format!("usage: elamond pair [--role {}] [--profile ID] [--yes]", Role::choices());
	let (mut role, mut profile, mut yes) = (None, None, false);
	let mut rest = args.iter().map(String::as_str);
	while let Some(a) = rest.next() {
		match a {
			"--yes" => yes = true,
			"--role" => role = Some(role_arg(rest.next().ok_or_else(usage)?)?),
			"--profile" => profile = Some(profile_arg(&config, rest.next().ok_or_else(usage)?)?),
			_ if a.starts_with("--role=") => role = Some(role_arg(&a["--role=".len()..])?),
			_ if a.starts_with("--profile=") => profile = Some(profile_arg(&config, &a["--profile=".len()..])?),
			_ => return Err(usage()),
		}
	}
	let role_given = role.is_some();
	let role = role.unwrap_or(config.default_role);
	if !yes && !can_answer() {
		return Err("elamond pair asks you to let in the phone that gives its code, and there's no terminal to ask on.\n  \
			 To let in whichever phone gives the code without asking, as a script would: elamond pair --yes"
			.into());
	}
	let dir = store::data_dir();
	let identity = identity::Identity::load_or_create(&dir)?;
	let name = machine_name(&config);
	let before: Vec<String> = match store::load_clients() {
		Ok(list) => list.into_iter().map(|c| c.id).collect(),
		Err(e) => {
			eprintln!("  ! {e}\n    pairing sets the damaged file aside and starts a new list");
			Vec::new()
		}
	};
	let pairing = store::new_pairing(&dir, role, profile.as_deref(), !yes).map_err(|e| format!("can't save the pairing code: {e}"))?;
	// Held while this runs: the service turns a phone away when no one is here to answer. Another
	// `elamond pair` whose code this one just replaced lets go of it within moments.
	let _held = match store::hold_pairing(&dir, timeout::PAIR_TAKE_OVER) {
		Ok(Some(lock)) => Some(lock),
		Ok(None) => {
			store::clear_pairing_code(&dir, &pairing.code);
			return Err("another elamond pair is still running; stop it first".into());
		}
		// Where locks don't work, a phone can only be let in with --yes.
		Err(e) if yes => {
			eprintln!("  ! can't lock {}: {e}", dir.display());
			None
		}
		Err(e) => {
			store::clear_pairing_code(&dir, &pairing.code);
			return Err(format!("can't lock {}: {e}; elamond pair --yes pairs without asking", dir.display()));
		}
	};
	// From here on, stopping withdraws the code instead of leaving it to be guessed at.
	for sig in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
		// SAFETY: the handler only stores to an atomic, which is async-signal-safe.
		unsafe { libc::signal(sig, on_stop as *const () as libc::sighandler_t) };
	}
	let lines = (!yes).then(answers);
	let new_phone = || store::load_clients().ok()?.into_iter().find(|c| !before.contains(&c.id));

	// Where the service listens, as the phone can reach it: the local network first, then the
	// names that stay when addresses change.
	let ifaces = listen::interfaces();
	let plan = listen::Plan::of(&config.listen, &ifaces);
	let offered = names::Offered::of(&plan.pairing_hosts(&ifaces), &config.names, names::tailscale(&plan, &ifaces).as_deref());
	let hosts = offered.hosts();
	let uri = pair_link(&identity, &name, config.port, &pairing.code, &hosts);

	let tty = std::io::stdout().is_terminal();
	// Without an address or a name in it, the link would only be refused.
	if let Some(qr) = qrcode::QrCode::new(uri.as_bytes()).ok().filter(|_| !hosts.is_empty()) {
		let img = qr
			.render::<qrcode::render::unicode::Dense1x2>()
			.dark_color(qrcode::render::unicode::Dense1x2::Dark)
			.light_color(qrcode::render::unicode::Dense1x2::Light)
			.quiet_zone(true)
			.build();
		for line in img.lines() {
			// Black on white whatever the terminal's theme, or phones read it inverted.
			if tty {
				println!("  \x1b[30;47m{line}\x1b[0m");
			} else {
				println!("  {line}");
			}
		}
	}
	let code = format!("{} {}", &pairing.code[..3], &pairing.code[3..]);
	println!();
	if hosts.is_empty() {
		println!("  In Elamon tap Add computer, give the address the phone reaches this computer at, and enter:");
	} else {
		println!("  Scan the code with the phone's camera, or in Elamon tap Add computer, give an address");
		println!("  or name below, and enter:");
	}
	println!();
	println!("      {code}");
	println!();
	println!("  Computer:     {name}");
	println!(
		"  Addresses:    {}  port {}",
		if offered.addrs.is_empty() { "none a phone can reach".into() } else { offered.addrs.join(", ") },
		config.port
	);
	if !plan.waiting.is_empty() {
		println!("                and {} once it has an address", plan.waiting.join(", "));
	}
	if !offered.names.is_empty() {
		println!("  Names:        {}", offered.names.join(", "));
	}
	if !offered.left_out.is_empty() {
		println!("  Not in the QR code, which carries {} at most: {}", consts::protocol::PAIR_HOSTS_MAX, offered.left_out.join(", "));
	}
	println!("  Certificate:  {}", store::pretty_fingerprint(&identity.fingerprint));
	println!("  The app shows the same certificate digits when you pair by code; they must match.");
	println!();
	println!("  Role:         {} — {}", role.as_str(), role.describe());
	if !role_given {
		println!("                another: elamond pair --role {}", Role::choices());
	}
	println!("                later: elamond clients role ID ROLE");
	let shown = config.profile(profile.as_deref().unwrap_or(&config.default_profile));
	if shown.name == shown.id {
		println!("  Profile:      {}", shown.id);
	} else {
		println!("  Profile:      {} — {}", shown.id, shown.name);
	}
	let others: Vec<&str> = config.profiles.keys().map(String::as_str).filter(|p| *p != shown.id).collect();
	if profile.is_none() && !others.is_empty() {
		println!("                another: elamond pair --profile {}", others.join("|"));
	}
	println!("                later: elamond clients profile ID PROFILE");
	println!();
	// Not where it listens on nothing yet: that says nothing either way.
	let probe = plan.probe_addrs();
	if !probe.is_empty() && !service_running(&probe, config.port) {
		println!("  ! The Elamon service isn't answering on port {}. Start it: systemctl --user start elamond", config.port);
		println!();
	}
	if yes {
		println!("  Whichever phone gives the code is paired, without asking (--yes).");
	} else {
		println!("  The phone that gives the code is shown here first, for you to let in or not.");
	}
	println!("  Waiting for the phone… (the code works for {} minutes, Ctrl+C to stop)", timeout::PAIRING_TTL_SECS / 60);

	let expired = || Err("the code expired; run `elamond pair` again".to_string());
	let mut asked: Option<String> = None;
	let paired = |c: store::Client| {
		// Withdrawn here too, should the service not get to it.
		store::clear_pairing_code(&dir, &pairing.code);
		println!("  Paired with \"{}\".", c.name);
		Ok(())
	};
	loop {
		std::thread::sleep(cadence::PAIR_POLL);
		if let Some(c) = new_phone() {
			return paired(c);
		}
		if STOP.load(Ordering::SeqCst) {
			store::clear_pairing_code(&dir, &pairing.code);
			return Err("stopped; the code no longer works".into());
		}
		match store::read_pairing(&dir) {
			// A phone gave the right code and waits: the person here decides, once.
			Some(p) if p.code == pairing.code && p.ask.is_some() => {
				let (Some(ask), Some(lines)) = (p.ask, &lines) else { continue };
				if ask.allow.is_some() || asked.as_deref() == Some(ask.id.as_str()) {
					continue;
				}
				asked = Some(ask.id.clone());
				let failed = |e: std::io::Error| format!("can't save the answer: {e}");
				match ask_person(&dir, &ask, role, &pairing.code, lines) {
					Answer::Allow => {
						store::answer_pairing(&dir, &pairing.code, &ask.id, true).map_err(failed)?;
						println!("  Letting it in…");
					}
					Answer::Deny => {
						store::answer_pairing(&dir, &pairing.code, &ask.id, false).map_err(failed)?;
						return Err("the phone was turned away, and the code no longer works; run `elamond pair` again for another".into());
					}
					Answer::Late => {
						store::clear_pairing_code(&dir, &pairing.code);
						return Err(format!("no answer within {} s: the phone was turned away, and the code no longer works", timeout::PAIR_APPROVAL_SECS));
					}
					Answer::Gone => {}
					Answer::Stopped => {
						store::clear_pairing_code(&dir, &pairing.code);
						return Err("stopped; the phone was turned away, and the code no longer works".into());
					}
				}
			}
			// Expiry is judged by the time saved with the code, as the service judges it.
			Some(p) if p.code == pairing.code && p.usable(store::now()) => {}
			Some(p) if p.code == pairing.code => {
				store::clear_pairing_code(&dir, &pairing.code);
				return if p.expires <= store::now() { expired() } else { Err(used_up()) };
			}
			Some(_) => return Err("another `elamond pair` replaced this code".into()),
			None => {
				// The service adds the phone before it withdraws the code.
				if let Some(c) = new_phone() {
					return paired(c);
				}
				if asked.is_some() {
					return Err("the phone stopped waiting before it was let in, and the code no longer works; run `elamond pair` again".into());
				}
				return if pairing.expires <= store::now() { expired() } else { Err(used_up()) };
			}
		}
	}
}

fn used_up() -> String {
	"the code was used up by wrong attempts; run `elamond pair` again".into()
}

/// Whether the service answers at one of `addrs`, where it listens.
fn service_running(addrs: &[IpAddr], port: u16) -> bool {
	addrs.iter().any(|a| std::net::TcpStream::connect_timeout(&SocketAddr::new(*a, port), timeout::SERVICE_CHECK).is_ok())
}

/// The link in the QR code: what the app needs to find this computer at one of `hosts`, know its
/// certificate and pair with `code`. Addresses and names need no encoding.
fn pair_link(identity: &identity::Identity, name: &str, port: u16, code: &str, hosts: &[String]) -> String {
	format!(
		"elamon://pair?v=1&id={}&name={}&port={port}&fp={}&code={code}&h={}",
		identity.id,
		percent_encode(name),
		identity.fingerprint,
		hosts.join(",")
	)
}

fn percent_encode(s: &str) -> String {
	s.bytes()
		.map(|b| match b {
			b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
			_ => format!("%{b:02X}"),
		})
		.collect()
}

/// The paired phones, for the commands that list and change them.
fn paired() -> Result<Vec<store::Client>, String> {
	let mut list = store::load_clients().map_err(|e| match e.kind() {
		std::io::ErrorKind::InvalidData => format!("{e}\n  fix the file, or move it away and pair the phones again"),
		_ => e.to_string(),
	})?;
	// Names go to the terminal: cleaned again, should the file hold one that isn't.
	for c in &mut list {
		c.name = store::sanitize_name(&c.name);
	}
	Ok(list)
}

fn role_arg(s: &str) -> Result<Role, String> {
	Role::parse(s).ok_or_else(|| format!("no role \"{s}\"; the roles are {}", Role::choices()))
}

/// A profile the settings have.
fn profile_arg(config: &store::Config, s: &str) -> Result<String, String> {
	if config.profiles.contains_key(s) {
		return Ok(s.to_string());
	}
	let ids: Vec<&str> = config.profiles.keys().map(String::as_str).collect();
	Err(format!("no profile \"{s}\" in the settings; the profiles are {}", ids.join(", ")))
}

/// `args` without `--with-added`, and whether it was there.
fn with_added(args: &[String]) -> (Vec<&str>, bool) {
	let flag = args.iter().any(|a| a == "--with-added");
	(args.iter().map(String::as_str).filter(|a| *a != "--with-added").collect(), flag)
}

/// The phones `which` names ("all", or an id) among `clients`, and with `with_added` those added
/// with its key, one step or more: the ids to act on, and the phones added with its key that are
/// left out. The id of a phone unpaired before still leads to those added with its key.
fn choose<'a>(clients: &'a [store::Client], which: &str, with_added: bool) -> Result<(Vec<String>, Vec<&'a store::Client>), String> {
	if which == "all" {
		return Ok((clients.iter().map(|c| c.id.clone()).collect(), Vec::new()));
	}
	let added = store::added_with(clients, which);
	let there = clients.iter().any(|c| c.id == which);
	if !there && (added.is_empty() || !with_added) {
		let mut e = format!("no paired phone has the id \"{which}\"");
		if !added.is_empty() {
			e.push_str(&format!("; {} still paired came by its key, --with-added acts on them", added.len()));
		}
		return Err(e);
	}
	let mut chosen: Vec<String> = clients.iter().filter(|c| c.id == which).map(|c| c.id.clone()).collect();
	if with_added {
		chosen.extend(added.iter().map(|c| c.id.clone()));
		return Ok((chosen, Vec::new()));
	}
	Ok((chosen, added))
}

/// The phones as the commands list them, a line each: id, role, profile, name, the app it said it
/// is last, and how it came. The profile is the one it gets (`default_profile` for one it has no
/// longer), as far as the settings `config` parse.
fn client_lines(clients: &[&store::Client], config: Option<&store::Config>) -> Vec<String> {
	let profile = |c: &store::Client| -> String {
		let Some(config) = config else { return c.profile.clone().unwrap_or_else(|| "default".into()) };
		let got = config.profile(c.profile.as_deref().unwrap_or(&config.default_profile)).id;
		match &c.profile {
			Some(p) if *p != got => format!("{got} ({p}: not in the settings)"),
			_ => got,
		}
	};
	let app = |c: &store::Client| c.app.as_deref().map_or_else(|| "-".to_string(), |a| a.replacen('/', " ", 1));
	let rows: Vec<(String, String)> = clients.iter().map(|c| (profile(c), app(c))).collect();
	let width = |f: &dyn Fn(usize) -> usize| (0..clients.len()).map(f).max().unwrap_or(0);
	let (pw, nw, aw) = (width(&|i| rows[i].0.chars().count()), width(&|i| clients[i].name.chars().count()), width(&|i| rows[i].1.chars().count()));
	clients
		.iter()
		.zip(&rows)
		.map(|(c, (p, a))| {
			let when = match &c.from {
				Some(from) => format!("added {} with the key of {from}", format_date(c.created)),
				None => format!("paired {}", format_date(c.created)),
			};
			let pad = |s: &str, w: usize| format!("{s}{}", " ".repeat(w.saturating_sub(s.chars().count())));
			format!("{}  {:<7}  {}  {}  {}  ({when})", c.id, c.role.as_str(), pad(p, pw), pad(&c.name, nw), pad(a, aw))
		})
		.collect()
}

fn clients(args: &[String]) -> Result<(), String> {
	let (args, family) = with_added(args);
	match args.as_slice() {
		[] if !family => {}
		["role", which, role] => return set_role(which, role_arg(role)?, family),
		["profile", which, profile] => return set_profile(which, profile, family),
		_ => {
			return Err(format!(
				"usage: elamond clients, elamond clients role <id|all> <{}> [--with-added], or elamond clients profile <id|all> <profile> [--with-added]",
				Role::choices()
			));
		}
	}
	let clients = paired()?;
	if clients.is_empty() {
		println!("No phones are paired. Pair one with `elamond pair`.");
		return Ok(());
	}
	let config = store::load_config().ok();
	for line in client_lines(&clients.iter().collect::<Vec<_>>(), config.as_ref()) {
		println!("{line}");
	}
	println!();
	println!("Change what a phone may do: elamond clients role <id|all> <{}>", Role::choices());
	println!("Change how it looks and behaves: elamond clients profile <id|all> <profile>");
	Ok(())
}

/// The lock on clients.json, held from reading it to writing it back.
fn lock_clients() -> Result<store::ClientsLock, String> {
	let dir = store::data_dir();
	store::lock_clients(&dir).map_err(|e| format!("can't lock {}: {e}", dir.join(store::CLIENTS_LOCK_FILE).display()))
}

fn set_role(which: &str, role: Role, family: bool) -> Result<(), String> {
	let _lock = lock_clients()?;
	let mut clients = paired()?;
	if clients.is_empty() {
		return Err("no phones are paired".into());
	}
	let (chosen, left) = choose(&clients, which, family)?;
	let left: Vec<store::Client> = left.into_iter().cloned().collect();
	let mut changed = Vec::new();
	for c in clients.iter_mut().filter(|c| chosen.contains(&c.id)) {
		if c.role != role {
			c.role = role;
			changed.push(c.name.clone());
		}
	}
	if !changed.is_empty() {
		store::save_clients(&clients).map_err(|e| e.to_string())?;
	}
	match changed.len() {
		0 if chosen.len() == 1 => println!("That phone already is {}.", role.as_str()),
		0 => println!("They already are {}.", role.as_str()),
		_ => println!("Now {}: {}. The service applies it at once, open streams included.", role.as_str(), changed.join(", ")),
	}
	println!("  {}: {}", role.as_str(), role.describe());
	print_left(&left);
	Ok(())
}

/// The phones added with the key of one changed and left as they were.
fn print_left(left: &[store::Client]) {
	if !left.is_empty() {
		println!();
		println!("Added with its key, and left as they are (--with-added changes them too):");
		let config = store::load_config().ok();
		for line in client_lines(&left.iter().collect::<Vec<_>>(), config.as_ref()) {
			println!("  {line}");
		}
	}
}

/// `elamond clients profile`: a profile the settings have, for the phones `which` names.
fn set_profile(which: &str, profile: &str, family: bool) -> Result<(), String> {
	let config = store::load_config()?;
	let profile = profile_arg(&config, profile)?;
	let _lock = lock_clients()?;
	let mut clients = paired()?;
	if clients.is_empty() {
		return Err("no phones are paired".into());
	}
	let (chosen, left) = choose(&clients, which, family)?;
	let left: Vec<store::Client> = left.into_iter().cloned().collect();
	let mut changed = Vec::new();
	for c in clients.iter_mut().filter(|c| chosen.contains(&c.id)) {
		if c.profile.as_deref() != Some(profile.as_str()) {
			c.profile = Some(profile.clone());
			changed.push(c.name.clone());
		}
	}
	if !changed.is_empty() {
		store::save_clients(&clients).map_err(|e| e.to_string())?;
	}
	let name = config.profile(&profile).name;
	let named = if name == profile { profile.clone() } else { format!("{profile} ({name})") };
	match changed.len() {
		0 if chosen.len() == 1 => println!("That phone already has {named}."),
		0 => println!("They already have {named}."),
		_ => println!("Now with {named}: {}. The service applies it at once, open streams included.", changed.join(", ")),
	}
	print_left(&left);
	Ok(())
}

/// `elamond control off` holds control off for every phone, whatever the settings say, until
/// `elamond control on`; the service notices within seconds, its acting requests at once.
fn control(what: Option<&str>) -> Result<(), String> {
	let dir = store::data_dir();
	match what.unwrap_or("status") {
		"off" => {
			store::set_control_off(&dir, true).map_err(|e| format!("can't write to {}: {e}", dir.display()))?;
			println!("Control is held off: no phone acts on this computer until `elamond control on`.");
		}
		"on" => {
			store::set_control_off(&dir, false).map_err(|e| format!("can't change {}: {e}", dir.display()))?;
		}
		"status" => {}
		other => return Err(format!("unknown \"{other}\"; elamond control off, on or status")),
	}
	let config = match store::load_config() {
		Ok(c) => c,
		Err(e) => {
			println!("Control: off. The settings don't parse:");
			println!("  {e}");
			println!("A running service keeps the settings it read before, but holds control off until they");
			println!("parse again; a service that starts now won't start at all.");
			if store::control_held_off(&dir) {
				println!("It is held off by `elamond control off` too.");
			}
			return Ok(());
		}
	};
	match (store::control_off_since(&dir), config.control) {
		(Some(since), _) => {
			let when = if since > 0 { format!(" since {}", format_date(since)) } else { String::new() };
			println!("Control: off, held off{when} by `elamond control off`; `elamond control on` lets go.");
			if !config.control {
				println!("  The settings have it off too (control = false).");
			}
		}
		(None, true) => println!("Control: on. Phones act as far as their roles allow; `elamond control off` stops them all."),
		(None, false) => println!("Control: off by the settings (control = false in elamond.toml)."),
	}
	Ok(())
}

/// Copies of keys a restore set aside in the data directory `dir`, `key.der.before-restore-*`.
fn keys_set_aside(dir: &std::path::Path) -> Vec<PathBuf> {
	let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
		.map(|d| d.flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("key.der.before-restore-"))).collect())
		.unwrap_or_default();
	found.sort();
	found
}

/// `elamond rotate-cert`: a new certificate and key, the old key signing that the new certificate
/// follows it before it is deleted, for routine renewal; with `--no-statement` nothing signed, for
/// a key that may have leaked. The service presents the new one within seconds.
fn rotate_cert(args: &[String]) -> Result<(), String> {
	let unsigned = match args {
		[] => false,
		[flag] if flag == "--no-statement" => true,
		_ => return Err("usage: elamond rotate-cert [--no-statement]".into()),
	};
	let dir = store::data_dir();
	if unsigned {
		let (old_fp, new_fp) = identity::renew_unsigned(&dir)?;
		println!("New certificate:  {}", store::pretty_fingerprint(&new_fp));
		println!("Old certificate:  {} (its key deleted, nothing signed with it)", store::pretty_fingerprint(&old_fp));
		println!();
		println!("Paired phones will say the computer's certificate changed: nothing vouches for the new one.");
		println!("If the old key may have leaked, whoever has it can still pose as this computer to the phones");
		println!("that trust it, and learn their keys. Unpair them all and pair each again:");
		println!("  elamond revoke all");
		println!("  elamond pair          (for each phone; the app replaces the computer it had)");
	} else {
		let r = identity::rotate(&dir)?;
		println!("New certificate:  {}", store::pretty_fingerprint(&r.new_fp));
		println!("Old certificate:  {}", store::pretty_fingerprint(&r.old_fp));
		println!();
		println!("The old certificate's key signed the new one and was then deleted. Paired phones check");
		println!("that signature the next time they connect and move to the new certificate by themselves.");
		println!("This is for routine renewal: if the old key may have leaked, phones mustn't follow what it");
		println!("signed. Then run `elamond rotate-cert --no-statement` and pair the phones again.");
	}
	println!("A running service presents the new certificate within a few seconds.");
	println!();
	println!("Older keys live on in backups made before now (elamond backup) and in files a restore set");
	println!("aside; delete those you don't need.");
	for path in keys_set_aside(&dir) {
		println!("  {}", path.display());
	}
	Ok(())
}

fn revoke(args: &[String]) -> Result<(), String> {
	let (args, family) = with_added(args);
	let [which] = args.as_slice() else {
		return Err("usage: elamond revoke <id|all> [--with-added]; `elamond clients` lists the ids".into());
	};
	let _lock = lock_clients()?;
	let clients = paired()?;
	if *which == "all" && clients.is_empty() {
		println!("No phones are paired.");
		return Ok(());
	}
	let (chosen, left) = choose(&clients, which, family)?;
	let keep: Vec<_> = clients.iter().filter(|c| !chosen.contains(&c.id)).cloned().collect();
	store::save_clients(&keep).map_err(|e| e.to_string())?;
	let gone: Vec<&str> = clients.iter().filter(|c| chosen.contains(&c.id)).map(|c| c.name.as_str()).collect();
	println!("Unpaired {} phone(s): {}.", gone.len(), gone.join(", "));
	// A phone that came by this one's token and asked for a key of its own stays paired: after
	// Import pairings, the new phone. Unless that is known for sure, it is the one to look at.
	if !left.is_empty() {
		println!();
		println!("  ! STILL PAIRED: these got a key of their own with the token of the phone just unpaired,");
		println!("  ! or of one that did:");
		let config = store::load_config().ok();
		for line in client_lines(&left, config.as_ref()) {
			println!("  !   {line}");
		}
		println!("  ! After Import pairings on a new phone, that is the new phone. If you don't know them, or the");
		println!("  ! phone was lost or stolen, unpair them too: elamond revoke {which} --with-added");
	}
	Ok(())
}

fn snapshot() -> Result<(), String> {
	let config = store::load_config()?;
	let (interval, history_len, _) = timing(&config);
	let mut c = collect::Collector::new(Ticks::new(cadence::CLI_SNAPSHOT));
	let all = ui::Sources { processes: true, media: true, volume: true, gpus: true, displays: true, links: true };
	c.read(true, all);
	std::thread::sleep(cadence::CLI_SNAPSHOT);
	let reading = c.read(true, all);
	let info = c.info("-", &machine_name(&config), interval, history_len);
	let mut procs = reading.processes.unwrap_or_default();
	procs.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
	procs.truncate(limit::SNAPSHOT_TOP);
	let out = serde_json::json!({ "info": info, "sample": reading.sample, "top_processes": procs });
	println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
	Ok(())
}

/// YYYY-MM-DD from Unix time, without pulling in a date library.
fn format_date(secs: u64) -> String {
	let days = (secs / 86_400) as i64;
	// Howard Hinnant's civil_from_days.
	let z = days + 719_468;
	let era = z.div_euclid(146_097);
	let doe = z.rem_euclid(146_097);
	let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
	let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
	let mp = (5 * doy + 2) / 153;
	let d = doy - (153 * mp + 2) / 5 + 1;
	let m = if mp < 10 { mp + 3 } else { mp - 9 };
	let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
	format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn names_in_the_pairing_link() {
		assert_eq!(percent_encode("Laptop-16.local_~"), "Laptop-16.local_~");
		assert_eq!(percent_encode("My PC"), "My%20PC");
		assert_eq!(percent_encode("It's"), "It%27s");
		assert_eq!(percent_encode("Ноут"), "%D0%9D%D0%BE%D1%83%D1%82");
		assert_eq!(percent_encode("a&b=c"), "a%26b%3Dc");
	}

	#[test]
	fn the_pairing_link_carries_addresses_then_names() {
		let identity = identity::Identity { id: "0123456789abcdef0123456789abcdef".into(), cert_der: Vec::new(), key_der: Vec::new(), fingerprint: "ab".repeat(32) };
		let addrs: Vec<IpAddr> = ["192.168.200.101", "100.64.0.1"].iter().map(|a| a.parse().unwrap()).collect();
		let offered = names::Offered::of(&addrs, &["deskpc.example.net".into()], Some("deskpc.tail1234.ts.net"));
		let link = pair_link(&identity, "My PC", 47823, "042917", &offered.hosts());
		assert_eq!(
			link,
			format!(
				"elamon://pair?v=1&id=0123456789abcdef0123456789abcdef&name=My%20PC&port=47823&fp={}&code=042917&h=192.168.200.101,100.64.0.1,deskpc.example.net,deskpc.tail1234.ts.net",
				"ab".repeat(32)
			)
		);
	}

	#[test]
	fn revoking_and_roles_reach_the_keys_a_token_gave_when_asked() {
		let c = |id: &str, from: Option<&str>| store::Client {
			id: id.into(),
			name: id.into(),
			token_hash: String::new(),
			created: 0,
			role: Role::Admin,
			from: from.map(str::to_string),
			..Default::default()
		};
		let list = [c("old", None), c("new", Some("old")), c("newer", Some("new")), c("other", None)];
		let ids = |r: Result<(Vec<String>, Vec<&store::Client>), String>| {
			let (chosen, left) = r.unwrap();
			(chosen, left.iter().map(|c| c.id.clone()).collect::<Vec<_>>())
		};
		// Alone, the rest of its family named; with --with-added, all of it.
		assert_eq!(ids(choose(&list, "old", false)), (vec!["old".into()], vec!["new".into(), "newer".into()]));
		assert_eq!(ids(choose(&list, "old", true)).0, ["old", "new", "newer"]);
		assert_eq!(ids(choose(&list, "new", false)), (vec!["new".into()], vec!["newer".into()]));
		assert_eq!(ids(choose(&list, "other", true)), (vec!["other".into()], vec![]));
		assert_eq!(ids(choose(&list, "all", false)).0.len(), 4);
		// The id of a phone unpaired before still reaches those added with its key, when asked.
		let after = [c("new", Some("old")), c("newer", Some("new")), c("other", None)];
		assert!(choose(&after, "old", false).unwrap_err().contains("--with-added"));
		assert_eq!(ids(choose(&after, "old", true)).0, ["new", "newer"]);
		assert!(choose(&after, "nobody", true).unwrap_err().contains("no paired phone"));
	}

	#[test]
	fn the_phones_as_the_commands_list_them() {
		let config = settings::read_sources(&[settings::Source::new("a.toml", "[profiles.owner]\nname = \"Owner\"\n")]).unwrap();
		let c = |id: &str, name: &str, profile: Option<&str>, app: Option<&str>| store::Client {
			id: id.into(),
			name: name.into(),
			created: 1_700_000_000,
			role: Role::Monitor,
			profile: profile.map(Into::into),
			app: app.map(Into::into),
			..Default::default()
		};
		let list = [c("0123abcd", "Pixel", Some("owner"), Some("android/0.6.0")), c("4567cdef", "Old tablet", Some("gone"), None), c("89ab0123", "Tab", None, None)];
		let lines = client_lines(&list.iter().collect::<Vec<_>>(), Some(&config));
		assert_eq!(
			lines,
			[
				"0123abcd  monitor  owner                                Pixel       android 0.6.0  (paired 2023-11-14)",
				"4567cdef  monitor  default (gone: not in the settings)  Old tablet  -              (paired 2023-11-14)",
				"89ab0123  monitor  default                              Tab         -              (paired 2023-11-14)",
			]
		);
		// Settings that don't parse: the profiles as clients.json has them.
		assert!(client_lines(&list.iter().collect::<Vec<_>>(), None)[1].contains("  gone "));
	}

	#[test]
	fn dates() {
		assert_eq!(format_date(0), "1970-01-01");
		assert_eq!(format_date(1_700_000_000), "2023-11-14");
		assert_eq!(format_date(951_782_400), "2000-02-29");
	}

	#[test]
	fn timing_within_bounds() {
		let config = |interval_ms, history_minutes| store::Config { interval_ms, history_minutes, ..Default::default() };
		let (i, h, notes) = timing(&config(1000, 10));
		assert_eq!((i, h, notes.len()), (Duration::from_secs(1), 600, 0));
		let (i, h, notes) = timing(&config(50, 10));
		assert_eq!((i, h), (Duration::from_millis(250), 2400));
		assert!(notes[0].contains("using 250"), "{notes:?}");
		let (_, h, notes) = timing(&config(1000, u64::MAX));
		assert_eq!(h, limit::HISTORY_MAX);
		assert!(notes[0].contains("keeping 7200"), "{notes:?}");
		let (_, h, notes) = timing(&config(10_000, 0));
		assert_eq!((h, notes.len()), (limit::HISTORY_MIN, 1));
	}
}
