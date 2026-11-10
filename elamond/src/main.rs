//! elamond: serves this computer's metrics to the Elamon phone app.

mod bus;
mod collect;
mod control;
mod model;
mod server;
mod services;
mod store;

use std::collections::HashMap;
use std::io::IsTerminal;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use axum_server::tls_rustls::RustlsConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::sync::watch;

pub const API_VERSION: u32 = 1;
/// The oldest app that understands this service; an older one asks to be updated.
pub const MIN_APP: &str = "0.3.0";
/// How often the settings files are looked at for edits; SIGHUP rereads them at once.
const SETTINGS_POLL: Duration = Duration::from_secs(5);
const SERVICE_TYPE: &str = "_elamon._tcp.local.";

const HELP: &str = "\
elamond — serves this computer's metrics to the Elamon app

Usage:
  elamond serve          run the service (what the systemd unit does)
  elamond pair           show a code and QR to pair a phone
  elamond clients        list paired phones
  elamond revoke <id|all> unpair a phone
  elamond snapshot       print one reading as JSON
  elamond --version

Settings: ~/.config/elamon/elamond.toml   State: ~/.local/share/elamon/
";

fn main() {
	let args: Vec<String> = std::env::args().skip(1).collect();
	let result = match args.first().map(String::as_str).unwrap_or("serve") {
		"serve" => serve(),
		"pair" => pair(),
		"clients" => clients(),
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

/// The tick and the points of history kept, from the settings within sane bounds.
fn timing(config: &store::Config) -> (Duration, usize) {
	let interval = Duration::from_millis(config.interval_ms.clamp(250, 10_000));
	let history_len = (config.history_minutes * 60_000 / interval.as_millis() as u64).clamp(10, 7200) as usize;
	(interval, history_len)
}

fn serve() -> Result<(), String> {
	let config = store::load_config()?;
	let identity = store::Identity::load_or_create().map_err(|e| format!("can't create the TLS identity: {e}"))?;
	let name = machine_name(&config);
	let (interval, history_len) = timing(&config);

	let mut collector = collect::Collector::new();
	// The first reading primes CPU counters; the published one starts a tick later.
	collector.read(false, false);
	let info = Arc::new(serde_json::to_string(&collector.info(&identity.id, &name, config.control, interval, history_len)).unwrap_or_default());
	let first = Arc::new(server::Frame { sample: "{}".into(), processes: None, info });
	let (tx, rx) = watch::channel(first);
	let hub = Arc::new(server::Hub::new(
		identity.id.clone(),
		name.clone(),
		rx,
		collect::HistoryRing::new(history_len),
		interval,
		Arc::new(config.clone()),
	));
	if config.control {
		eprintln!("control is on: paired phones may act on this computer");
	}

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
			.spawn(move || collect_loop(collector, hub, tx, interval, id, name, rt))
			.map_err(|e| e.to_string())?;
	}

	let _mdns = if config.mdns { advertise(&name, &identity, config.port) } else { None };

	let provider = Arc::new(rustls::crypto::ring::default_provider());
	let cert = CertificateDer::from(identity.cert_der.clone());
	let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key_der.clone()));
	let mut tls = rustls::ServerConfig::builder_with_provider(provider)
		.with_safe_default_protocol_versions()
		.map_err(|e| e.to_string())?
		.with_no_client_auth()
		.with_single_cert(vec![cert], key)
		.map_err(|e| format!("bad TLS identity: {e}"))?;
	tls.alpn_protocols = vec![b"http/1.1".to_vec()];

	let ip: IpAddr = config.listen.parse().map_err(|_| format!("listen: \"{}\" is not an IP address", config.listen))?;
	runtime.block_on(async move {
		tokio::spawn(watch_settings(hub.clone()));
		let app = server::router(hub).into_make_service_with_connect_info::<SocketAddr>();
		let tls = RustlsConfig::from_config(Arc::new(tls));
		let addr = SocketAddr::new(ip, config.port);
		// "::" listens on IPv4 too, unless the kernel has IPv6 off; then fall back to IPv4.
		let listener = match std::net::TcpListener::bind(addr) {
			Ok(l) => l,
			Err(_) if ip == IpAddr::from([0u16; 8]) => std::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], config.port)))
				.map_err(|e| format!("can't listen on port {}: {e}", config.port))?,
			Err(e) => return Err(format!("can't listen on {addr}: {e}")),
		};
		listener.set_nonblocking(true).map_err(|e| e.to_string())?;
		eprintln!(
			"elamond {} on port {}, certificate {}",
			env!("CARGO_PKG_VERSION"),
			config.port,
			store::pretty_fingerprint(&identity.fingerprint)
		);
		let handle = axum_server::Handle::new();
		let h = handle.clone();
		tokio::spawn(async move {
			shutdown_signal().await;
			h.graceful_shutdown(Some(Duration::from_secs(2)));
		});
		axum_server::from_tcp_rustls(listener, tls)
			.map_err(|e| e.to_string())?
			.handle(handle)
			.serve(app)
			.await
			.map_err(|e| e.to_string())
	})
}

