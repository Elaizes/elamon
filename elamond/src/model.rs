//! What the service sends to the app. Field names are part of the protocol
//! (see docs/protocol.md): add fields freely, never rename or repurpose one.

use serde::Serialize;

/// Facts about the machine that rarely change.
#[derive(Serialize, Clone, Debug)]
pub struct Info {
	pub id: String,
	pub name: String,
	pub service_version: &'static str,
	pub api: u32,
	/// The oldest app version that understands this service.
	pub min_app: &'static str,
	/// Milliseconds between the service's readings; a stream can't be faster.
	pub interval_ms: u64,
	/// How far back `history` reaches, in seconds.
	pub history_seconds: u64,
	pub os: String,
	pub kernel: String,
	pub arch: String,
	pub cpu_model: String,
	pub cpu_physical_cores: Option<usize>,
	pub cpu_logical_cores: usize,
	pub memory_total: u64,
	pub boot_time: u64,
	pub gpus: Vec<GpuInfo>,
	pub capabilities: Vec<&'static str>,
}

#[derive(Serialize, Clone, Debug)]
pub struct GpuInfo {
	/// PCI slot like "0000:01:00.0", stable across boots.
	pub id: String,
	pub vendor: &'static str,
	pub name: String,
	pub driver: String,
	/// The GPU that the firmware showed the boot screen on, usually the integrated one.
	pub boot_display: bool,
}

