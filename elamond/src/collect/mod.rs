//! Takes a reading of the machine once per tick.

mod display;
pub mod gpu;
pub mod sysfs;

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use sysinfo::{
	CpuRefreshKind, Disks, MemoryRefreshKind, Networks, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System,
	UpdateKind, Users,
};

use crate::model::*;
use sysfs::{read, read_num};

/// Ticks between slow refreshes: disk usage, display modes, user names, and while no app
/// is connected, sensors and battery too.
const SLOW_EVERY: u32 = 5;
/// Ticks between looks for new sensors (a plugged-in drive, a GPU driver loaded late).
const RESCAN_EVERY: u32 = 60;
const CMD_MAX: usize = 400;

pub struct Collector {
	sys: System,
	networks: Networks,
	disks: Disks,
	users: Users,
	cards: Vec<gpu::Card>,
	gpus: gpu::GpuReader,
	last: Instant,
	tick: u32,
	disk_prev: HashMap<String, (u64, u64)>,
	cached_disks: Vec<Disk>,
	cached_displays: Vec<Display>,
	displays_fresh: bool,
	sensor_set: sysfs::SensorSet,
	sensors: sysfs::Sensors,
	battery: Option<Battery>,
	power: Power,
	net_prev: HashMap<String, (u64, u64)>,
	net_meta: HashMap<String, NetMeta>,
}

struct NetMeta {
	kind: &'static str,
	up: bool,
	speed: Option<u64>,
	addrs: Vec<String>,
}

pub struct Reading {
	pub sample: Sample,
	pub processes: Option<Vec<Process>>,
}

impl Collector {
	pub fn new() -> Self {
		let refresh = RefreshKind::nothing()
			.with_cpu(CpuRefreshKind::everything())
			.with_memory(MemoryRefreshKind::everything());
		let sys = System::new_with_specifics(refresh).expect("system information is unavailable");
		Collector {
			sys,
			networks: Networks::new_with_refreshed_list().expect("network information is unavailable"),
			disks: Disks::new_with_refreshed_list().expect("disk information is unavailable"),
			users: Users::new_with_refreshed_list().expect("user information is unavailable"),
			cards: gpu::cards(),
			gpus: gpu::GpuReader::new(),
			last: Instant::now(),
			tick: 0,
			disk_prev: HashMap::new(),
			cached_disks: Vec::new(),
			cached_displays: Vec::new(),
			displays_fresh: false,
			sensor_set: sysfs::SensorSet::scan(),
			sensors: sysfs::Sensors::default(),
			battery: None,
			power: Power::default(),
			net_prev: HashMap::new(),
			net_meta: HashMap::new(),
		}
	}

	pub fn info(&self, id: &str, name: &str) -> Info {
		let cpu_model = self
			.sys
			.cpus()
			.first()
			.map(|c| c.brand().trim().to_string())
			.filter(|b| !b.is_empty())
			.unwrap_or_else(|| "CPU".into());
		let mut gpus: Vec<GpuInfo> = self.cards.iter().map(|c| c.info.clone()).collect();
		for g in &mut gpus {
			if let Some(n) = self.gpus.nvidia_name(&g.id) {
				g.name = n.clone();
			}
		}
		Info {
			id: id.to_string(),
			name: name.to_string(),
			service_version: env!("CARGO_PKG_VERSION"),
			api: crate::API_VERSION,
			os: sysfs::os_name().or_else(|| System::long_os_version().ok()).unwrap_or_else(|| "Linux".into()),
			kernel: System::kernel_version().unwrap_or_default(),
			arch: System::cpu_arch(),
			cpu_model,
			cpu_physical_cores: System::physical_core_count().ok(),
			cpu_logical_cores: self.sys.cpus().len(),
			memory_total: self.sys.total_memory(),
			boot_time: System::boot_time().unwrap_or(0),
			gpus,
			capabilities: vec!["processes", "displays", "history", "sensors"],
		}
	}

