//! D-Bus: logind for power, systemd for units, power-profiles-daemon, MPRIS players.
//!
//! The service is a user service outside any login session, so polkit treats it as
//! "inactive": power actions, profile switching and system units come back as
//! "challenge" unless a polkit rule allows them (the NixOS module can add one).

use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::OnceCell;
use zbus::proxy::{Builder, CacheProperties};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::{Connection, Proxy};

/// Longest a single D-Bus round trip may take before the service gives up on it.
const CALL_TIMEOUT: Duration = Duration::from_secs(3);

pub struct Bus {
	system: OnceCell<Option<Connection>>,
	session: OnceCell<Option<Connection>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scope {
	User,
	System,
}

impl Scope {
	pub fn parse(s: Option<&str>) -> Scope {
		if s == Some("system") { Scope::System } else { Scope::User }
	}
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct PowerCaps {
	/// logind's answers: "yes", "challenge" (needs a password), "no" or "na".
	pub poweroff: String,
	pub reboot: String,
	pub suspend: String,
	pub lock: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Profiles {
	pub active: String,
	pub available: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct UnitInfo {
	pub name: String,
	pub description: String,
	pub active: String,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Player {
	/// The bus name, which also identifies the player in requests.
	pub id: String,
	pub name: String,
	/// "Playing", "Paused" or "Stopped".
	pub status: String,
	pub title: Option<String>,
	pub artist: Option<String>,
	pub album: Option<String>,
	/// Microseconds.
	pub length: Option<i64>,
	pub position: Option<i64>,
	pub volume: Option<f64>,
	pub has_art: bool,
	pub can_next: bool,
	pub can_previous: bool,
	pub can_play: bool,
	pub can_pause: bool,
	pub can_seek: bool,
	#[serde(skip)]
	pub art_url: Option<String>,
	#[serde(skip)]
	pub track: Option<OwnedObjectPath>,
}

fn err(e: impl std::fmt::Display) -> String {
	let s = e.to_string();
	// polkit's refusal reads like an internal error otherwise.
	if s.contains("Interactive authentication required") || s.contains("AccessDenied") || s.contains("not authorized") {
		"not_permitted".into()
	} else {
		s
	}
}

impl Bus {
	pub fn new() -> Self {
		Bus { system: OnceCell::new(), session: OnceCell::new() }
	}

	async fn system(&self) -> Result<&Connection, String> {
		self.system
			.get_or_init(|| async { Connection::system().await.map_err(|e| eprintln!("system bus: {e}")).ok() })
			.await
			.as_ref()
			.ok_or_else(|| "no_system_bus".to_string())
	}

	async fn session(&self) -> Result<&Connection, String> {
		self.session
			.get_or_init(|| async { Connection::session().await.map_err(|e| eprintln!("session bus: {e}")).ok() })
			.await
			.as_ref()
			.ok_or_else(|| "no_session_bus".to_string())
	}

	async fn conn(&self, scope: Scope) -> Result<&Connection, String> {
		match scope {
			Scope::User => self.session().await,
			Scope::System => self.system().await,
		}
	}

	async fn proxy<'a>(conn: &Connection, dest: &'a str, path: &'a str, iface: &'a str) -> Result<Proxy<'a>, String> {
		Builder::new(conn)
			.destination(dest)
			.map_err(err)?
			.path(path)
			.map_err(err)?
			.interface(iface)
			.map_err(err)?
			.cache_properties(CacheProperties::No)
			.build()
			.await
			.map_err(err)
	}

	async fn logind(&self) -> Result<Proxy<'static>, String> {
		let c = self.system().await?;
		Self::proxy(c, "org.freedesktop.login1", "/org/freedesktop/login1", "org.freedesktop.login1.Manager").await
	}

	// ---- power ------------------------------------------------------------

	pub async fn power_caps(&self) -> PowerCaps {
		let run = async {
			let p = self.logind().await?;
			let ask = |m: &'static str| {
				let p = p.clone();
				async move { p.call::<_, _, String>(m, &()).await.unwrap_or_else(|_| "na".into()) }
			};
			Ok::<_, String>(PowerCaps {
				poweroff: ask("CanPowerOff").await,
				reboot: ask("CanReboot").await,
				suspend: ask("CanSuspend").await,
				lock: !self.own_sessions().await.unwrap_or_default().is_empty(),
			})
		};
		tokio::time::timeout(CALL_TIMEOUT, run).await.ok().and_then(Result::ok).unwrap_or_default()
	}

