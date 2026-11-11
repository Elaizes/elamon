//! Takes a reading of the machine once per tick.

mod display;
pub mod gpu;
pub mod sysfs;

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use sysinfo::{
	CpuRefreshKind, Disks, MemoryRefreshKind, Networks, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System,
	UpdateKind, Users,
};

use crate::consts::cadence::Ticks;
use crate::consts::{limit, protocol};
use crate::model::*;
use sysfs::{read, read_num};

pub struct Collector {
	sys: System,
	networks: Networks,
	disks: Disks,
	users: Users,
	cards: Vec<gpu::Card>,
	gpus: gpu::GpuReader,
	every: Ticks,
	last: Instant,
	tick: u32,
	/// The tick processes were last read on, and when.
	procs_read: Option<(u32, Instant)>,
	/// What each process had used by then, by pid and start time (pids are reused).
	procs_used: HashMap<(u32, u64), ProcUse>,
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

/// A process's running totals: CPU time in milliseconds, bytes read and written.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
struct ProcUse {
	cpu_ms: u64,
	read: u64,
	written: u64,
}

impl ProcUse {
	/// Share of the whole CPU (0–100) and bytes per second since `before`, `secs` ago.
	fn since(self, before: ProcUse, secs: f64, ncpu: usize) -> (f32, f64, f64) {
		let secs = secs.max(0.001);
		let cpu = self.cpu_ms.saturating_sub(before.cpu_ms) as f64 / (secs * 1000.0 * ncpu.max(1) as f64) * 100.0;
		let rate = |now: u64, then: u64| now.saturating_sub(then) as f64 / secs;
		(cpu.min(100.0) as f32, rate(self.read, before.read), rate(self.written, before.written))
	}
}

impl Collector {
	pub fn new(every: Ticks) -> Self {
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
			every,
			last: Instant::now(),
			tick: 0,
			procs_read: None,
			procs_used: HashMap::new(),
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

	pub fn info(&self, id: &str, name: &str, control: bool, interval: std::time::Duration, history_len: usize) -> Info {
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
			api: protocol::API_VERSION,
			min_app: protocol::MIN_APP,
			interval_ms: interval.as_millis() as u64,
			history_seconds: (interval * history_len as u32).as_secs(),
			os: sysfs::os_name().or_else(|| System::long_os_version().ok()).unwrap_or_else(|| "Linux".into()),
			kernel: System::kernel_version().unwrap_or_default(),
			arch: System::cpu_arch(),
			cpu_model,
			cpu_physical_cores: System::physical_core_count().ok(),
			cpu_logical_cores: self.sys.cpus().len(),
			memory_total: self.sys.total_memory(),
			boot_time: System::boot_time().unwrap_or(0),
			gpus,
			capabilities: {
				let mut c = vec!["processes", "displays", "history", "sensors", "links", "media"];
				if control {
					c.push("control");
				}
				c
			},
		}
	}

	/// `watched`: an app is connected, so heavier sources (NVML, DRM) are worth reading.
	/// `want_procs`: some app shows the process list. The first reading after a tick without
	/// one only sets where the processes' rates count from, and has no processes.
	pub fn read(&mut self, watched: bool, want_procs: bool) -> Reading {
		let now = Instant::now();
		let secs = now.duration_since(self.last).as_secs_f64().max(0.001);
		self.last = now;
		let slow = self.tick.is_multiple_of(self.every.slow);
		if self.tick % self.every.rescan == self.every.rescan - 1 {
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

		let processes = if want_procs { self.processes() } else { None };

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
			links: Vec::new(),
		};
		Reading { sample, processes }
	}