	/// `watched`: an app is connected, so heavier sources (NVML, DRM) are worth reading.
	/// `want_procs`: some app shows the process list.
	pub fn read(&mut self, watched: bool, want_procs: bool) -> Reading {
		let now = Instant::now();
		let secs = now.duration_since(self.last).as_secs_f64().max(0.001);
		self.last = now;
		let slow = self.tick % SLOW_EVERY == 0;
		if self.tick % RESCAN_EVERY == RESCAN_EVERY - 1 {
			self.sensor_set = sysfs::SensorSet::scan();
		}
		self.tick = self.tick.wrapping_add(1);

		// Per-core clocks are only shown, never kept in history.
		if watched {
			self.sys.refresh_cpu_all();
		} else {
			self.sys.refresh_cpu_usage();
		}
		self.sys.refresh_memory();

		if watched || slow {
			let sleeping: Vec<PathBuf> =
				self.cards.iter().filter(|c| c.sleeping()).map(|c| c.device.clone()).collect();
			self.sensors = self.sensor_set.read(&sleeping);
			self.battery = sysfs::battery();
			self.power = sysfs::power();
		}
		let sensors = self.sensors.clone();

		let cpus = self.sys.cpus();
		let load = System::load_average().unwrap_or_default();
		let cpu = Cpu {
			usage: r1(self.sys.global_cpu_usage()),
			cores: cpus.iter().map(|c| r1(c.usage())).collect(),
			freq: cpus.iter().map(|c| c.frequency() as u32).collect(),
			temp: sensors.cpu_temp,
			load: [r1(load.one as f32), r1(load.five as f32), r1(load.fifteen as f32)],
		};
		let memory = Memory {
			total: self.sys.total_memory(),
			used: self.sys.used_memory(),
			available: self.sys.available_memory(),
			swap_total: self.sys.total_swap(),
			swap_used: self.sys.used_swap(),
		};

		if !watched {
			self.gpus.release();
		}
		let gpus = self.gpus.read(&self.cards, watched);

		if slow || self.cached_disks.is_empty() {
			self.disks.refresh(true);
			self.cached_disks = disks(&self.disks);
		}
		if watched && (slow || !self.displays_fresh) {
			self.cached_displays = display::displays(&self.cards);
			self.displays_fresh = true;
		} else if !watched {
			self.displays_fresh = false;
		}
		if slow {
			self.users.refresh();
		}

		let processes = if want_procs {
			Some(self.processes(secs))
		} else {
			None
		};

		let sample = Sample {
			t: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0),
			uptime: System::uptime().unwrap_or(0),
			cpu,
			memory,
			gpus,
			temps: sensors.temps,
			fans: sensors.fans,
			battery: self.battery.clone(),
			power: self.power.clone(),
			disks: self.cached_disks.clone(),
			disk_io: self.disk_io(secs),
			net: self.net(secs, watched && slow),
			displays: self.cached_displays.clone(),
		};
		Reading { sample, processes }
	}

	fn processes(&mut self, secs: f64) -> Vec<Process> {
		let kind = ProcessRefreshKind::nothing()
			.with_cpu()
			.with_memory()
			.with_disk_usage()
			.with_user(UpdateKind::OnlyIfNotSet)
			.with_cmd(UpdateKind::OnlyIfNotSet)
			.with_gpu_usage()
			.with_gpu_memory()
			.without_tasks();
		self.sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
		let ncpu = self.sys.cpus().len().max(1) as f32;
		self.sys
			.processes()
			.values()
			.filter(|p| p.thread_kind().is_none())
			// Kernel threads are children of kthreadd (pid 2) and have no command line.
			.filter(|p| p.pid().as_u32() != 2 && p.parent().map(|pp| pp.as_u32()) != Some(2))
			.map(|p| {
				let cmd: Vec<String> = p.cmd().iter().map(|a| a.to_string_lossy().into_owned()).collect();
				let mut cmd = cmd.join(" ");
				if cmd.len() > CMD_MAX {
					let mut cut = CMD_MAX;
					while !cmd.is_char_boundary(cut) {
						cut -= 1;
					}
					cmd.truncate(cut);
					cmd.push('…');
				}
				let disk = p.disk_usage();
				Process {
					pid: p.pid().as_u32(),
					ppid: p.parent().map(|pp| pp.as_u32()),
					name: process_name(&p.name().to_string_lossy(), p.cmd().first()),
					cmd,
					user: p.user_id().and_then(|u| self.users.get_user_by_id(u)).map(|u| u.name().to_string()),
					cpu: r1(p.cpu_usage() / ncpu),
					mem: p.memory(),
					status: p.status().to_string(),
					start: p.start_time(),
					read: r1_64(disk.read_bytes as f64 / secs),
					write: r1_64(disk.written_bytes as f64 / secs),
					gpu: p.gpu_usage().map(r1).filter(|g| *g > 0.0),
					gpu_mem: p.gpu_memory().filter(|m| *m > 0),
				}
			})
			.collect()
	}

	/// Counters come from /proc/net/dev, one read for every interface. Addresses, link state
	/// and speed change rarely and cost a read each, so they're refreshed on slow ticks.
	fn net(&mut self, secs: f64, refresh_meta: bool) -> Vec<Net> {
		let Some(text) = read("/proc/net/dev") else { return Vec::new() };
		let counters: Vec<(String, u64, u64)> = text
			.lines()
			.skip(2)
			.filter_map(|line| {
				let (name, rest) = line.split_once(':')?;
				let f: Vec<u64> = rest.split_whitespace().filter_map(|x| x.parse().ok()).collect();
				Some((name.trim().to_string(), *f.first()?, *f.get(8)?))
			})
			.filter(|(name, _, _)| name != "lo")
			.collect();

		let unknown = counters.iter().any(|(n, _, _)| !self.net_meta.contains_key(n));
		if refresh_meta || unknown {
			self.networks.refresh(true);
			self.net_meta.clear();
			for (name, _, _) in &counters {
				let dir = Path::new("/sys/class/net").join(name);
				let addrs = self
					.networks
					.iter()
					.find(|(n, _)| n.to_string_lossy() == name.as_str())
					.map(|(_, d)| d.ip_networks().iter().map(|n| n.addr.to_string()).collect())
					.unwrap_or_default();
				self.net_meta.insert(
					name.clone(),
					NetMeta {
						kind: net_kind(name),
						up: matches!(read(dir.join("operstate")).as_deref(), Some("up") | Some("unknown"))
							&& read_num::<u8>(dir.join("carrier")) != Some(0),
						speed: read_num::<i64>(dir.join("speed")).filter(|s| *s > 0).map(|s| s as u64),
						addrs,
					},
				);
			}
		}

		let mut out: Vec<Net> = counters
			.into_iter()
			.map(|(name, rx, tx)| {
				let (prx, ptx) = self.net_prev.insert(name.clone(), (rx, tx)).unwrap_or((rx, tx));
				let meta = self.net_meta.get(&name);
				Net {
					kind: meta.map_or("virtual", |m| m.kind),
					up: meta.is_some_and(|m| m.up),
					addrs: meta.map(|m| m.addrs.clone()).unwrap_or_default(),
					speed: meta.and_then(|m| m.speed),
					rx: r1_64(rx.saturating_sub(prx) as f64 / secs),
					tx: r1_64(tx.saturating_sub(ptx) as f64 / secs),
					rx_total: rx,
					tx_total: tx,
					name,
				}
			})
			.collect();
		out.sort_by(|a, b| kind_order(a.kind).cmp(&kind_order(b.kind)).then(sysfs::natural_cmp(&a.name, &b.name)));
		out
	}

	fn disk_io(&mut self, secs: f64) -> Vec<DiskIo> {
		let Some(stats) = read("/proc/diskstats") else { return Vec::new() };
		let mut out = Vec::new();
		for line in stats.lines() {
			let f: Vec<&str> = line.split_whitespace().collect();
			if f.len() < 10 {
				continue;
			}
			let name = f[2];
			// Whole physical disks only: partitions, loop, zram and device-mapper have no device/.
			let block = Path::new("/sys/block").join(name);
			if !block.join("device").exists() {
				continue;
			}
			let sectors_read: u64 = f[5].parse().unwrap_or(0);
			let sectors_written: u64 = f[9].parse().unwrap_or(0);
			let (r, w) = (sectors_read * 512, sectors_written * 512);
			let (pr, pw) = self.disk_prev.insert(name.to_string(), (r, w)).unwrap_or((r, w));
			out.push(DiskIo {
				name: name.to_string(),
				model: read(block.join("device/model")),
				read: r1_64(r.saturating_sub(pr) as f64 / secs),
				write: r1_64(w.saturating_sub(pw) as f64 / secs),
			});
		}
		out
	}
}