	pub async fn power(&self, action: &str) -> Result<(), String> {
		let method = match action {
			"poweroff" => "PowerOff",
			"reboot" => "Reboot",
			"suspend" => "Suspend",
			"lock" => return self.lock().await,
			_ => return Err("unknown_action".into()),
		};
		let p = self.logind().await?;
		// false: don't ask for a password interactively; there's nobody to ask.
		p.call::<_, _, ()>(method, &(false,)).await.map_err(err)
	}

	/// The paths of this user's login sessions.
	async fn own_sessions(&self) -> Result<Vec<OwnedObjectPath>, String> {
		let uid = std::fs::metadata("/proc/self").map(|m| m.uid()).map_err(err)?;
		let p = self.logind().await?;
		let sessions: Vec<(String, u32, String, String, OwnedObjectPath)> =
			p.call("ListSessions", &()).await.map_err(err)?;
		Ok(sessions.into_iter().filter(|s| s.1 == uid).map(|s| s.4).collect())
	}

	/// Locks every session of this user; the desktop's locker reacts to logind's Lock signal.
	async fn lock(&self) -> Result<(), String> {
		let c = self.system().await?;
		let sessions = self.own_sessions().await?;
		if sessions.is_empty() {
			return Err("no_session".into());
		}
		for path in sessions {
			let s = Self::proxy(c, "org.freedesktop.login1", path.as_str(), "org.freedesktop.login1.Session").await?;
			s.call::<_, _, ()>("Lock", &()).await.map_err(err)?;
		}
		Ok(())
	}

	// ---- power profiles -------------------------------------------------------

	async fn ppd(&self) -> Result<Proxy<'static>, String> {
		let c = self.system().await?;
		// The daemon took a freedesktop name in 0.20 and keeps the old one as an alias.
		for (dest, path) in [
			("org.freedesktop.UPower.PowerProfiles", "/org/freedesktop/UPower/PowerProfiles"),
			("net.hadess.PowerProfiles", "/net/hadess/PowerProfiles"),
		] {
			if let Ok(p) = Self::proxy(c, dest, path, dest).await
				&& p.get_property::<String>("ActiveProfile").await.is_ok()
			{
				return Ok(p);
			}
		}
		Err("no_power_profiles".into())
	}

	pub async fn profiles(&self) -> Option<Profiles> {
		let run = async {
			let p = self.ppd().await.ok()?;
			let active: String = p.get_property("ActiveProfile").await.ok()?;
			let list: Vec<HashMap<String, OwnedValue>> = p.get_property("Profiles").await.ok()?;
			let available = list
				.iter()
				.filter_map(|m| m.get("Profile").and_then(|v| String::try_from(v.clone()).ok()))
				.collect();
			Some(Profiles { active, available })
		};
		tokio::time::timeout(CALL_TIMEOUT, run).await.ok().flatten()
	}

	pub async fn set_profile(&self, profile: &str) -> Result<(), String> {
		let p = self.ppd().await?;
		p.set_property("ActiveProfile", profile).await.map_err(err)
	}

	// ---- systemd units ---------------------------------------------------------