	/// CPU and disk rates are worked out here from each process's totals over the real time
	/// since the previous reading, which must be from the tick before: sysinfo's own figures
	/// count from its last refresh of anything, so after a pause they'd be divided by the
	/// wrong time, and the first ever reading would be lifetime totals.
	fn processes(&mut self) -> Option<Vec<Process>> {
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
		let now = Instant::now();
		let secs = self
			.procs_read
			.filter(|(tick, _)| tick.wrapping_add(1) == self.tick)
			.map(|(_, at)| now.duration_since(at).as_secs_f64());
		self.procs_read = Some((self.tick, now));
		let ncpu = self.sys.cpus().len();
		let mut used_now = HashMap::with_capacity(self.procs_used.len());
		let mut out = Vec::new();
		// Kernel threads are children of kthreadd (pid 2) and have no command line.
		let shown = |p: &&sysinfo::Process| {
			p.thread_kind().is_none() && p.pid().as_u32() != 2 && p.parent().map(|pp| pp.as_u32()) != Some(2)
		};
		for p in self.sys.processes().values().filter(shown) {
			let disk = p.disk_usage();
			let used = ProcUse { cpu_ms: p.accumulated_cpu_time(), read: disk.total_read_bytes, written: disk.total_written_bytes };
			let key = (p.pid().as_u32(), p.start_time());
			used_now.insert(key, used);
			let Some(secs) = secs else { continue };
			// One that wasn't there a tick ago started since, and all it used, it used since.
			let (cpu, read, write) = used.since(self.procs_used.get(&key).copied().unwrap_or_default(), secs, ncpu);
			let cmd: Vec<String> = p.cmd().iter().map(|a| a.to_string_lossy().into_owned()).collect();
			let mut cmd = cmd.join(" ");
			if cmd.len() > limit::CMD_MAX {
				let mut cut = limit::CMD_MAX;
				while !cmd.is_char_boundary(cut) {
					cut -= 1;
				}
				cmd.truncate(cut);
				cmd.push('…');
			}
			out.push(Process {
				pid: p.pid().as_u32(),
				ppid: p.parent().map(|pp| pp.as_u32()),
				name: process_name(&p.name().to_string_lossy(), p.cmd().first()),
				cmd,
				user: p.user_id().and_then(|u| self.users.get_user_by_id(u)).map(|u| u.name().to_string()),
				cpu: r1(cpu),
				mem: p.memory(),
				status: p.status().to_string(),
				start: p.start_time(),
				read: r1_64(read),
				write: r1_64(write),
				gpu: p.gpu_usage().map(r1).filter(|g| *g > 0.0),
				gpu_mem: p.gpu_memory().filter(|m| *m > 0),
			});
		}
		self.procs_used = used_now;
		secs.map(|_| out)
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

		// Interfaces that went away (a VPN down, a container stopped) are forgotten.
		self.net_prev.retain(|name, _| counters.iter().any(|(n, _, _)| n == name));
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
		// Disks that went away (a USB drive pulled) are forgotten.
		self.disk_prev.retain(|name, _| out.iter().any(|d| &d.name == name));
		out
	}
}

/// The kernel's name for a process is cut at 15 characters, and Nix wrappers turn a program's
/// name into ".name-wrapped"; argv[0] then says what people know the program as.
pub fn process_name(comm: &str, argv0: Option<&std::ffi::OsString>) -> String {
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

	pub fn capacity(&self) -> usize {
		self.cap
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

#[cfg(test)]
mod tests {
	use super::*;
	use std::ffi::OsString;

	#[test]
	fn process_names() {
		let argv0 = |s: &str| Some(OsString::from(s));
		// Nix wrappers: the kernel sees ".name-wrapped", cut to 15 characters.
		assert_eq!(process_name(".firefox-wrappe", argv0("/nix/store/abc-firefox/bin/firefox").as_ref()), "firefox");
		assert_eq!(process_name(".kitty-wrapped", argv0("/run/current-system/sw/bin/.kitty-wrapped").as_ref()), "kitty");
		assert_eq!(process_name(".kitty-wrapped", None), "kitty");
		// Long names are cut by the kernel; argv[0] has the rest.
		assert_eq!(process_name("gnome-shell-cal", argv0("/usr/libexec/gnome-shell-calendar-server").as_ref()), "gnome-shell-calendar-server");
		// A short name stays, whatever argv[0] says (Chromium's renderers rewrite it).
		assert_eq!(process_name("bash", argv0("-bash").as_ref()), "bash");
		assert_eq!(process_name("chrome", argv0("/opt/chrome/chrome --type=renderer").as_ref()), "chrome");
		assert_eq!(process_name("sshd", argv0("sshd: user@pts/0").as_ref()), "sshd");
	}

	#[test]
	fn process_rates_over_real_time() {
		let before = ProcUse { cpu_ms: 1_000, read: 0, written: 4096 };
		let now = ProcUse { cpu_ms: 3_000, read: 1_000_000, written: 4096 };
		// 2 s of CPU in 4 s on 2 cores: a quarter of the machine.
		assert_eq!(now.since(before, 4.0, 2), (25.0, 250_000.0, 0.0));
		// A process born since the last reading counts from nothing.
		assert_eq!(ProcUse { cpu_ms: 500, read: 0, written: 0 }.since(ProcUse::default(), 1.0, 1).0, 50.0);
		// Counters don't go backwards, and the share never passes the whole CPU.
		assert_eq!(before.since(now, 1.0, 1), (0.0, 0.0, 0.0));
		assert_eq!(now.since(ProcUse::default(), 0.5, 1).0, 100.0);
	}

	fn sample(t: u64, cpu: f32, nets: &[(&'static str, f64)], gpu: Option<f32>) -> Sample {
		Sample {
			t,
			cpu: Cpu { usage: cpu, ..Default::default() },
			net: nets
				.iter()
				.map(|&(kind, rx)| Net {
					name: kind.to_string(),
					kind,
					up: true,
					addrs: Vec::new(),
					rx,
					tx: 0.0,
					rx_total: 0,
					tx_total: 0,
					speed: None,
				})
				.collect(),
			gpus: gpu.map(|u| Gpu { id: "g".into(), usage: Some(u), ..Default::default() }).into_iter().collect(),
			..Default::default()
		}
	}

	#[test]
	fn history_keeps_the_newest_points() {
		let mut ring = HistoryRing::new(2);
		ring.push(&sample(1, 10.0, &[], None));
		ring.push(&sample(2, 20.0, &[("wired", 100.0), ("vpn", 100.0)], Some(5.0)));
		ring.push(&sample(3, 30.0, &[("wireless", 7.0)], None));
		let h = ring.export();
		assert_eq!(h.t, [2, 3]);
		assert_eq!(h.cpu, [20.0, 30.0]);
		// VPN traffic is already counted on the link it runs over.
		assert_eq!(h.rx, [100.0, 7.0]);
		assert_eq!(h.gpus.len(), 1);
		assert_eq!(h.gpus[0].usage, [Some(5.0), None]);
		assert_eq!(HistoryRing::new(3).export().t.len(), 0);
	}
}
