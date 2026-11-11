//! D-Bus: logind for power, systemd for units, power-profiles-daemon, MPRIS players.
//!
//! elamond runs as a user service outside any login session, so polkit treats it as
//! "inactive": power actions, profile switching and system units come back as
//! "challenge" unless a polkit rule allows them (the NixOS module can add one).

use std::collections::HashMap;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde::Serialize;
use zbus::proxy::{Builder, CacheProperties};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::{Connection, Proxy};

use crate::consts::{limit, protocol, timeout};
use crate::errors::{reason, ErrorCode};

/// The two buses, each connected on first use and again after its connection broke.
pub struct Bus {
	system: Link,
	session: Link,
}

struct Link {
	name: &'static str,
	/// What a call answers while there is no connection.
	unavailable: &'static str,
	state: Mutex<LinkState>,
}

#[derive(Default)]
struct LinkState {
	conn: Option<Connection>,
	/// After a failed attempt, no new one before this; the wait doubles each time.
	retry_at: Option<Instant>,
	wait: Duration,
}

impl Link {
	fn new(name: &'static str, unavailable: &'static str) -> Self {
		Link { name, unavailable, state: Mutex::new(LinkState::default()) }
	}

	fn state(&self) -> std::sync::MutexGuard<'_, LinkState> {
		self.state.lock().unwrap_or_else(PoisonError::into_inner)
	}
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
	User,
	System,
}

impl Scope {
	pub fn parse(s: Option<&str>) -> Scope {
		if s == Some("system") { Scope::System } else { Scope::User }
	}
}

#[derive(Serialize, Clone, Debug)]
pub struct PowerCaps {
	/// logind's answers: "yes", "challenge" (needs a password), "no" or "na".
	pub poweroff: String,
	pub reboot: String,
	pub suspend: String,
	pub lock: bool,
}

/// What the app sees when logind can't be asked (control off, or no answer).
impl Default for PowerCaps {
	fn default() -> Self {
		PowerCaps { poweroff: "na".into(), reboot: "na".into(), suspend: "na".into(), lock: false }
	}
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
		ErrorCode::NotPermitted.as_str().into()
	} else {
		s
	}
}

/// A player's bus name, checked before it goes anywhere near the bus or the log.
pub fn is_player(id: &str) -> bool {
	id.strip_prefix(protocol::MPRIS_PREFIX)
		.is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)))
}

impl Bus {
	pub fn new() -> Self {
		Bus { system: Link::new("system", reason::NO_SYSTEM_BUS), session: Link::new("session", reason::NO_SESSION_BUS) }
	}

	fn link(&self, scope: Scope) -> &Link {
		match scope {
			Scope::User => &self.session,
			Scope::System => &self.system,
		}
	}

	/// The scope's connection, made now if there is none. Every method call on it gives up
	/// after `timeout::CALL`.
	async fn conn(&self, scope: Scope) -> Result<Connection, String> {
		let link = self.link(scope);
		{
			let s = link.state();
			if let Some(c) = &s.conn {
				return Ok(c.clone());
			}
			if s.retry_at.is_some_and(|t| Instant::now() < t) {
				return Err(link.unavailable.into());
			}
		}
		let made = tokio::time::timeout(timeout::CALL, async {
			let builder = match scope {
				Scope::User => zbus::connection::Builder::session()?,
				Scope::System => zbus::connection::Builder::system()?,
			};
			builder.method_timeout(timeout::CALL).build().await
		})
		.await;
		let mut s = link.state();
		match made {
			Ok(Ok(c)) => {
				if !s.wait.is_zero() {
					eprintln!("{} bus: connected", link.name);
				}
				*s = LinkState { conn: Some(c.clone()), ..Default::default() };
				Ok(c)
			}
			failed => {
				let why = match failed {
					Ok(Err(e)) => e.to_string(),
					_ => reason::TIMED_OUT.into(),
				};
				s.wait = (s.wait * 2).clamp(timeout::BUS_RETRY_MIN, timeout::BUS_RETRY_MAX);
				s.retry_at = Some(Instant::now() + s.wait);
				eprintln!("{} bus: {why}; trying again in {} s", link.name, s.wait.as_secs());
				Err(link.unavailable.into())
			}
		}
	}