/// The kernel's name for a process is cut at 15 characters, and Nix wrappers rename programs
/// to ".name-wrapped"; argv[0] then says what people know the program as.
fn process_name(comm: &str, argv0: Option<&std::ffi::OsString>) -> String {
	let arg = argv0
		.map(|a| a.to_string_lossy())
		.and_then(|a| a.split_whitespace().next().map(str::to_string))
		.map(|a| a.rsplit('/').next().unwrap_or("").trim_end_matches(':').to_string())
		.filter(|a| !a.is_empty());
	let wrapped = comm.starts_with('.') && comm.contains("-wra");
	let name = match arg {
		Some(a) if comm.len() >= 15 && a.starts_with(comm) => a,
		Some(a) if wrapped => a,
		_ => comm.to_string(),
	};
	// argv[0] of a wrapped program is sometimes the wrapper's own name too.
	match name.strip_prefix('.').and_then(|n| n.strip_suffix("-wrapped")) {
		Some(inner) if !inner.is_empty() => inner.to_string(),
		_ => name,
	}
}

fn kind_order(kind: &str) -> u8 {
	match kind {
		"wired" => 0,
		"wireless" => 1,
		"vpn" => 2,
		_ => 3,
	}
}

fn disks(disks: &Disks) -> Vec<Disk> {
	const SKIP_FS: [&str; 9] = ["overlay", "squashfs", "ramfs", "efivarfs", "devtmpfs", "tmpfs", "nsfs", "autofs", "erofs"];
	let mut by_device: HashMap<String, Disk> = HashMap::new();
	for d in disks.iter() {
		let fs = d.file_system().to_string_lossy().into_owned();
		if SKIP_FS.contains(&fs.as_str()) || fs.starts_with("fuse") || d.total_space() == 0 {
			continue;
		}
		let device = d.name().to_string_lossy().into_owned();
		let mount = d.mount_point().to_string_lossy().into_owned();
		let disk = Disk {
			device: device.clone(),
			mount: mount.clone(),
			fs,
			total: d.total_space(),
			available: d.available_space(),
			removable: d.is_removable(),
		};
		// One filesystem mounted many times (btrfs subvolumes, bind mounts) is shown once, at its shortest path.
		match by_device.get(&device) {
			Some(have) if have.mount.len() <= mount.len() => {}
			_ => {
				by_device.insert(device, disk);
			}
		}
	}
	let mut v: Vec<Disk> = by_device.into_values().collect();
	v.sort_by(|a, b| a.mount.len().cmp(&b.mount.len()).then(a.mount.cmp(&b.mount)));
	v
}

