//! Monitors under Hyprland (docs/protocol.md, Monitors): what Hyprland says of each output, read
//! by a thread of its own only while a client shows them, and the actions `display.output.<name>`,
//! `display.dpms.<name>` and `display.brightness.<name>`.
//!
//! An action is done one at a time, on what Hyprland says right then (`hyprctl -j monitors all`
//! afresh, under the lock): an output it doesn't show is no action; turning off, or putting to
//! sleep, the last output that shows a picture takes its name typed in; and it is done once
//! Hyprland shows it done, not when `hyprctl` returns. Without `hyprctl`, a running Hyprland or Lua
//! settings there are no such actions, and the monitors are what DRM says, as before.

pub mod hypr;
pub mod light;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

use self::hypr::{Failed, Hyprctl, Output};
use self::light::{Level, Way};
use crate::actions::{Param, ParamKind, Spec};
use crate::consts::cadence;
use crate::errors::{reason, ErrorCode};
use crate::settings::DisplaysDef;

/// What the service sees of the monitors through Hyprland.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Seen {
	/// The Hyprland acted on; none while none on Lua is reached.
	pub instance: Option<String>,
	/// Why there is none: no `hyprctl`, no Hyprland running, its settings not Lua.
	pub why: Option<String>,
	pub outputs: Vec<Output>,
	/// How each output's brightness is reached, by name, and what it was last.
	pub light: BTreeMap<String, (Way, Option<Level>)>,
	/// When Hyprland said it.
	pub at: Option<Instant>,
}

impl Seen {
	pub fn output(&self, name: &str) -> Option<&Output> {
		self.outputs.iter().find(|o| o.name == name)
	}

	/// Whether an output other than `name` with a screen shows a picture.
	pub fn another_lit(&self, name: &str) -> bool {
		self.outputs.iter().any(|o| o.name != name && !o.headless() && o.lit())
	}

	/// Whether turning `o` off, or its picture, takes its name typed in: it is a screen, it is on,
	/// and no other screen shows a picture. An output without a screen leaves nobody in the dark.
	pub fn typed_off(&self, o: &Output) -> bool {
		!o.headless() && !o.disabled && !self.another_lit(&o.name)
	}

	/// The outputs a client is told of and may act on, as `[displays]` says.
	pub fn shown<'a>(&'a self, def: &'a DisplaysDef) -> impl Iterator<Item = &'a Output> {
		self.outputs.iter().filter(move |o| def.headless || !o.headless())
	}

	/// Whether it was read recently enough to be shown as it is now: within two of `[displays]
	/// every` and a moment (the thread reads it every `every` while a client shows the monitors).
	pub fn fresh(&self, def: &DisplaysDef) -> bool {
		self.at.is_some_and(|t| t.elapsed() <= Duration::from_secs(def.every.max(1) * 2) + cadence::DISPLAYS_LINGER)
	}

	/// The brightness of `name` now, in percent, where it can be read.
	pub fn brightness(&self, name: &str) -> Option<f64> {
		self.light.get(name).and_then(|(_, l)| l.map(Level::percent))
	}
}

/// What an action is about: `display.output.DP-1` → (`Output`, `DP-1`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum What {
	Output,
	Dpms,
	Brightness,
}

pub fn target(id: &str) -> Option<(What, &str)> {
	let rest = id.strip_prefix("display.")?;
	let (what, name) = rest.split_once('.')?;
	let what = match what {
		"output" => What::Output,
		"dpms" => What::Dpms,
		"brightness" => What::Brightness,
		_ => return None,
	};
	hypr::fit_name(name).then_some((what, name))
}

fn on_off(typed: Option<&str>) -> Param {
	Param {
		id: "state".into(),
		label: "State".into(),
		kind: ParamKind::Choice(vec![("on".into(), "On".into()), ("off".into(), "Off".into())]),
		optional: false,
		default: None,
		typed: typed.map(|t| BTreeMap::from([("off".to_string(), t.to_string())])).unwrap_or_default(),
	}

}

/// An on-or-off action of a monitor as the settings are checked against it, whatever the monitor.
pub fn on_off_spec(id: &str) -> Spec {
	Spec { id: id.into(), name: id.into(), kind: "display", icon: "monitor".into(), role: crate::access::Role::Operate, confirm: true, unlock: false, params: vec![on_off(None)] }
}

