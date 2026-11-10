//! GPUs: found through DRM in sysfs, measured through NVML (NVIDIA) or sysfs (AMD, Intel).
//!
//! A hybrid laptop powers its discrete GPU down when nothing uses it. Opening it through
//! NVML wakes it and keeps it awake, so a sleeping GPU is reported as such and left alone,
//! and NVML is only held while someone is watching.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use nvml_wrapper::enum_wrappers::device::{Clock, TemperatureSensor};
use nvml_wrapper::Nvml;

use super::sysfs::{dir_entries, link_name, read, read_num, runtime_status};
use crate::model::{r1, Gpu, GpuInfo};

pub struct Card {
	pub info: GpuInfo,
	/// /sys/class/drm/cardN
	pub drm: PathBuf,
	/// The PCI device directory.
	pub device: PathBuf,
}

impl Card {
	pub fn sleeping(&self) -> bool {
		runtime_status(&self.device).as_deref() == Some("suspended")
	}
}

pub fn cards() -> Vec<Card> {
	let pci_names = PciNames::load();
	let mut cards = Vec::new();
	for (name, path) in dir_entries("/sys/class/drm") {
		if !name.starts_with("card") || name.contains('-') {
			continue;
		}
		let Ok(device) = fs::canonicalize(path.join("device")) else { continue };
		// simpledrm and other firmware framebuffers are not GPUs.
		let Some(vendor_id) = read(device.join("vendor")) else { continue };
		let driver = link_name(device.join("driver")).unwrap_or_default();
		let Some(slot) = device.file_name().map(|s| s.to_string_lossy().into_owned()) else { continue };
		let vendor = match vendor_id.as_str() {
			"0x10de" => "nvidia",
			"0x1002" => "amd",
			"0x8086" => "intel",
			_ => "other",
		};
		let device_id = read(device.join("device")).unwrap_or_default();
		let name = read(device.join("product_name"))
			.or_else(|| pci_names.lookup(&vendor_id, &device_id))
			.unwrap_or_else(|| {
				let v = match vendor {
					"nvidia" => "NVIDIA",
					"amd" => "AMD",
					"intel" => "Intel",
					_ => "",
				};
				format!("{v} GPU {}:{}", vendor_id.trim_start_matches("0x"), device_id.trim_start_matches("0x"))
					.trim()
					.to_string()
			});
		let boot_display = read_num::<u8>(device.join("boot_vga")) == Some(1);
		if cards.iter().any(|c: &Card| c.info.id == slot) {
			continue;
		}
		cards.push(Card {
			info: GpuInfo { id: slot, vendor, name, driver, boot_display },
			drm: path,
			device,
		});
	}
	cards
}

pub struct GpuReader {
	nvml: Option<Nvml>,
	nvml_failed: bool,
	nvml_names: HashMap<String, String>,
}

impl GpuReader {
	pub fn new() -> Self {
		GpuReader { nvml: None, nvml_failed: false, nvml_names: HashMap::new() }
	}

	/// Lets NVIDIA GPUs sleep again; called when no app is connected.
	pub fn release(&mut self) {
		self.nvml = None;
	}

	/// The marketing name NVML reports, better than the PCI database's.
	pub fn nvidia_name(&self, slot: &str) -> Option<&String> {
		self.nvml_names.get(slot)
	}

	pub fn read(&mut self, cards: &[Card], watched: bool) -> Vec<Gpu> {
		cards.iter().map(|c| self.read_card(c, watched)).collect()
	}

	fn read_card(&mut self, card: &Card, watched: bool) -> Gpu {
		let mut gpu = Gpu { id: card.info.id.clone(), state: "active", ..Default::default() };
		if card.sleeping() {
			gpu.state = "sleeping";
			return gpu;
		}
		match card.info.driver.as_str() {
			"nvidia" => {
				if watched {
					self.read_nvidia(card, &mut gpu);
				} else {
					gpu.state = "unknown";
				}
			}
			"amdgpu" => read_amd(card, &mut gpu),
			"i915" | "xe" => read_intel(card, &mut gpu),
			_ => {}
		}
		gpu
	}

	fn nvml(&mut self) -> Option<&Nvml> {
		if self.nvml.is_none() && !self.nvml_failed {
			self.nvml = open_nvml();
			// The library is either installed or not; no use retrying every second.
			self.nvml_failed = self.nvml.is_none();
		}
		self.nvml.as_ref()
	}