	/// Turns a failed call into the text the app gets. A connection that broke (the bus
	/// restarted, say) is dropped, so the next call makes a new one.
	fn failed(&self, scope: Scope, e: zbus::Error) -> String {
		if let zbus::Error::InputOutput(io) = &e
			&& io.kind() != std::io::ErrorKind::TimedOut
		{
			let link = self.link(scope);
			if link.state().conn.take().is_some() {
				eprintln!("{} bus: {e}; connecting again", link.name);
			}
		}
		err(e)
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
		let c = self.conn(Scope::System).await?;
		Self::proxy(&c, "org.freedesktop.login1", "/org/freedesktop/login1", "org.freedesktop.login1.Manager").await
	}

	// ---- power ------------------------------------------------------------

	pub async fn power_caps(&self) -> PowerCaps {
		let run = async {
			let p = self.logind().await?;
			let ask = |m: &'static str| {
				let p = p.clone();
				async move {
					match p.call::<_, _, String>(m, &()).await {
						Ok(answer) => answer,
						Err(e) => {
							self.failed(Scope::System, e);
							"na".into()
						}
					}
				}
			};
			Ok::<_, String>(PowerCaps {
				poweroff: ask("CanPowerOff").await,
				reboot: ask("CanReboot").await,
				suspend: ask("CanSuspend").await,
				lock: !self.own_sessions().await.unwrap_or_default().is_empty(),
			})
		};
		tokio::time::timeout(timeout::CALL, run).await.ok().and_then(Result::ok).unwrap_or_default()
	}

	/// The logind method behind each power action the app may ask for.
	pub fn power_method(action: &str) -> Option<&'static str> {
		Some(match action {
			"poweroff" => "PowerOff",
			"reboot" => "Reboot",
			"suspend" => "Suspend",
			"lock" => "Lock",
			_ => return None,
		})
	}

	pub async fn power(&self, action: &str) -> Result<(), String> {
		let method = Self::power_method(action).ok_or(reason::UNKNOWN_ACTION)?;
		if method == "Lock" {
			return self.lock().await;
		}
		let p = self.logind().await?;
		// false: don't ask for a password interactively; there's nobody to ask.
		p.call::<_, _, ()>(method, &(false,)).await.map_err(|e| self.failed(Scope::System, e))
	}

	/// The paths of this user's login sessions.
	async fn own_sessions(&self) -> Result<Vec<OwnedObjectPath>, String> {
		let uid = std::fs::metadata("/proc/self").map(|m| m.uid()).map_err(err)?;
		let p = self.logind().await?;
		let sessions: Vec<(String, u32, String, String, OwnedObjectPath)> =
			p.call("ListSessions", &()).await.map_err(|e| self.failed(Scope::System, e))?;
		Ok(sessions.into_iter().filter(|s| s.1 == uid).map(|s| s.4).collect())
	}

	/// Locks every session of this user; the desktop's locker reacts to logind's Lock signal.
	async fn lock(&self) -> Result<(), String> {
		let c = self.conn(Scope::System).await?;
		let sessions = self.own_sessions().await?;
		if sessions.is_empty() {
			return Err(reason::NO_SESSION.into());
		}
		for path in sessions {
			let s = Self::proxy(&c, "org.freedesktop.login1", path.as_str(), "org.freedesktop.login1.Session").await?;
			s.call::<_, _, ()>("Lock", &()).await.map_err(|e| self.failed(Scope::System, e))?;
		}
		Ok(())
	}

	// ---- power profiles -------------------------------------------------------

	async fn ppd(&self) -> Result<Proxy<'static>, String> {
		let c = self.conn(Scope::System).await?;
		// The daemon took a freedesktop name in 0.20 and keeps the old one as an alias.
		for (dest, path) in [
			("org.freedesktop.UPower.PowerProfiles", "/org/freedesktop/UPower/PowerProfiles"),
			("net.hadess.PowerProfiles", "/net/hadess/PowerProfiles"),
		] {
			let Ok(p) = Self::proxy(&c, dest, path, dest).await else { continue };
			match p.get_property::<String>("ActiveProfile").await {
				Ok(_) => return Ok(p),
				Err(e) => {
					self.failed(Scope::System, e);
				}
			}
		}
		Err(reason::NO_POWER_PROFILES.into())
	}

	pub async fn profiles(&self) -> Option<Profiles> {
		let run = async {
			let p = self.ppd().await.ok()?;
			let active: String = p.get_property("ActiveProfile").await.map_err(|e| self.failed(Scope::System, e)).ok()?;
			let list: Vec<HashMap<String, OwnedValue>> =
				p.get_property("Profiles").await.map_err(|e| self.failed(Scope::System, e)).ok()?;
			let available = list
				.iter()
				.filter_map(|m| m.get("Profile").and_then(|v| String::try_from(v.clone()).ok()))
				.collect();
			Some(Profiles { active, available })
		};
		tokio::time::timeout(timeout::CALL, run).await.ok().flatten()
	}

	pub async fn set_profile(&self, profile: &str) -> Result<(), String> {
		let p = self.ppd().await?;
		p.set_property("ActiveProfile", profile).await.map_err(|e| self.failed(Scope::System, e.into()))
	}

	// ---- systemd units ---------------------------------------------------------

	async fn systemd(&self, scope: Scope) -> Result<Proxy<'static>, String> {
		let c = self.conn(scope).await?;
		Self::proxy(&c, "org.freedesktop.systemd1", "/org/freedesktop/systemd1", "org.freedesktop.systemd1.Manager").await
	}

	/// "active", "inactive", "failed", "activating"… or "missing" when no such unit exists.
	pub async fn unit_state(&self, scope: Scope, name: &str) -> Option<String> {
		let run = async {
			let m = self.systemd(scope).await.ok()?;
			let path: OwnedObjectPath = m.call("LoadUnit", &(name,)).await.map_err(|e| self.failed(scope, e)).ok()?;
			let c = self.conn(scope).await.ok()?;
			let u = Self::proxy(&c, "org.freedesktop.systemd1", path.as_str(), "org.freedesktop.systemd1.Unit").await.ok()?;
			let load: String = u.get_property("LoadState").await.map_err(|e| self.failed(scope, e)).ok()?;
			if load == "not-found" {
				return Some("missing".to_string());
			}
			u.get_property::<String>("ActiveState").await.map_err(|e| self.failed(scope, e)).ok()
		};
		tokio::time::timeout(timeout::CALL, run).await.ok().flatten()
	}

	/// The systemd method behind each operation the app may ask for on a unit.
	pub fn unit_method(op: &str) -> Option<&'static str> {
		Some(match op {
			"start" => "StartUnit",
			"stop" => "StopUnit",
			"restart" => "RestartUnit",
			_ => return None,
		})
	}

	pub async fn unit_op(&self, scope: Scope, name: &str, op: &str) -> Result<(), String> {
		let method = Self::unit_method(op).ok_or(reason::UNKNOWN_OPERATION)?;
		let m = self.systemd(scope).await?;
		m.call::<_, _, OwnedObjectPath>(method, &(name, "replace")).await.map(|_| ()).map_err(|e| self.failed(scope, e))
	}

	/// Loaded service units, for picking one when adding a link.
	pub async fn units(&self, scope: Scope) -> Result<Vec<UnitInfo>, String> {
		type Row = (String, String, String, String, String, String, OwnedObjectPath, u32, String, OwnedObjectPath);
		let m = self.systemd(scope).await?;
		let rows: Vec<Row> = m.call("ListUnits", &()).await.map_err(|e| self.failed(scope, e))?;
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
			let c = self.conn(Scope::User).await.ok()?;
			let dbus = Self::proxy(&c, "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus").await.ok()?;
			let names: Vec<String> = dbus.call("ListNames", &()).await.map_err(|e| self.failed(Scope::User, e)).ok()?;
			let mut out = Vec::new();
			for n in names.into_iter().filter(|n| is_player(n)) {
				if let Some(p) = Self::player(&c, &n).await {
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
		tokio::time::timeout(timeout::CALL, run).await.ok().flatten().unwrap_or_default()
	}

	async fn player(c: &Connection, bus_name: &str) -> Option<Player> {
		let root = Self::proxy(c, bus_name, "/org/mpris/MediaPlayer2", "org.mpris.MediaPlayer2").await.ok()?;
		let p = Self::proxy(c, bus_name, "/org/mpris/MediaPlayer2", "org.mpris.MediaPlayer2.Player").await.ok()?;
		let name: String = root
			.get_property("Identity")
			.await
			.unwrap_or_else(|_| bus_name.trim_start_matches(protocol::MPRIS_PREFIX).to_string());
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
			has_art: art_url.as_deref().and_then(art_path).is_some(),
			can_next: flag(p.get_property("CanGoNext").await),
			can_previous: flag(p.get_property("CanGoPrevious").await),
			can_play: flag(p.get_property("CanPlay").await),
			can_pause: flag(p.get_property("CanPause").await),
			can_seek: flag(p.get_property("CanSeek").await),
			art_url,
			track: meta.get("mpris:trackid").and_then(|v| OwnedObjectPath::try_from(v.clone()).ok()),
		})
	}

	/// The operations the app may ask a player for.
	pub fn is_player_op(op: &str) -> bool {
		matches!(op, "play_pause" | "next" | "previous" | "volume" | "seek")
	}

	pub async fn player_op(&self, id: &str, op: &str, value: Option<f64>) -> Result<(), String> {
		if !is_player(id) {
			return Err(reason::UNKNOWN_PLAYER.into());
		}
		let c = self.conn(Scope::User).await?;
		let p = Self::proxy(&c, id, "/org/mpris/MediaPlayer2", "org.mpris.MediaPlayer2.Player").await?;
		let failed = |e: zbus::Error| self.failed(Scope::User, e);
		match op {
			"play_pause" => p.call::<_, _, ()>("PlayPause", &()).await.map_err(failed),
			"next" => p.call::<_, _, ()>("Next", &()).await.map_err(failed),
			"previous" => p.call::<_, _, ()>("Previous", &()).await.map_err(failed),
			"volume" => {
				let v = value.ok_or(reason::NO_VALUE)?.clamp(0.0, 1.0);
				p.set_property("Volume", v).await.map_err(|e| failed(e.into()))
			}
			"seek" => {
				// SetPosition wants the current track's id; seconds come in, microseconds go out.
				let target = (value.ok_or(reason::NO_VALUE)?.max(0.0) * 1e6) as i64;
				let player = Self::player(&c, id).await.ok_or(reason::UNKNOWN_PLAYER)?;
				let track = player.track.ok_or(reason::NO_TRACK)?;
				p.call::<_, _, ()>("SetPosition", &(track, target)).await.map_err(failed)
			}
			_ => Err(reason::UNKNOWN_OPERATION.into()),
		}
	}

	/// The cover of what a player plays, when it's a local file.
	pub async fn player_art(&self, id: &str) -> Option<(Vec<u8>, &'static str)> {
		if !is_player(id) {
			return None;
		}
		let c = self.conn(Scope::User).await.ok()?;
		let player = tokio::time::timeout(timeout::CALL, Self::player(&c, id)).await.ok()??;
		read_image(&art_path(player.art_url.as_deref()?)?).await
	}
}

