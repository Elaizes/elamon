//! Monitors, read from the kernel (DRM) so it works the same under any compositor or none.
//!
//! sysfs gives connection, DPMS and EDID for every connector without touching the GPU.
//! The current mode and position need a DRM query on the card node; that is only done for
//! GPUs that are awake, and the node is closed right after, because the first opener of a
//! card with no compositor running becomes its DRM master.

use std::fs::{File, OpenOptions};
use std::os::fd::{AsFd, BorrowedFd};
use std::path::Path;

use drm::control::Device as ControlDevice;
use drm::Device;

use super::gpu::Card;
use super::sysfs::{dir_entries, read};
use crate::model::{Display, Mode};

struct Node(File);

impl AsFd for Node {
	fn as_fd(&self) -> BorrowedFd<'_> {
		self.0.as_fd()
	}
}
impl Device for Node {}
impl ControlDevice for Node {}

/// What the card node says about one connector.
struct Live {
	name: String,
	mode: Option<Mode>,
	position: Option<[u32; 2]>,
	max_refresh: Option<f32>,
}

pub fn displays(cards: &[Card]) -> Vec<Display> {
	let mut out = Vec::new();
	for card in cards {
		let Some(card_name) = card.drm.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
		let live = if card.sleeping() { Vec::new() } else { query_card(&card_name) };
		for (entry, dir) in dir_entries("/sys/class/drm") {
			let Some(conn) = entry.strip_prefix(&format!("{card_name}-")) else { continue };
			// Writeback connectors are for screen capture, not monitors.
			if conn.starts_with("Writeback") {
				continue;
			}
			let connected = read(dir.join("status")).as_deref() == Some("connected");
			let dpms = read(dir.join("dpms"));
			let edid = std::fs::read(dir.join("edid")).ok().and_then(|b| parse_edid(&b));
			let l = live.iter().find(|l| l.name == conn);
			let mode = l.and_then(|l| l.mode.clone()).filter(|_| connected);
			let active = mode.is_some() && dpms.as_deref() != Some("Off");
			let internal = conn.starts_with("eDP") || conn.starts_with("LVDS") || conn.starts_with("DSI");
			out.push(Display {
				connector: conn.to_string(),
				gpu: card.info.id.clone(),
				connected,
				active,
				dpms,
				internal,
				vendor: edid.as_ref().and_then(|e| e.vendor.clone()),
				model: edid.as_ref().and_then(|e| e.model.clone()),
				year: edid.as_ref().and_then(|e| e.year),
				width_mm: edid.as_ref().and_then(|e| e.size_mm).map(|s| s.0),
				height_mm: edid.as_ref().and_then(|e| e.size_mm).map(|s| s.1),
				position: l.and_then(|l| l.position).filter(|_| mode.is_some()),
				mode,
				preferred: edid.as_ref().and_then(|e| e.preferred.clone()),
				max_refresh: l.and_then(|l| l.max_refresh).or(edid.as_ref().and_then(|e| e.max_refresh)),
			});
		}
	}
	out.sort_by_key(|d| (!d.connected, !d.internal));
	out
}

fn query_card(card_name: &str) -> Vec<Live> {
	let path = Path::new("/dev/dri").join(card_name);
	let Ok(file) = OpenOptions::new().read(true).write(true).open(&path) else { return Vec::new() };
	let node = Node(file);
	let Ok(res) = node.resource_handles() else { return Vec::new() };
	let mut out = Vec::new();
	for &handle in res.connectors() {
		// No forced probe: that would re-read EDIDs and can blink some monitors.
		let Ok(info) = node.get_connector(handle, false) else { continue };
		let name = format!("{}-{}", info.interface().as_str(), info.interface_id());
		let crtc = info
			.current_encoder()
			.and_then(|e| node.get_encoder(e).ok())
			.and_then(|e| e.crtc())
			.and_then(|c| node.get_crtc(c).ok());
		let mode = crtc.as_ref().and_then(|c| c.mode()).map(|m| to_mode(&m));
		let max_refresh = info
			.modes()
			.iter()
			.map(|m| to_mode(m).refresh)
			.fold(None, |a: Option<f32>, r| Some(a.map_or(r, |a| a.max(r))));
		out.push(Live {
			name,
			mode,
			position: crtc.map(|c| [c.position().0, c.position().1]),
			max_refresh,
		});
	}
	out
}

fn to_mode(m: &drm::control::Mode) -> Mode {
	let (w, h) = m.size();
	let htotal = m.hsync().2 as f64;
	let vtotal = m.vsync().2 as f64;
	let refresh = if htotal > 0.0 && vtotal > 0.0 {
		m.clock() as f64 * 1000.0 / (htotal * vtotal)
	} else {
		m.vrefresh() as f64
	};
	Mode { width: w as u32, height: h as u32, refresh: round_refresh(refresh) }
}

/// 143.981 Hz reads as 144, 59.94 stays 59.94.
fn round_refresh(r: f64) -> f32 {
	let whole = r.round();
	if (r - whole).abs() < 0.03 { whole as f32 } else { ((r * 100.0).round() / 100.0) as f32 }
}

pub struct Edid {
	pub vendor: Option<String>,
	pub model: Option<String>,
	pub year: Option<u16>,
	pub size_mm: Option<(u32, u32)>,
	pub preferred: Option<Mode>,
	pub max_refresh: Option<f32>,
}