/// Rereads the settings on SIGHUP, or when elamond.toml or a drop-in changes. Files that
/// don't parse leave the running settings as they were.
async fn watch_settings(hub: Arc<server::Hub>) {
	use tokio::signal::unix::{signal, SignalKind};
	let Ok(mut hangup) = signal(SignalKind::hangup()) else {
		return;
	};
	let mut stamp = store::config_stamp();
	loop {
		let asked = tokio::select! {
			_ = hangup.recv() => true,
			_ = tokio::time::sleep(SETTINGS_POLL) => false,
		};
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

fn collect_loop(
	mut collector: collect::Collector,
	hub: Arc<server::Hub>,
	tx: watch::Sender<Arc<server::Frame>>,
	interval: Duration,
	id: String,
	name: String,
	rt: tokio::runtime::Handle,
) {
	let mut info = tx.borrow().info.clone();
	let mut tick: u64 = 0;
	let mut extras = control::Extras::new();
	loop {
		let started = std::time::Instant::now();
		let watched = hub.watchers.load(Ordering::SeqCst) > 0;
		let want_procs = hub.proc_watchers.load(Ordering::SeqCst) > 0;
		let want_media = hub.media_watchers.load(Ordering::SeqCst) > 0;
		let mut reading = collector.read(watched, want_procs);
		extras.apply(&hub, &rt, watched, want_media, &mut reading.sample);
		if let Ok(mut h) = hub.history.lock() {
			h.push(&reading.sample);
		}
		// GPU names improve once NVML has been read, so info is rebuilt now and then.
		if tick % 10 == 1 {
			let fresh = serde_json::to_string(&collector.info(&id, &name, hub.config().control, hub.interval, hub.history_len)).unwrap_or_default();
			if fresh != *info {
				info = Arc::new(fresh);
			}
		}
		tick += 1;
		let frame = server::Frame {
			sample: serde_json::to_string(&reading.sample).unwrap_or_else(|_| "{}".into()),
			processes: reading.processes.map(Arc::new),
			info: info.clone(),
		};
		tx.send_replace(Arc::new(frame));
		std::thread::sleep(interval.saturating_sub(started.elapsed()));
	}
}

fn advertise(name: &str, identity: &store::Identity, port: u16) -> Option<mdns_sd::ServiceDaemon> {
	let daemon = mdns_sd::ServiceDaemon::new().map_err(|e| eprintln!("mdns: {e}")).ok()?;
	let host = sysinfo::System::host_name().unwrap_or_else(|_| "elamon".into());
	let host: String = host.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' }).collect();
	let props: HashMap<String, String> = [
		("id".to_string(), identity.id.clone()),
		("api".to_string(), API_VERSION.to_string()),
		("fp".to_string(), identity.fingerprint[..16].to_string()),
	]
	.into();
	let info = mdns_sd::ServiceInfo::new(SERVICE_TYPE, name, &format!("{host}.local."), (), port, props)
		.map_err(|e| eprintln!("mdns: {e}"))
		.ok()?
		.enable_addr_auto();
	daemon.register(info).map_err(|e| eprintln!("mdns: {e}")).ok()?;
	Some(daemon)
}

fn pair() -> Result<(), String> {
	let config = store::load_config()?;
	let identity = store::Identity::load_or_create().map_err(|e| format!("can't create the TLS identity: {e}"))?;
	let name = machine_name(&config);
	let pairing = store::new_pairing().map_err(|e| format!("can't save the pairing code: {e}"))?;
	let before: Vec<String> = store::load_clients().into_iter().map(|c| c.id).collect();

	let addrs = collect::local_addresses();
	let hosts: Vec<String> = addrs.iter().map(|a| a.to_string()).collect();
	let uri = format!(
		"elamon://pair?v=1&id={}&name={}&port={}&fp={}&code={}&h={}",
		identity.id,
		percent_encode(&name),
		config.port,
		identity.fingerprint,
		pairing.code,
		hosts.join(",")
	);

	let tty = std::io::stdout().is_terminal();
	if let Ok(qr) = qrcode::QrCode::new(uri.as_bytes()) {
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
	println!("  Scan the code with the phone's camera, or in Elamon tap Add and enter:");
	println!();
	println!("      {code}");
	println!();
	println!("  Computer:     {name}");
	println!("  Addresses:    {}  port {}", if hosts.is_empty() { "none found".into() } else { hosts.join(", ") }, config.port);
	println!("  Certificate:  {}", store::pretty_fingerprint(&identity.fingerprint));
	println!("  The app shows the same certificate digits when you pair by code; they must match.");
	println!();
	if !service_running(config.port) {
		println!("  ! The service isn't answering on port {}. Start it: systemctl --user start elamond", config.port);
		println!();
	}
	println!("  Waiting for the phone… (the code works for {} minutes, Ctrl+C to stop)", store::PAIRING_TTL / 60);

	let deadline = std::time::Instant::now() + Duration::from_secs(store::PAIRING_TTL);
	while std::time::Instant::now() < deadline {
		std::thread::sleep(Duration::from_millis(500));
		if let Some(c) = store::load_clients().into_iter().find(|c| !before.contains(&c.id)) {
			println!("  Paired with \"{}\".", c.name);
			return Ok(());
		}
		if store::load_pairing().is_none() {
			return Err("the code was used up by wrong attempts; run `elamond pair` again".into());
		}
	}
	store::clear_pairing();
	Err("the code expired; run `elamond pair` again".into())
}

fn service_running(port: u16) -> bool {
	std::net::TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], port)), Duration::from_millis(300)).is_ok()
}