	fn read_nvidia(&mut self, card: &Card, gpu: &mut Gpu) {
		let slot = card.info.id.clone();
		let Some(nvml) = self.nvml() else {
			gpu.state = "unknown";
			return;
		};
		// NVML writes the PCI bus id with an 8-digit domain.
		let bus_id = format!("0000{}", slot.to_ascii_uppercase());
		let Ok(dev) = nvml.device_by_pci_bus_id(bus_id).or_else(|_| nvml.device_by_pci_bus_id(slot.as_str()))
		else {
			gpu.state = "unknown";
			return;
		};
		if let Ok(u) = dev.utilization_rates() {
			gpu.usage = Some(u.gpu as f32);
		}
		if let Ok(m) = dev.memory_info() {
			gpu.mem_used = Some(m.used);
			gpu.mem_total = Some(m.total);
		}
		gpu.temp = dev.temperature(TemperatureSensor::Gpu).ok().map(|t| t as f32);
		gpu.power = dev.power_usage().ok().map(|mw| r1(mw as f32 / 1000.0));
		gpu.freq = dev.clock_info(Clock::Graphics).ok();
		gpu.freq_max = dev.max_clock_info(Clock::Graphics).ok();
		gpu.mem_freq = dev.clock_info(Clock::Memory).ok();
		gpu.fan = dev.fan_speed(0).ok();
		let name = dev.name().ok();
		if let Some(name) = name {
			self.nvml_names.insert(slot, name);
		}
	}
}

fn open_nvml() -> Option<Nvml> {
	if let Ok(n) = Nvml::init() {
		return Some(n);
	}
	// NixOS keeps the driver's libraries outside the loader's search path.
	for dir in ["/run/opengl-driver/lib", "/usr/lib64", "/usr/lib/x86_64-linux-gnu", "/usr/lib"] {
		let path = Path::new(dir).join("libnvidia-ml.so.1");
		if path.exists()
			&& let Ok(n) = Nvml::builder().lib_path(OsStr::new(path.as_os_str())).init()
		{
			return Some(n);
		}
	}
	None
}

fn read_amd(card: &Card, gpu: &mut Gpu) {
	let d = &card.device;
	gpu.usage = read_num::<f32>(d.join("gpu_busy_percent"));
	gpu.mem_used = read_num(d.join("mem_info_vram_used"));
	gpu.mem_total = read_num(d.join("mem_info_vram_total"));
	if let Some((_, hw)) = dir_entries(d.join("hwmon")).into_iter().next() {
		// "edge" is the die's surface, the figure other tools show first.
		gpu.temp = read_num::<f32>(hw.join("temp1_input")).map(|m| r1(m / 1000.0));
		gpu.power = read_num::<f32>(hw.join("power1_average"))
			.or_else(|| read_num::<f32>(hw.join("power1_input")))
			.map(|uw| r1(uw / 1e6));
		gpu.freq = read_num::<u64>(hw.join("freq1_input")).map(|hz| (hz / 1_000_000) as u32);
		gpu.mem_freq = read_num::<u64>(hw.join("freq2_input")).map(|hz| (hz / 1_000_000) as u32);
		// pwm1 is the fan duty cycle out of 255; NVML reports the same thing in percent.
		gpu.fan = read_num::<u32>(hw.join("pwm1")).map(|p| p * 100 / 255);
	}
}

fn read_intel(card: &Card, gpu: &mut Gpu) {
	let drm = &card.drm;
	// i915 puts the frequencies on the card, xe per tile and GT.
	if let Some(freq) = read_num::<u32>(drm.join("gt_act_freq_mhz")) {
		gpu.freq = Some(freq);
		gpu.freq_max = read_num(drm.join("gt_RP0_freq_mhz")).or_else(|| read_num(drm.join("gt_max_freq_mhz")));
	} else {
		let gt = card.device.join("tile0/gt0/freq0");
		gpu.freq = read_num(gt.join("act_freq"));
		gpu.freq_max = read_num(gt.join("rp0_freq")).or_else(|| read_num(gt.join("max_freq")));
	}
}

/// Names from the pci.ids database, when the system has one.
struct PciNames {
	text: Option<String>,
}

impl PciNames {
	fn load() -> Self {
		let paths = [
			"/usr/share/hwdata/pci.ids",
			"/usr/share/misc/pci.ids",
			"/usr/share/pci.ids",
			"/run/current-system/sw/share/hwdata/pci.ids",
		];
		// Packagers can point at their copy, as the Nix package does.
		let own = std::env::var_os("ELAMON_PCI_IDS").and_then(|p| fs::read_to_string(p).ok());
		PciNames { text: own.or_else(|| paths.iter().find_map(|p| fs::read_to_string(p).ok())) }
	}

	fn lookup(&self, vendor: &str, device: &str) -> Option<String> {
		let text = self.text.as_ref()?;
		let vendor = vendor.trim_start_matches("0x");
		let device = device.trim_start_matches("0x");
		let mut in_vendor = false;
		for line in text.lines() {
			if line.starts_with('#') || line.is_empty() {
				continue;
			}
			if !line.starts_with('\t') {
				if in_vendor {
					return None;
				}
				in_vendor = line.starts_with(vendor);
			} else if in_vendor && !line.starts_with("\t\t") {
				let line = &line[1..];
				if let Some(rest) = line.strip_prefix(device) {
					let name = rest.trim();
					// "GA107M [GeForce RTX 3050 Ti Mobile]": the bracketed part is the product.
					let product = name
						.rfind('[')
						.and_then(|i| name[i + 1..].strip_suffix(']'))
						.unwrap_or(name);
					return Some(product.to_string());
				}
			}
		}
		None
	}
}