/// The file a `file://` URL names: `file:///a/b` or `file://localhost/a/b`, other hosts not.
/// The path is percent-decoded as bytes, since file names needn't be UTF-8.
fn art_path(url: &str) -> Option<PathBuf> {
	let rest = url.strip_prefix("file://")?;
	let path = rest.strip_prefix("localhost").unwrap_or(rest);
	if !path.starts_with('/') {
		return None;
	}
	Some(PathBuf::from(OsString::from_vec(percent_decode(path))))
}

/// A cover image, if the path is a regular file no larger than `limit::ART_MAX`.
async fn read_image(path: &Path) -> Option<(Vec<u8>, &'static str)> {
	use tokio::io::AsyncReadExt;
	// Only a regular file is opened at all: opening a device can do something by itself.
	if !tokio::fs::metadata(path).await.ok()?.is_file() {
		return None;
	}
	// Non-blocking, so a FIFO put in its place since can't hang the open; it is then refused.
	let file = tokio::fs::OpenOptions::new()
		.read(true)
		.custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
		.open(path)
		.await
		.ok()?;
	if !file.metadata().await.ok()?.is_file() {
		return None;
	}
	let mut bytes = Vec::new();
	file.take(limit::ART_MAX as u64 + 1).read_to_end(&mut bytes).await.ok()?;
	if bytes.len() > limit::ART_MAX {
		return None;
	}
	let kind = image_kind(&bytes)?;
	Some((bytes, kind))
}