/// One reading of everything that changes. Processes are added per client.
#[derive(Serialize, Clone, Debug, Default)]
pub struct Sample {
	/// Unix time in milliseconds.
	pub t: u64,
	pub uptime: u64,
	pub cpu: Cpu,
	pub memory: Memory,
	pub gpus: Vec<Gpu>,
	pub temps: Vec<Temp>,
	pub fans: Vec<Fan>,
	pub battery: Option<Battery>,
	pub power: Power,
	pub disks: Vec<Disk>,
	pub disk_io: Vec<DiskIo>,
	pub net: Vec<Net>,
	pub displays: Vec<Display>,
	/// Web services this computer hosts, while an app is connected.
	pub services: Vec<crate::services::ServiceStatus>,
	/// Media players and the volume, for apps that asked for them.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub media: Option<crate::control::Media>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Cpu {
	/// Whole CPU, 0–100.
	pub usage: f32,
	/// Each logical core, 0–100.
	pub cores: Vec<f32>,
	/// Each logical core in MHz.
	pub freq: Vec<u32>,
	/// Package temperature in °C, when a sensor reports it.
	pub temp: Option<f32>,
	pub load: [f32; 3],
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Memory {
	pub total: u64,
	pub used: u64,
	pub available: u64,
	pub swap_total: u64,
	pub swap_used: u64,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Gpu {
	pub id: String,
	/// "active", "sleeping" (powered down by runtime PM) or "unknown".
	pub state: &'static str,
	/// 0–100.
	pub usage: Option<f32>,
	pub mem_used: Option<u64>,
	pub mem_total: Option<u64>,
	pub temp: Option<f32>,
	pub power: Option<f32>,
	pub freq: Option<u32>,
	pub freq_max: Option<u32>,
	pub mem_freq: Option<u32>,
	pub fan: Option<u32>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Temp {
	/// The hwmon driver, like "coretemp" or "nvme".
	pub chip: String,
	/// "cpu", "gpu", "storage", "wireless", "battery", "board" or "other".
	pub kind: &'static str,
	/// The device behind the chip when it has a model name, like an SSD.
	pub device: Option<String>,
	pub label: String,
	pub value: f32,
	pub high: Option<f32>,
	pub crit: Option<f32>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Fan {
	pub chip: String,
	pub label: String,
	pub rpm: u32,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Battery {
	pub name: String,
	pub model: Option<String>,
	/// 0–100.
	pub percent: Option<f32>,
	/// "charging", "discharging", "full", "not_charging" or "unknown".
	pub status: String,
	/// Watts going in or out, always positive.
	pub power: Option<f32>,
	pub energy_now: Option<f32>,
	pub energy_full: Option<f32>,
	pub energy_design: Option<f32>,
	pub cycles: Option<u32>,
	/// Seconds until empty or full at the current rate.
	pub time_left: Option<u64>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Power {
	pub on_ac: Option<bool>,
	/// ACPI platform profile, like "balanced".
	pub profile: Option<String>,
	pub profiles: Vec<String>,
	pub governor: Option<String>,
	/// Intel/AMD energy-performance preference, like "balance_power".
	pub epp: Option<String>,
	/// power-profiles-daemon's profile, like "power-saver", which the app can switch.
	pub ppd_profile: Option<String>,
	pub ppd_profiles: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Disk {
	pub device: String,
	pub mount: String,
	pub fs: String,
	pub total: u64,
	pub available: u64,
	pub removable: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct DiskIo {
	pub name: String,
	pub model: Option<String>,
	/// Bytes per second.
	pub read: f64,
	pub write: f64,
}

#[derive(Serialize, Clone, Debug)]
pub struct Net {
	pub name: String,
	/// "wired", "wireless", "vpn" or "virtual".
	pub kind: &'static str,
	pub up: bool,
	pub addrs: Vec<String>,
	/// Bytes per second.
	pub rx: f64,
	pub tx: f64,
	pub rx_total: u64,
	pub tx_total: u64,
	/// Mbit/s, when the link reports it.
	pub speed: Option<u64>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Display {
	/// Connector name like "DP-1", as compositors call it.
	pub connector: String,
	/// PCI slot of the GPU the connector belongs to.
	pub gpu: String,
	pub connected: bool,
	/// Lit and showing a picture (has a mode and DPMS is on).
	pub active: bool,
	pub dpms: Option<String>,
	pub internal: bool,
	pub vendor: Option<String>,
	pub model: Option<String>,
	pub year: Option<u16>,
	/// Panel size in millimetres.
	pub width_mm: Option<u32>,
	pub height_mm: Option<u32>,
	pub mode: Option<Mode>,
	pub preferred: Option<Mode>,
	pub max_refresh: Option<f32>,
	pub position: Option<[u32; 2]>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Mode {
	pub width: u32,
	pub height: u32,
	pub refresh: f32,
}

#[derive(Serialize, Clone, Debug)]
pub struct Process {
	pub pid: u32,
	pub ppid: Option<u32>,
	pub name: String,
	pub cmd: String,
	pub user: Option<String>,
	/// Share of the whole CPU, 0–100.
	pub cpu: f32,
	/// Resident memory in bytes.
	pub mem: u64,
	pub status: String,
	pub start: u64,
	/// Bytes per second.
	pub read: f64,
	pub write: f64,
	pub gpu: Option<f32>,
	pub gpu_mem: Option<u64>,
}

/// The last minutes of the main series, column by column, oldest first.
#[derive(Serialize, Clone, Debug, Default)]
pub struct History {
	pub t: Vec<u64>,
	pub cpu: Vec<f32>,
	pub cpu_temp: Vec<Option<f32>>,
	pub mem: Vec<u64>,
	pub swap: Vec<u64>,
	pub rx: Vec<f64>,
	pub tx: Vec<f64>,
	pub read: Vec<f64>,
	pub write: Vec<f64>,
	pub battery_power: Vec<Option<f32>>,
	/// Per GPU id: usage and temperature, None while it sleeps.
	pub gpus: Vec<GpuHistory>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct GpuHistory {
	pub id: String,
	pub usage: Vec<Option<f32>>,
	pub temp: Vec<Option<f32>>,
}

/// One decimal is all the app shows and it keeps the JSON short.
pub fn r1(x: f32) -> f32 {
	(x * 10.0).round() / 10.0
}

pub fn r1_64(x: f64) -> f64 {
	(x * 10.0).round() / 10.0
}