/// The actions on the monitors Hyprland shows, by output in its order: on or off, its picture,
/// and its brightness where that is read. Outputs whose names can't be acted on have none.
pub fn specs(seen: &Seen, def: &DisplaysDef) -> Vec<Spec> {
	if seen.instance.is_none() {
		return Vec::new();
	}
	let mut out = Vec::new();
	for o in seen.shown(def).filter(|o| hypr::fit_name(&o.name)) {
		let typed = seen.typed_off(o).then_some(o.name.as_str());
		let spec = |what: &str, name: String, confirm: bool, param: Param| Spec {
			id: format!("display.{what}.{}", o.name),
			name,
			kind: "display",
			icon: "monitor".into(),
			role: def.role,
			confirm,
			unlock: false,
			params: vec![param],
		};
		out.push(spec("output", o.name.clone(), true, on_off(typed)));
		// A disabled output has no picture to put to sleep.
		if !o.disabled {
			out.push(spec("dpms", format!("{} DPMS", o.name), true, on_off(typed)));
		}
		if let Some((way, Some(_))) = seen.light.get(&o.name) {
			// On the slider's steps from 0, so that 100 is one of them: a panel's least rounded up to one.
			let step = def.brightness_step.max(1);
			let min = if matches!(way, Way::Backlight(_)) { def.panel_min.div_ceil(step) * step } else { 0 }.min(100);
			let param = Param {
				id: "level".into(),
				label: "Brightness".into(),
				kind: ParamKind::Range { min: min.into(), max: 100.into(), step: step.into(), unit: Some("%".into()) },
				optional: false,
				default: None,
				typed: BTreeMap::new(),
			};
			out.push(spec("brightness", format!("{} brightness", o.name), false, param));
		}
	}
	out
}

/// Where the programs are: found on the PATH as they are needed, or given (tests).
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Debug)]
enum Programs {
	Found,
	Given { hyprctl: Option<PathBuf>, ddcutil: Option<PathBuf> },
}

impl Programs {
	fn hyprctl(&self) -> Option<Hyprctl> {
		match self {
			Programs::Found => Hyprctl::find(),
			Programs::Given { hyprctl, .. } => hyprctl.clone().map(|program| Hyprctl { program }),
		}
	}

	fn ddcutil(&self) -> Option<PathBuf> {
		match self {
			Programs::Found => crate::control::find_program("ddcutil"),
			Programs::Given { ddcutil, .. } => ddcutil.clone(),
		}
	}
}

struct State {
	def: DisplaysDef,
	wanted_until: Option<Instant>,
	seen: Arc<Seen>,
	instance: Option<String>,
	/// When Hyprland was last looked for.
	looked: Option<Instant>,
	/// Each DDC bus's last reading and when it was.
	ddc: BTreeMap<u32, (Instant, Option<Level>)>,
	/// What the journal was told, once each.
	said: BTreeSet<String>,
}

/// The monitors' part of the service.
pub struct Displays {
	state: Mutex<State>,
	wake: Condvar,
	/// One action on the monitors at a time.
	acting: Arc<tokio::sync::Mutex<()>>,
	/// One `ddcutil` at a time: they share the monitors' buses.
	ddc_bus: Mutex<()>,
	programs: Programs,
	sys: PathBuf,
	dev: PathBuf,
	/// Whether it tells the journal what it finds: the service's does, a command's doesn't.
	journal: bool,
}

/// What is said to the journal, under `displays:`.
fn say(s: &str) {
	eprintln!("displays: {s}");
}