/// The main series over the last minutes, kept so the app's charts are full the moment it connects.
pub struct HistoryRing {
	cap: usize,
	points: VecDeque<Point>,
}

struct Point {
	t: u64,
	cpu: f32,
	cpu_temp: Option<f32>,
	mem: u64,
	swap: u64,
	rx: f64,
	tx: f64,
	read: f64,
	write: f64,
	battery_power: Option<f32>,
	gpus: Vec<(String, Option<f32>, Option<f32>)>,
}

impl HistoryRing {
	pub fn new(cap: usize) -> Self {
		HistoryRing { cap, points: VecDeque::with_capacity(cap) }
	}

	pub fn push(&mut self, s: &Sample) {
		if self.points.len() == self.cap {
			self.points.pop_front();
		}
		// Virtual links mirror real traffic (bridges, VPN over Wi-Fi); counting them would double it.
		let real = |n: &&Net| n.kind == "wired" || n.kind == "wireless";
		self.points.push_back(Point {
			t: s.t,
			cpu: s.cpu.usage,
			cpu_temp: s.cpu.temp,
			mem: s.memory.used,
			swap: s.memory.swap_used,
			rx: s.net.iter().filter(real).map(|n| n.rx).sum(),
			tx: s.net.iter().filter(real).map(|n| n.tx).sum(),
			read: s.disk_io.iter().map(|d| d.read).sum(),
			write: s.disk_io.iter().map(|d| d.write).sum(),
			battery_power: s.battery.as_ref().and_then(|b| b.power),
			gpus: s.gpus.iter().map(|g| (g.id.clone(), g.usage, g.temp)).collect(),
		});
	}