pub fn parse_edid(b: &[u8]) -> Option<Edid> {
	if b.len() < 128 || b[..8] != [0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0] {
		return None;
	}
	let id = u16::from_be_bytes([b[8], b[9]]);
	let letter = |shift: u16| (((id >> shift) & 0x1f) as u8 + b'A' - 1) as char;
	let pnp: String = [letter(10), letter(5), letter(0)].iter().collect();
	let vendor = Some(vendor_name(&pnp).map(String::from).unwrap_or(pnp));
	let year = (b[17] > 0).then(|| 1990 + b[17] as u16);

	let mut model = None;
	let mut panel_part = None;
	let mut range_max = None;
	let mut preferred = None;
	let mut size_mm = None;
	let mut max_refresh: Option<f32> = None;

	for (i, o) in [54usize, 72, 90, 108].into_iter().enumerate() {
		let d = &b[o..o + 18];
		if d[0] == 0 && d[1] == 0 {
			match d[3] {
				0xfc => model = Some(descriptor_text(&d[5..18])).filter(|s| !s.is_empty()),
				// Laptop panels rarely have a name, but carry their part number as plain text.
				0xfe if model.is_none() => panel_part = Some(descriptor_text(&d[5..18])).filter(|s| !s.is_empty()),
				0xfd => {
					let extra = if d[4] & 0x02 != 0 { 255 } else { 0 };
					range_max = Some((d[6] as u32 + extra) as f32);
				}
				_ => {}
			}
		} else if let Some((mode, mm)) = detailed_timing(d) {
			if i == 0 {
				preferred = Some(mode.clone());
				size_mm = mm;
			}
			max_refresh = Some(max_refresh.map_or(mode.refresh, |m| m.max(mode.refresh)));
		}
	}

	// CTA-861 extension blocks carry more detailed timings, often the high refresh ones.
	for ext in b[128..].as_chunks::<128>().0 {
		if ext[0] != 0x02 {
			continue;
		}
		let start = ext[2] as usize;
		let mut o = start;
		while start >= 4 && o + 18 <= 127 {
			if let Some((mode, _)) = detailed_timing(&ext[o..o + 18]) {
				max_refresh = Some(max_refresh.map_or(mode.refresh, |m| m.max(mode.refresh)));
			} else {
				break;
			}
			o += 18;
		}
	}

	let size_mm = size_mm.or_else(|| (b[21] > 0 && b[22] > 0).then(|| (b[21] as u32 * 10, b[22] as u32 * 10)));
	Some(Edid {
		vendor,
		model: model.or(panel_part),
		year,
		size_mm,
		preferred,
		max_refresh: match (max_refresh, range_max) {
			(Some(a), Some(b)) => Some(a.max(b)),
			(a, b) => a.or(b),
		},
	})
}

fn detailed_timing(d: &[u8]) -> Option<(Mode, Option<(u32, u32)>)> {
	let clock = u16::from_le_bytes([d[0], d[1]]) as f64 * 10_000.0;
	if clock == 0.0 {
		return None;
	}
	let hactive = d[2] as u32 | ((d[4] as u32 & 0xf0) << 4);
	let hblank = d[3] as u32 | ((d[4] as u32 & 0x0f) << 8);
	let vactive = d[5] as u32 | ((d[7] as u32 & 0xf0) << 4);
	let vblank = d[6] as u32 | ((d[7] as u32 & 0x0f) << 8);
	let total = ((hactive + hblank) * (vactive + vblank)) as f64;
	if total == 0.0 || hactive == 0 || vactive == 0 {
		return None;
	}
	let hmm = d[12] as u32 | ((d[14] as u32 & 0xf0) << 4);
	let vmm = d[13] as u32 | ((d[14] as u32 & 0x0f) << 8);
	let mode = Mode { width: hactive, height: vactive, refresh: round_refresh(clock / total) };
	Some((mode, (hmm > 0 && vmm > 0).then_some((hmm, vmm))))
}

fn descriptor_text(bytes: &[u8]) -> String {
	let end = bytes.iter().position(|&c| c == 0x0a).unwrap_or(bytes.len());
	String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}

/// PNP manufacturer ids of common monitor and laptop panel makers.
fn vendor_name(pnp: &str) -> Option<&'static str> {
	Some(match pnp {
		"ACI" | "AUS" => "ASUS",
		"ACR" => "Acer",
		"AOC" => "AOC",
		"AUO" => "AU Optronics",
		"BNQ" => "BenQ",
		"BOE" => "BOE",
		"CMN" | "CMO" => "Chimei Innolux",
		"CSO" => "CSOT",
		"DEL" => "Dell",
		"DWE" => "Daewoo",
		"EIZ" | "ENC" => "EIZO",
		"FUS" => "Fujitsu",
		"GBT" => "Gigabyte",
		"GSM" | "LGD" => "LG",
		"HPN" | "HWP" => "HP",
		"HSD" => "HannStar",
		"HSL" => "Hisense",
		"IVO" => "InfoVision",
		"IQT" => "iiyama",
		"KDB" => "Kyocera",
		"LEN" => "Lenovo",
		"LPL" => "LG Philips",
		"MAX" => "Maxdata",
		"MED" => "Medion",
		"MEI" => "Panasonic",
		"MSI" => "MSI",
		"MST" => "MStar",
		"NEC" => "NEC",
		"NCP" => "Najing CEC",
		"PHL" => "Philips",
		"PIO" => "Pioneer",
		"QDS" => "Quanta Display",
		"SAM" | "SDC" | "SEC" => "Samsung",
		"SHP" => "Sharp",
		"SNY" => "Sony",
		"STA" => "STAR",
		"TCL" => "TCL",
		"TOS" => "Toshiba",
		"TSB" => "Toshiba",
		"VIZ" => "Vizio",
		"VSC" => "ViewSonic",
		"XMI" => "Xiaomi",
		"APP" => "Apple",
		"MZI" => "Digital Vision",
		"GGL" => "Google",
		"RHT" => "Red Hat (virtual)",
		_ => return None,
	})
}