impl Displays {
	/// On this computer: `/sys` and `/dev`, or the folders `ELAMON_SYSFS` and `ELAMON_DEV` name (an
	/// end-to-end test's own, so a stand-in monitor never reaches a real backlight or bus).
	pub fn new() -> Displays {
		let root = |var: &str, default: &str| std::env::var_os(var).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(default));
		Displays::with(Programs::Found, root("ELAMON_SYSFS", "/sys"), root("ELAMON_DEV", "/dev"))
	}

	/// For a command: what it finds goes to the person, not to the journal.
	pub fn quiet() -> Displays {
		Displays { journal: false, ..Displays::new() }
	}

	/// With `hyprctl` and `ddcutil` where the test puts them, and its own sysfs.
	#[cfg(test)]
	pub fn given(hyprctl: Option<PathBuf>, ddcutil: Option<PathBuf>, sys: PathBuf, dev: PathBuf) -> Displays {
		Displays::with(Programs::Given { hyprctl, ddcutil }, sys, dev)
	}

	fn with(programs: Programs, sys: PathBuf, dev: PathBuf) -> Displays {
		Displays {
			state: Mutex::new(State { def: DisplaysDef::default(), wanted_until: None, seen: Arc::default(), instance: None, looked: None, ddc: BTreeMap::new(), said: BTreeSet::new() }),
			wake: Condvar::new(),
			acting: Arc::new(tokio::sync::Mutex::new(())),
			ddc_bus: Mutex::new(()),
			programs,
			sys,
			dev,
			journal: true,
		}
	}

	fn lock(&self) -> std::sync::MutexGuard<'_, State> {
		self.state.lock().unwrap_or_else(|p| p.into_inner())
	}

	pub fn seen(&self) -> Arc<Seen> {
		self.lock().seen.clone()
	}

	pub fn apply(&self, def: &DisplaysDef) {
		self.lock().def = def.clone();
	}

	/// A client shows the monitors now (`true`, each tick it does): they are read for a while.
	pub fn want(&self, now: bool) {
		if !now {
			return;
		}
		let mut s = self.lock();
		let was = s.wanted_until.is_some_and(|t| t > Instant::now());
		s.wanted_until = Some(Instant::now() + cadence::DISPLAYS_LINGER);
		if !was {
			self.wake.notify_all();
		}
	}

	/// Reads the monitors once, then while a client shows them, on a thread of its own.
	pub fn start(self: &Arc<Self>) {
		let me = self.clone();
		let _ = std::thread::Builder::new().name("displays".into()).spawn(move || {
			me.refresh();
			loop {
				let (wanted, every) = {
					let s = me.lock();
					(s.wanted_until.is_some_and(|t| t > Instant::now()), Duration::from_secs(s.def.every.max(1)))
				};
				if wanted {
					me.refresh();
				}
				let s = me.lock();
				let wait = if wanted { every } else { Duration::from_secs(1) };
				drop(me.wake.wait_timeout(s, wait));
			}
		});
	}

	fn say_once(&self, s: String) {
		if self.lock().said.insert(s.clone()) && self.journal {
			say(&s);
		}
	}

	/// The Hyprland to act on, looked for again when there is none (at most every
	/// `HYPRLAND_LOOK`, unless `now`) or the last call failed.
	fn instance(&self, hyprctl: &Hyprctl, now: bool) -> Result<String, String> {
		{
			let s = self.lock();
			if let Some(i) = &s.instance {
				return Ok(i.clone());
			}
			if !now && s.looked.is_some_and(|t| t.elapsed() < cadence::HYPRLAND_LOOK) {
				return Err(s.seen.why.clone().unwrap_or_default());
			}
		}
		self.lock().looked = Some(Instant::now());
		let list = hyprctl.instances().map_err(|e| match e {
			Failed::Missing => "hyprctl isn't there".to_string(),
			e => format!("hyprctl instances: {e}"),
		})?;
		let Some(newest) = list.first() else { return Err("no Hyprland runs".into()) };
		if list.len() > 1 {
			self.say_once(format!("{} Hyprland instances run: acting on the newest, {}", list.len(), newest.instance));
		}
		match hyprctl.lua(&newest.instance) {
			Ok(true) => {}
			Ok(false) => return Err("Hyprland's settings aren't Lua".into()),
			Err(e) => return Err(format!("hyprctl eval: {e}")),
		}
		self.say_once(format!("Hyprland {} found", newest.instance));
		self.lock().instance = Some(newest.instance.clone());
		Ok(newest.instance.clone())
	}

	/// What Hyprland says now, read afresh, and kept for the collector.
	pub fn refresh(&self) -> Arc<Seen> {
		self.read(false, true)
	}

	/// What Hyprland says now; `now` looks for Hyprland again if there is none, `ddc` reads an
	/// external monitor's brightness when it is due (an action doesn't: a monitor waking up would fail
	/// it, and the phone would wait on it).
	fn read(&self, now: bool, ddc: bool) -> Arc<Seen> {
		let seen = self.look(now, ddc);
		let seen = Arc::new(seen);
		let mut s = self.lock();
		if s.seen.why != seen.why
			&& self.journal
			&& let Some(why) = &seen.why
		{
			say(&format!("monitors aren't acted on: {why}"));
		}
		s.seen = seen.clone();
		seen
	}

	fn look(&self, now: bool, ddc: bool) -> Seen {
		let Some(hyprctl) = self.programs.hyprctl() else {
			return Seen { why: Some("hyprctl isn't there".into()), ..Default::default() };
		};
		let mut tries = 0;
		let (instance, outputs) = loop {
			let instance = match self.instance(&hyprctl, now || tries > 0) {
				Ok(i) => i,
				Err(why) => return Seen { why: Some(why), ..Default::default() },
			};
			match hyprctl.outputs(&instance) {
				Ok(o) => break (instance, o),
				// Hyprland started again, with another instance: looked for once more at once.
				Err(e) => {
					self.lock().instance = None;
					tries += 1;
					if tries > 1 {
						return Seen { why: Some(format!("hyprctl monitors: {e}")), ..Default::default() };
					}
				}
			}
		};
		for o in outputs.iter().filter(|o| !hypr::fit_name(&o.name)) {
			self.say_once(format!("an output named {:?} isn't acted on: its name has more than A-Z, a-z, 0-9, _, . and -", o.name));
		}
		let light = self.light(&outputs, ddc);
		Seen { instance: Some(instance), why: None, outputs, light, at: Some(Instant::now()) }
	}

	/// Each output's brightness: a panel's from sysfs every time; an external monitor's through DDC
	/// every `ddc_every` (with `ddc`), only while it shows a picture (reading it may wake it), else its
	/// last; one that doesn't answer keeps its last level.
	fn light(&self, outputs: &[Output], ddc: bool) -> BTreeMap<String, (Way, Option<Level>)> {
		let every = Duration::from_secs(self.lock().def.ddc_every);
		let ddcutil = self.programs.ddcutil();
		let mut out = BTreeMap::new();
		for o in outputs.iter().filter(|o| !o.headless() && hypr::fit_name(&o.name)) {
			let Some(dir) = light::connector_dir(&self.sys, &o.name) else { continue };
			let Some(way) = light::way_of(&self.sys, &dir) else { continue };
			let level = match &way {
				Way::Backlight(device) => light::backlight(&self.sys, device),
				Way::Ddc(bus) => {
					let Some(program) = ddcutil.as_ref().filter(|_| light::i2c_open(&self.dev, *bus)) else {
						out.insert(o.name.clone(), (way.clone(), None));
						continue;
					};
					let last = self.lock().ddc.get(bus).cloned();
					match last {
						Some((at, level)) if at.elapsed() < every || !o.lit() || !ddc => level.filter(|_| o.lit()),
						_ if !o.lit() || !ddc => None,
						_ => {
							let read = {
								let _bus = self.ddc_bus.lock().unwrap_or_else(|p| p.into_inner());
								light::ddc_get(program, *bus).ok()
							};
							let level = read.or(last.and_then(|(_, l)| l));
							self.lock().ddc.insert(*bus, (Instant::now(), level));
							level
						}
					}
				}
			};
			out.insert(o.name.clone(), (way, level));
		}
		out
	}

	/// Runs a monitor's action with its checked parameters; `confirm_text` as the client sent it.
	/// `Err` is the code and the message it is answered with.
	pub async fn act(self: &Arc<Self>, bus: &crate::bus::Bus, id: &str, params: &BTreeMap<String, Value>, confirm_text: Option<&str>) -> Result<(), (ErrorCode, Option<String>)> {
		let Some((what, name)) = target(id) else { return Err((ErrorCode::NoSuchAction, None)) };
		let name = name.to_string();
		// Owned, so that an action whose request goes away still holds it until it is done.
		let one = self.acting.clone().lock_owned().await;
		let me = self.clone();
		let seen = tokio::task::spawn_blocking(move || me.read(true, false)).await.map_err(|e| (ErrorCode::Failed, Some(e.to_string())))?;
		let def = self.lock().def.clone();
		let Some(o) = seen.shown(&def).find(|o| o.name == name).cloned() else { return Err((ErrorCode::NoSuchAction, None)) };
		let Some(instance) = seen.instance.clone() else { return Err((ErrorCode::NoSuchAction, None)) };
		let state = params.get("state").and_then(Value::as_str);
		let wait = Duration::from_secs(def.wait);
		match what {
			What::Output | What::Dpms => {
				// A disabled output has no picture to put to sleep or wake.
				if what == What::Dpms && o.disabled {
					return Err((ErrorCode::NoSuchAction, None));
				}
				let on = state == Some("on");
				let (lua, undo, done): (String, String, fn(&Output, bool) -> bool) = match what {
					What::Output => (if on { hypr::lua_on(&o) } else { hypr::lua_off(&o.name) }, hypr::lua_on(&o), |o, on| o.disabled != on),
					_ => (hypr::lua_dpms(&o.name, on), hypr::lua_dpms(&o.name, true), |o, on| o.dpms_status == on),
				};
				// Nothing to do asks for nothing.
				if done(&o, on) {
					return Ok(());
				}
				if !on && seen.typed_off(&o) && confirm_text != Some(o.name.as_str()) {
					return Err((ErrorCode::ConfirmRequired, Some(o.name.clone())));
				}
				let me = self.clone();
				let asleep = !seen.outputs.iter().any(|x| !x.headless() && x.lit());
				let result = tokio::task::spawn_blocking(move || {
					let _one = one;
					let hyprctl = me.programs.hyprctl().ok_or_else(|| "hyprctl isn't there".to_string())?;
					hyprctl.eval(&instance, &lua).map_err(|e| e.to_string())?;
					let deadline = Instant::now() + wait;
					let shown = loop {
						if let Ok(outputs) = hyprctl.outputs(&instance)
							&& outputs.iter().find(|x| x.name == o.name).is_some_and(|x| done(x, on))
						{
							break true;
						}
						if Instant::now() >= deadline {
							break false;
						}
						std::thread::sleep(cadence::DISPLAYS_VERIFY);
					};
					// An off Hyprland hasn't applied yet would still come with its next frame, behind the
					// next action's back (which takes the screen as lit): taken back, so not done means
					// nothing changed.
					if !shown && !on {
						let _ = hyprctl.eval(&instance, &undo);
					}
					// What may be done next, and who is last, as Hyprland has it now: with nobody watching
					// the monitors the thread wouldn't read it again.
					me.read(false, false);
					Ok(shown)
				})
				.await
				.map_err(|e| (ErrorCode::Failed, Some(e.to_string())))?;
				match result {
					Ok(true) => Ok(()),
					Ok(false) if what == What::Output && on && asleep => Err((ErrorCode::Failed, Some(reason::WAITING_FOR_A_SCREEN.into()))),
					Ok(false) => Err((ErrorCode::Failed, Some(reason::NOT_APPLIED.into()))),
					Err(e) => Err((ErrorCode::Failed, Some(e))),
				}
			}
			What::Brightness => {
				let percent = params.get("level").and_then(Value::as_f64).ok_or((ErrorCode::BadParam, Some("level".to_string())))?;
				let Some((way, Some(level))) = seen.light.get(&name).cloned() else { return Err((ErrorCode::NoSuchAction, None)) };
				let raw = level.raw_of(percent);
				let result = match &way {
					Way::Backlight(device) => bus.set_brightness("backlight", device, raw).await,
					Way::Ddc(n) => {
						let (program, n) = (self.programs.ddcutil(), *n);
						let me = self.clone();
						let set = tokio::task::spawn_blocking(move || match program {
							Some(p) => {
								let _bus = me.ddc_bus.lock().unwrap_or_else(|p| p.into_inner());
								light::ddc_set(&p, n, raw)
							}
							None => Err("ddcutil isn't there".into()),
						})
						.await
						.unwrap_or_else(|e| Err(e.to_string()));
						if set.is_ok() {
							self.lock().ddc.insert(n, (Instant::now(), Some(Level { raw, ..level })));
						}
						set
					}
				};
				let me = self.clone();
				tokio::task::spawn_blocking(move || me.read(false, false)).await.ok();
				drop(one);
				result.map_err(|e| (ErrorCode::Failed, Some(e)))
			}
		}
	}

	/// What `elamond check` and `setup --check` say of the monitors, a line each: whether they are
	/// acted on (`true`), or what is missing (`false`): Hyprland, `ddcutil`, the I²C bus, a monitor
	/// that doesn't answer.
	pub fn check(&self) -> Vec<(bool, String)> {
		let seen = self.read(true, true);
		if let Some(why) = &seen.why {
			return vec![(false, format!("monitors: not turned on and off from a phone: {why}"))];
		}
		let mut out = vec![(true, format!("monitors: Hyprland on Lua: {} turned on and off from a phone", seen.outputs.iter().filter(|o| !o.headless()).map(|o| o.name.as_str()).collect::<Vec<_>>().join(", ")))];
		let ddcutil = self.programs.ddcutil();
		for (name, (way, level)) in &seen.light {
			let line = match way {
				Way::Backlight(device) if level.is_some() => (true, format!("monitors: {name}'s brightness through logind ({device})")),
				Way::Backlight(device) => (false, format!("monitors: {name}'s backlight {device} can't be read")),
				Way::Ddc(bus) if ddcutil.is_none() => (false, format!("monitors: {name}'s brightness needs ddcutil, and access to /dev/i2c-{bus}")),
				Way::Ddc(bus) if !light::i2c_open(&self.dev, *bus) => (false, format!("monitors: {name}'s brightness needs access to /dev/i2c-{bus}: add the user to the i2c group")),
				Way::Ddc(_) if level.is_some() => (true, format!("monitors: {name}'s brightness through DDC")),
				Way::Ddc(_) if seen.output(name).is_some_and(Output::lit) => (false, format!("monitors: {name} doesn't answer DDC: no brightness for it")),
				Way::Ddc(_) => (true, format!("monitors: {name}'s brightness through DDC, read once it shows a picture")),
			};
			out.push(line);
		}
		out
	}

	/// `elamond values displays`: what the service sees of each output, and the actions it offers.
	pub fn listing(&self, def: &DisplaysDef) -> String {
		let seen = self.read(true, true);
		let Some(instance) = &seen.instance else {
			return format!("Monitors aren't acted on: {}.\n", seen.why.as_deref().unwrap_or("no Hyprland"));
		};
		let mut out = format!("Hyprland {instance}, its settings Lua.\n");
		let width = seen.outputs.iter().map(|o| o.name.chars().count()).max().unwrap_or(0);
		for o in &seen.outputs {
			let mut parts = vec![if o.disabled { "off".to_string() } else { format!("on · DPMS {}", if o.dpms_status { "on" } else { "off" }) }];
			if o.width > 0 {
				parts.push(format!("{}×{} @ {:.2} Hz at {}, {}, scale {}", o.width, o.height, o.refresh_rate, o.x, o.y, number(o.scale)));
			}
			match seen.light.get(&o.name) {
				Some((way, level)) => {
					let how = match way {
						Way::Backlight(d) => format!("backlight {d}"),
						Way::Ddc(b) => format!("DDC on i2c-{b}"),
					};
					parts.push(match level {
						Some(l) => format!("brightness {:.0} % ({how})", l.percent()),
						None => format!("brightness unread ({how})"),
					});
				}
				None if !o.headless() => parts.push("no brightness".into()),
				None => {}
			}
			if o.headless() {
				parts.push(if def.headless { "no screen".into() } else { "no screen: left out ([displays] headless = false)".into() });
			} else if seen.typed_off(&o.clone()) {
				parts.push("no other screen shows a picture: its name is typed to turn it off".into());
			}
			if !hypr::fit_name(&o.name) {
				parts.push("not acted on: its name".into());
			}
			out.push_str(&format!("{:width$}  {}\n", o.name, parts.join(" · ")));
		}
		let ids: Vec<String> = specs(&seen, def).into_iter().map(|s| s.id).collect();
		out.push_str(&format!("Actions: {}\n", if ids.is_empty() { "none".to_string() } else { ids.join(", ") }));
		out
	}
}