fn percent_encode(s: &str) -> String {
	s.bytes()
		.map(|b| match b {
			b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
			_ => format!("%{b:02X}"),
		})
		.collect()
}

fn clients() -> Result<(), String> {
	let clients = store::load_clients();
	if clients.is_empty() {
		println!("No phones are paired. Pair one with `elamond pair`.");
	}
	for c in clients {
		println!("{}  {}  (paired {})", c.id, c.name, format_date(c.created));
	}
	Ok(())
}

fn revoke(which: Option<&str>) -> Result<(), String> {
	let which = which.ok_or("which phone? `elamond clients` lists their ids")?;
	let clients = store::load_clients();
	let keep: Vec<_> = clients.iter().filter(|c| which != "all" && c.id != which).cloned().collect();
	if keep.len() == clients.len() {
		return Err(format!("no paired phone has the id \"{which}\""));
	}
	store::save_clients(&keep).map_err(|e| e.to_string())?;
	println!("Unpaired {} phone(s).", clients.len() - keep.len());
	Ok(())
}

fn snapshot() -> Result<(), String> {
	let mut c = collect::Collector::new();
	c.read(true, true);
	std::thread::sleep(Duration::from_secs(1));
	let reading = c.read(true, true);
	let config = store::load_config()?;
	let (interval, history_len) = timing(&config);
	let info = c.info("-", &machine_name(&config), config.control, interval, history_len);
	let mut procs = reading.processes.unwrap_or_default();
	procs.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
	procs.truncate(10);
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