	async fn systemd(&self, scope: Scope) -> Result<Proxy<'static>, String> {
		let c = self.conn(scope).await?;
		Self::proxy(c, "org.freedesktop.systemd1", "/org/freedesktop/systemd1", "org.freedesktop.systemd1.Manager").await
	}

	/// "active", "inactive", "failed", "activating"… or "missing" when no such unit exists.
	pub async fn unit_state(&self, scope: Scope, name: &str) -> Option<String> {
		let run = async {
			let m = self.systemd(scope).await.ok()?;
			let path: OwnedObjectPath = m.call("LoadUnit", &(name,)).await.ok()?;
			let c = self.conn(scope).await.ok()?;
			let u = Self::proxy(c, "org.freedesktop.systemd1", path.as_str(), "org.freedesktop.systemd1.Unit").await.ok()?;
			let load: String = u.get_property("LoadState").await.ok()?;
			if load == "not-found" {
				return Some("missing".to_string());
			}
			u.get_property::<String>("ActiveState").await.ok()
		};
		tokio::time::timeout(CALL_TIMEOUT, run).await.ok().flatten()
	}

	pub async fn unit_op(&self, scope: Scope, name: &str, op: &str) -> Result<(), String> {
		let method = match op {
			"start" => "StartUnit",
			"stop" => "StopUnit",
			"restart" => "RestartUnit",
			_ => return Err("unknown_operation".into()),
		};
		let m = self.systemd(scope).await?;
		m.call::<_, _, OwnedObjectPath>(method, &(name, "replace")).await.map(|_| ()).map_err(err)
	}

	/// Loaded service units, for picking one when adding a service.
	pub async fn units(&self, scope: Scope) -> Result<Vec<UnitInfo>, String> {
		type Row = (String, String, String, String, String, String, OwnedObjectPath, u32, String, OwnedObjectPath);
		let m = self.systemd(scope).await?;
		let rows: Vec<Row> = m.call("ListUnits", &()).await.map_err(err)?;
		let mut v: Vec<UnitInfo> = rows
			.into_iter()
			.filter(|r| r.0.ends_with(".service") && r.2 != "not-found")
			// Desktop autostart entries, D-Bus activations and transient runs aren't what people host.
			.filter(|r| !["app-", "dbus-", "run-", "user@", "user-runtime-dir@"].iter().any(|p| r.0.starts_with(p)))
			.map(|r| UnitInfo { name: r.0, description: r.1, active: r.3 })
			.collect();
		v.sort_by(|a, b| a.name.cmp(&b.name));
		Ok(v)
	}

	// ---- media players -------------------------------------------------------------

	pub async fn players(&self) -> Vec<Player> {
		let run = async {
			let c = self.session().await.ok()?;
			let dbus = Self::proxy(c, "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus").await.ok()?;
			let names: Vec<String> = dbus.call("ListNames", &()).await.ok()?;
			let mut out = Vec::new();
			for n in names.into_iter().filter(|n| n.starts_with("org.mpris.MediaPlayer2.")) {
				if let Some(p) = Self::player(c, &n).await {
					out.push(p);
				}
			}
			// Playing first, then paused, then the rest.
			out.sort_by_key(|p| match p.status.as_str() {
				"Playing" => 0,
				"Paused" => 1,
				_ => 2,
			});
			Some(out)
		};
		tokio::time::timeout(CALL_TIMEOUT, run).await.ok().flatten().unwrap_or_default()
	}

	async fn player(c: &Connection, bus_name: &str) -> Option<Player> {
		let root = Self::proxy(c, bus_name, "/org/mpris/MediaPlayer2", "org.mpris.MediaPlayer2").await.ok()?;
		let p = Self::proxy(c, bus_name, "/org/mpris/MediaPlayer2", "org.mpris.MediaPlayer2.Player").await.ok()?;
		let name: String = root
			.get_property("Identity")
			.await
			.unwrap_or_else(|_| bus_name.trim_start_matches("org.mpris.MediaPlayer2.").to_string());
		let meta: HashMap<String, OwnedValue> = p.get_property("Metadata").await.unwrap_or_default();
		let text = |k: &str| meta.get(k).and_then(|v| String::try_from(v.clone()).ok()).filter(|s| !s.is_empty());
		let artist = meta
			.get("xesam:artist")
			.and_then(|v| <Vec<String>>::try_from(v.clone()).ok())
			.map(|a| a.join(", "))
			.filter(|s| !s.is_empty())
			.or_else(|| text("xesam:artist"));
		let length = meta.get("mpris:length").and_then(|v| {
			i64::try_from(v.clone()).ok().or_else(|| u64::try_from(v.clone()).ok().map(|u| u as i64))
		});
		let art_url = text("mpris:artUrl");
		let flag = |b: Result<bool, zbus::Error>| b.unwrap_or(false);
		Some(Player {
			id: bus_name.to_string(),
			name,
			status: p.get_property("PlaybackStatus").await.unwrap_or_else(|_| "Stopped".into()),
			title: text("xesam:title"),
			artist,
			album: text("xesam:album"),
			length: length.filter(|l| *l > 0),
			position: p.get_property::<i64>("Position").await.ok(),
			volume: p.get_property::<f64>("Volume").await.ok(),
			has_art: art_url.as_deref().is_some_and(|u| u.starts_with("file://")),
			can_next: flag(p.get_property("CanGoNext").await),
			can_previous: flag(p.get_property("CanGoPrevious").await),
			can_play: flag(p.get_property("CanPlay").await),
			can_pause: flag(p.get_property("CanPause").await),
			can_seek: flag(p.get_property("CanSeek").await),
			art_url,
			track: meta.get("mpris:trackid").and_then(|v| OwnedObjectPath::try_from(v.clone()).ok()),
		})
	}

	pub async fn player_op(&self, id: &str, op: &str, value: Option<f64>) -> Result<(), String> {
		if !id.starts_with("org.mpris.MediaPlayer2.") {
			return Err("unknown_player".into());
		}
		let c = self.session().await?;
		let p = Self::proxy(c, id, "/org/mpris/MediaPlayer2", "org.mpris.MediaPlayer2.Player").await?;
		match op {
			"play_pause" => p.call::<_, _, ()>("PlayPause", &()).await.map_err(err),
			"next" => p.call::<_, _, ()>("Next", &()).await.map_err(err),
			"previous" => p.call::<_, _, ()>("Previous", &()).await.map_err(err),
			"volume" => {
				let v = value.ok_or("no_value")?.clamp(0.0, 1.0);
				p.set_property("Volume", v).await.map_err(err)
			}
			"seek" => {
				// SetPosition wants the current track's id; seconds come in, microseconds go out.
				let target = (value.ok_or("no_value")?.max(0.0) * 1e6) as i64;
				let player = Self::player(c, id).await.ok_or("unknown_player")?;
				let track = player.track.ok_or("no_track")?;
				p.call::<_, _, ()>("SetPosition", &(track, target)).await.map_err(err)
			}
			_ => Err("unknown_operation".into()),
		}
	}

	/// The cover of what a player plays, when it's a local file.
	pub async fn player_art(&self, id: &str) -> Option<(Vec<u8>, &'static str)> {
		let c = self.session().await.ok()?;
		let player = Self::player(c, id).await?;
		let url = player.art_url?;
		let path = url.strip_prefix("file://")?;
		let path = percent_decode(path);
		let meta = tokio::fs::metadata(&path).await.ok()?;
		if !meta.is_file() || meta.len() > 8 * 1024 * 1024 {
			return None;
		}
		let bytes = tokio::fs::read(&path).await.ok()?;
		let kind = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
			"image/png"
		} else if bytes.starts_with(&[0xff, 0xd8]) {
			"image/jpeg"
		} else if bytes.len() > 12 && &bytes[8..12] == b"WEBP" {
			"image/webp"
		} else {
			return None;
		};
		Some((bytes, kind))
	}
}

fn percent_decode(s: &str) -> String {
	let b = s.as_bytes();
	let mut out = Vec::with_capacity(b.len());
	let mut i = 0;
	while i < b.len() {
		if b[i] == b'%'
			&& i + 2 < b.len()
			&& let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok())
		{
			out.push(v);
			i += 3;
			continue;
		}
		out.push(b[i]);
		i += 1;
	}
	String::from_utf8_lossy(&out).into_owned()
}