/// A number as Lua and people take it: `1`, `1.25`.
fn number(x: f64) -> String {
	format!("{}", (x * 1000.0).round() / 1000.0)
}

/// What Hyprland says, put on the monitors DRM tells of (matched by name: Hyprland names a screen
/// by its connector); outputs without a screen as rows of their own when `[displays] headless`.
pub fn merge(displays: &mut Vec<crate::model::Display>, seen: &Seen, def: &DisplaysDef) {
	if seen.instance.is_none() {
		return;
	}
	for o in seen.shown(def) {
		let controls = crate::model::Controls { enabled: !o.disabled, dpms: o.dpms_status, headless: o.headless(), scale: o.scale, brightness: seen.brightness(&o.name) };
		if let Some(d) = displays.iter_mut().find(|d| d.connector == o.name) {
			d.active = d.connected && o.lit();
			d.controls = Some(controls);
		} else if o.headless() {
			let shown = o.width > 0 && o.height > 0;
			displays.push(crate::model::Display {
				connector: o.name.clone(),
				gpu: String::new(),
				connected: true,
				active: o.lit(),
				dpms: None,
				internal: false,
				vendor: None,
				model: None,
				year: None,
				width_mm: None,
				height_mm: None,
				mode: shown.then(|| crate::model::Mode { width: o.width, height: o.height, refresh: ((o.refresh_rate * 100.0).round() / 100.0) as f32 }),
				preferred: None,
				max_refresh: None,
				position: (shown && o.x >= 0 && o.y >= 0).then_some([o.x as u32, o.y as u32]),
				controls: Some(controls),
			});
		}
	}
}

impl Default for Displays {
	fn default() -> Self {
		Displays::new()
	}
}

#[cfg(test)]
pub(crate) mod tests;