fn image_kind(bytes: &[u8]) -> Option<&'static str> {
	if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
		Some("image/png")
	} else if bytes.starts_with(&[0xff, 0xd8]) {
		Some("image/jpeg")
	} else if bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
		Some("image/webp")
	} else {
		None
	}
}

fn percent_decode(s: &str) -> Vec<u8> {
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
	out
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::os::unix::ffi::OsStrExt;

	#[test]
	fn scopes() {
		assert_eq!(Scope::parse(Some("system")), Scope::System);
		assert_eq!(Scope::parse(Some("user")), Scope::User);
		assert_eq!(Scope::parse(None), Scope::User);
		assert_eq!(Scope::parse(Some("System")), Scope::User);
	}

	#[test]
	fn percent_decoding_keeps_bytes() {
		assert_eq!(percent_decode("/a%20b/%E2%9C%93"), "/a b/✓".as_bytes());
		assert_eq!(percent_decode("%FF%fe"), [0xff, 0xfe]);
		// Not escapes: left as they are.
		assert_eq!(percent_decode("100%"), b"100%");
		assert_eq!(percent_decode("%zz%4"), b"%zz%4");
	}

	#[test]
	fn art_urls() {
		let path = |u: &str| art_path(u).map(|p| p.as_os_str().as_bytes().to_vec());
		assert_eq!(path("file:///home/a/b%20c.jpg").unwrap(), b"/home/a/b c.jpg");
		assert_eq!(path("file://localhost/tmp/x.png").unwrap(), b"/tmp/x.png");
		assert_eq!(path("file:///m/%FF.jpg").unwrap(), b"/m/\xff.jpg");
		assert_eq!(path("file://otherhost/x.png"), None);
		assert_eq!(path("file://localhostx/x.png"), None);
		assert_eq!(path("file://relative.png"), None);
		assert_eq!(path("https://example.org/x.png"), None);
	}

	#[test]
	fn image_kinds() {
		assert_eq!(image_kind(b"\x89PNG\r\n\x1a\n"), Some("image/png"));
		assert_eq!(image_kind(b"\xff\xd8\xff\xe0"), Some("image/jpeg"));
		assert_eq!(image_kind(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
		assert_eq!(image_kind(b"GIF89a"), None);
	}

	#[test]
	fn player_ids() {
		assert!(is_player("org.mpris.MediaPlayer2.spotify"));
		assert!(is_player("org.mpris.MediaPlayer2.firefox.instance_1_42"));
		assert!(!is_player("org.mpris.MediaPlayer2."));
		assert!(!is_player("org.freedesktop.login1"));
		assert!(!is_player("org.mpris.MediaPlayer2.x\ny"));
	}

	#[tokio::test]
	async fn only_regular_files_are_read_as_art() {
		let dir = std::env::temp_dir().join(format!("elamond-art-{}", std::process::id()));
		std::fs::create_dir_all(&dir).unwrap();
		let png = dir.join("cover.png");
		std::fs::write(&png, b"\x89PNG\r\n\x1a\nrest").unwrap();
		assert_eq!(read_image(&png).await.map(|(_, k)| k), Some("image/png"));
		assert!(read_image(&dir).await.is_none());
		assert!(read_image(Path::new("/dev/zero")).await.is_none());
		let fifo = dir.join("fifo");
		let c = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
		// SAFETY: mkfifo with a valid C string.
		assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
		assert!(read_image(&fifo).await.is_none());
		let big = dir.join("big.png");
		let mut data = b"\x89PNG".to_vec();
		data.resize(limit::ART_MAX + 1, 0);
		std::fs::write(&big, data).unwrap();
		assert!(read_image(&big).await.is_none());
		std::fs::remove_dir_all(dir).unwrap();
	}
}
