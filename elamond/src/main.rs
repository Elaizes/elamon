//! elamond: serves this computer's metrics to the Elamon phone app.

mod access;
mod activity;
mod backup;
mod bus;
mod collect;
mod consts;
mod control;
mod errors;
#[cfg(test)]
mod fixtures;
mod identity;
mod links;
mod listen;
mod mdns;
mod model;
mod names;
mod server;
mod store;

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
  elamond pair [--role ROLE]           show a code and QR to pair a phone
  elamond clients                      list paired phones and their roles
  elamond clients role <id|all> ROLE   change what a phone may do
  elamond revoke <id|all>              unpair a phone
  elamond control [off|on|status]      hold control off for every phone, or let go
  elamond rotate-cert                  a new certificate, which paired phones follow
  elamond backup FILE                  save the identity, the paired phones and the rest to FILE,
                                       encrypted with a passphrase (--plain: not encrypted)
  elamond restore FILE                 put a backup back, the service stopped (--force: running);
                                       --with-settings puts elamond.toml and elamond.d back too
  elamond snapshot                     print one reading as JSON
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
	let result = match args.first().map(String::as_str).unwrap_or("serve") {
		"serve" => serve(),
		"pair" => pair(&args[1..]),
		"clients" => clients(&args[1..]),
		"control" => control(args.get(1).map(String::as_str)),
		"rotate-cert" => rotate_cert(&args[1..]),
		"backup" => backup::backup(&args[1..]),
		"restore" => backup::restore(&args[1..]),
		"revoke" => revoke(args.get(1).map(String::as_str)),
		"snapshot" => snapshot(),
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
	let config = store::load_config()?;
	let dir = store::data_dir();
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
	collector.read(false, false);
	std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
	let info = serde_json::to_string(&collector.info(&identity.id, &name, config.control && !held_off, interval, history_len));
	let info = Arc::new(info.unwrap_or_default());
	let first = Arc::new(server::Frame { sample: "{}".into(), basic: "{}".into(), media: None, processes: None, info, watched: false });
	let (tx, rx) = watch::channel(first);
	let hub = Arc::new(server::Hub::new(
		identity.id.clone(),
		name.clone(),
		rx,
		collect::HistoryRing::new(history_len),
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
			.spawn(move || collect_loop(collector, hub, tx, every, id, name, rt))
			.map_err(|e| e.to_string())?;
	}

	runtime.block_on(async move {
		tokio::spawn(watch_settings(hub.clone()));
		let app = server::router(hub.clone()).into_make_service_with_connect_info::<SocketAddr>();
		let tls = RustlsConfig::from_config(Arc::new(tls));
		let mut listeners = listen::Listeners::new(config.port, tls.clone(), app);
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
/// don't parse leave the running settings as they were. Looks at `elamond control off`'s file
/// as often.
async fn watch_settings(hub: Arc<server::Hub>) {
	use tokio::signal::unix::{signal, SignalKind};
	let Ok(mut hangup) = signal(SignalKind::hangup()) else {
		return;
	};
	let mut stamp = store::config_stamp();
	loop {
		let asked = tokio::select! {
			_ = hangup.recv() => true,
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
			Err(e) => eprintln!("settings: kept the running ones; {e}"),
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

fn collect_loop(
	mut collector: collect::Collector,
	hub: Arc<server::Hub>,
	tx: watch::Sender<Arc<server::Frame>>,
	every: Ticks,
	id: String,
	name: String,
	rt: tokio::runtime::Handle,
) {
	let _exit = ExitOnPanic;
	let mut info = tx.borrow().info.clone();
	let mut tick: u32 = 0;
	let mut extras = control::Extras::new(every);
	loop {
		let started = std::time::Instant::now();
		let watched = hub.watchers.load(Ordering::SeqCst) > 0;
		let want_procs = hub.proc_watchers.load(Ordering::SeqCst) > 0;
		let want_media = hub.media_watchers.load(Ordering::SeqCst) > 0;
		let mut reading = collector.read(watched, want_procs);
		let media = extras.apply(&hub, &rt, watched, want_media, &mut reading.sample);
		if let Ok(mut h) = hub.history.lock() {
			h.push(&reading.sample);
		}
		// GPU names improve once NVML has been read, so info is rebuilt now and then.
		if tick % every.info == 1 % every.info {
			let fresh = serde_json::to_string(&collector.info(&id, &name, hub.control_on(), hub.interval, hub.history_len)).unwrap_or_default();
			if fresh != *info {
				info = Arc::new(fresh);
			}
		}
		tick = tick.wrapping_add(1);
		let sample = serde_json::to_string(&reading.sample).unwrap_or_else(|_| "{}".into());
		let basic = serde_json::to_string(&reading.sample.into_basic()).unwrap_or_else(|_| "{}".into());
		let frame = server::Frame {
			sample,
			basic,
			media: media.and_then(|m| serde_json::to_string(&m).ok()),
			processes: reading.processes.map(Arc::new),
			info: info.clone(),
			watched,
		};
		tx.send_replace(Arc::new(frame));
		std::thread::sleep(hub.interval.saturating_sub(started.elapsed()));
	}
}

/// Set by Ctrl+C, SIGTERM or a closed terminal while `elamond pair` waits.
static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_stop(_: libc::c_int) {
	STOP.store(true, Ordering::SeqCst);
}

fn pair(args: &[String]) -> Result<(), String> {
	let config = store::load_config()?;
	let role = match args {
		[] => config.default_role,
		[flag, r] if flag == "--role" => role_arg(r)?,
		[flag] if flag.starts_with("--role=") => role_arg(&flag["--role=".len()..])?,
		_ => return Err(format!("usage: elamond pair [--role {}]", Role::choices())),
	};
	let identity = identity::Identity::load_or_create(&store::data_dir())?;
	let name = machine_name(&config);
	let before: Vec<String> = match store::load_clients() {
		Ok(list) => list.into_iter().map(|c| c.id).collect(),
		Err(e) => {
			eprintln!("  ! {e}\n    pairing sets the damaged file aside and starts a new list");
			Vec::new()
		}
	};
	let pairing = store::new_pairing(role).map_err(|e| format!("can't save the pairing code: {e}"))?;
	// From here on, stopping withdraws the code instead of leaving it to be guessed at.
	for sig in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
		// SAFETY: the handler only stores to an atomic, which is async-signal-safe.
		unsafe { libc::signal(sig, on_stop as *const () as libc::sighandler_t) };
	}
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
	if args.is_empty() {
		println!("                another: elamond pair --role {}", Role::choices());
	}
	println!("                later: elamond clients role ID ROLE");
	println!();
	// Not where it listens on nothing yet: that says nothing either way.
	let probe = plan.probe_addrs();
	if !probe.is_empty() && !service_running(&probe, config.port) {
		println!("  ! The Elamon service isn't answering on port {}. Start it: systemctl --user start elamond", config.port);
		println!();
	}
	println!("  Waiting for the phone… (the code works for {} minutes, Ctrl+C to stop)", timeout::PAIRING_TTL_SECS / 60);

	let expired = || Err("the code expired; run `elamond pair` again".to_string());
	loop {
		std::thread::sleep(cadence::PAIR_POLL);
		if let Some(c) = new_phone() {
			println!("  Paired with \"{}\".", c.name);
			return Ok(());
		}
		if STOP.load(Ordering::SeqCst) {
			store::clear_pairing_code(&pairing.code);
			return Err("stopped; the code no longer works".into());
		}
		match store::read_pairing() {
			// Expiry is judged by the time saved with the code, as the service judges it.
			Some(p) if p.code == pairing.code && p.usable(store::now()) => {}
			Some(p) if p.code == pairing.code => {
				store::clear_pairing_code(&pairing.code);
				return if p.expires <= store::now() { expired() } else { Err(used_up()) };
			}
			Some(_) => return Err("another `elamond pair` replaced this code".into()),
			None => {
				// The service adds the phone before it withdraws the code.
				if let Some(c) = new_phone() {
					println!("  Paired with \"{}\".", c.name);
					return Ok(());
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
	store::load_clients().map_err(|e| match e.kind() {
		std::io::ErrorKind::InvalidData => format!("{e}\n  fix the file, or move it away and pair the phones again"),
		_ => e.to_string(),
	})
}

fn role_arg(s: &str) -> Result<Role, String> {
	Role::parse(s).ok_or_else(|| format!("no role \"{s}\"; the roles are {}", Role::choices()))
}

fn clients(args: &[String]) -> Result<(), String> {
	match args {
		[] => {}
		[cmd, which, role] if cmd == "role" => return set_role(which, role_arg(role)?),
		_ => return Err(format!("usage: elamond clients, or elamond clients role <id|all> <{}>", Role::choices())),
	}
	let clients = paired()?;
	if clients.is_empty() {
		println!("No phones are paired. Pair one with `elamond pair`.");
		return Ok(());
	}
	let width = clients.iter().map(|c| c.name.chars().count()).max().unwrap_or(0);
	for c in &clients {
		let pad = width - c.name.chars().count();
		println!("{}  {:<7}  {}{}  (paired {})", c.id, c.role.as_str(), c.name, " ".repeat(pad), format_date(c.created));
	}
	println!();
	println!("Change what a phone may do: elamond clients role <id|all> <{}>", Role::choices());
	Ok(())
}

fn set_role(which: &str, role: Role) -> Result<(), String> {
	let mut clients = paired()?;
	let mut changed = Vec::new();
	let mut matched = 0;
	for c in clients.iter_mut().filter(|c| which == "all" || c.id == which) {
		matched += 1;
		if c.role != role {
			c.role = role;
			changed.push(c.name.clone());
		}
	}
	if matched == 0 {
		return Err(if which == "all" { "no phones are paired".into() } else { format!("no paired phone has the id \"{which}\"") });
	}
	if !changed.is_empty() {
		store::save_clients(&clients).map_err(|e| e.to_string())?;
	}
	match changed.len() {
		0 if matched == 1 => println!("That phone already is {}.", role.as_str()),
		0 => println!("They already are {}.", role.as_str()),
		_ => println!("Now {}: {}. The service applies it at once, open streams included.", role.as_str(), changed.join(", ")),
	}
	println!("  {}: {}", role.as_str(), role.describe());
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
	let config = store::load_config()?;
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

/// `elamond rotate-cert`: a new certificate and key, the old key signing that the new certificate
/// follows it before it is deleted. The service presents the new one within seconds.
fn rotate_cert(args: &[String]) -> Result<(), String> {
	if !args.is_empty() {
		return Err("usage: elamond rotate-cert".into());
	}
	let r = identity::rotate(&store::data_dir())?;
	println!("New certificate:  {}", store::pretty_fingerprint(&r.new_fp));
	println!("Old certificate:  {}", store::pretty_fingerprint(&r.old_fp));
	println!();
	println!("The old certificate's key signed the new one and was then deleted. Paired phones with");
	println!("Elamon 0.5 or newer check that signature the next time they connect and move to the new");
	println!("certificate by themselves; older apps say the certificate changed, and pair again.");
	println!("A running service presents the new certificate within a few seconds.");
	Ok(())
}

fn revoke(which: Option<&str>) -> Result<(), String> {
	let which = which.ok_or("which phone? `elamond clients` lists their ids")?;
	let clients = paired()?;
	if which == "all" && clients.is_empty() {
		println!("No phones are paired.");
		return Ok(());
	}
	let keep: Vec<_> = clients.iter().filter(|c| which != "all" && c.id != which).cloned().collect();
	if keep.len() == clients.len() {
		return Err(format!("no paired phone has the id \"{which}\""));
	}
	store::save_clients(&keep).map_err(|e| e.to_string())?;
	println!("Unpaired {} phone(s).", clients.len() - keep.len());
	Ok(())
}

fn snapshot() -> Result<(), String> {
	let config = store::load_config()?;
	let (interval, history_len, _) = timing(&config);
	let mut c = collect::Collector::new(Ticks::new(cadence::CLI_SNAPSHOT));
	c.read(true, true);
	std::thread::sleep(cadence::CLI_SNAPSHOT);
	let reading = c.read(true, true);
	let info = c.info("-", &machine_name(&config), config.control, interval, history_len);
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