	pub fn export(&self) -> History {
		let p = &self.points;
		let mut ids: Vec<String> = Vec::new();
		for point in p {
			for (id, _, _) in &point.gpus {
				if !ids.contains(id) {
					ids.push(id.clone());
				}
			}
		}
		let gpus = ids
			.into_iter()
			.map(|id| {
				let find = |point: &Point| point.gpus.iter().find(|g| g.0 == id).map(|g| (g.1, g.2));
				GpuHistory {
					usage: p.iter().map(|pt| find(pt).and_then(|g| g.0)).collect(),
					temp: p.iter().map(|pt| find(pt).and_then(|g| g.1)).collect(),
					id,
				}
			})
			.collect();
		History {
			t: p.iter().map(|x| x.t).collect(),
			cpu: p.iter().map(|x| x.cpu).collect(),
			cpu_temp: p.iter().map(|x| x.cpu_temp).collect(),
			mem: p.iter().map(|x| x.mem).collect(),
			swap: p.iter().map(|x| x.swap).collect(),
			rx: p.iter().map(|x| x.rx).collect(),
			tx: p.iter().map(|x| x.tx).collect(),
			read: p.iter().map(|x| x.read).collect(),
			write: p.iter().map(|x| x.write).collect(),
			battery_power: p.iter().map(|x| x.battery_power).collect(),
			gpus,
		}
	}
}

/// "wired", "wireless", "vpn" or "virtual" (bridges, containers).
pub fn net_kind(name: &str) -> &'static str {
	let dir = Path::new("/sys/class/net").join(name);
	if dir.join("wireless").exists() || dir.join("phy80211").exists() {
		"wireless"
	} else if read(dir.join("type")).as_deref() == Some("65534")
		|| ["tun", "tap", "wg", "tailscale", "zt", "ppp"].iter().any(|p| name.starts_with(p))
	{
		"vpn"
	} else if dir.join("device").exists() {
		"wired"
	} else {
		"virtual"
	}
}

/// IPv4 addresses a phone could reach this computer on: the local network first, then VPNs.
pub fn local_addresses() -> Vec<IpAddr> {
	let Ok(networks) = Networks::new_with_refreshed_list() else { return Vec::new() };
	let mut found: Vec<(u8, IpAddr)> = Vec::new();
	for (name, data) in networks.iter() {
		let rank = match net_kind(&name.to_string_lossy()) {
			"wired" => 0,
			"wireless" => 1,
			"vpn" => 2,
			_ => continue,
		};
		for n in data.ip_networks() {
			if let IpAddr::V4(v4) = n.addr
				&& !v4.is_loopback()
				&& !v4.is_link_local()
				&& !found.iter().any(|(_, a)| *a == n.addr)
			{
				found.push((rank, n.addr));
			}
		}
	}
	found.sort_by_key(|(rank, _)| *rank);
	found.into_iter().map(|(_, a)| a).collect()
}
