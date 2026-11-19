//! The monitors' actions against a stand-in `hyprctl` and `ddcutil`: scripts that keep their
//! state in files beside them, and a sysfs and /dev of the test's own.

use std::path::PathBuf;

use serde_json::json;

use super::*;
use crate::settings::DisplaysDef;

/// A stand-in hyprctl. Its outputs are files in `outputs/` (`disabled=…`, `dpms=…`, `backend=…`);
/// the instances it lists are `instances`; each call goes to `log`, each Lua it is given to
/// `evals`. Files beside it change what it does: `hang` (it never answers), `hyprlang` (no Lua),
/// `asleep` (an output turned on stays off: no screen draws), `stuck` (nothing it is told is done).
const HYPRCTL: &str = r#"#!/bin/sh
here=$(dirname "$0")
printf '%s\n' "$*" >> "$here/log"
[ -f "$here/hang" ] && sleep 30
sig=""
if [ "$1" = "--instance" ]; then sig="$2"; shift 2; fi
if [ "$1 $2" = "-j instances" ]; then cat "$here/instances"; exit 0; fi
if [ -n "$sig" ] && ! grep -q "\"$sig\"" "$here/instances"; then echo "error: no such instance" >&2; exit 1; fi
field() { v=${1#*$2 = \'}; printf '%s' "${v%%\'*}"; }
set_key() { f="$here/outputs/$1"; [ -f "$f" ] || exit 0; sed -i "s/^$2=.*/$2=$3/" "$f"; }
case "$1" in
eval)
	[ -f "$here/hyprlang" ] && { echo "error: eval needs Lua settings"; exit 7; }
	[ "$2" = "return" ] && { echo ok; exit 0; }
	printf '%s\n' "$2" >> "$here/evals"
	[ -f "$here/stuck" ] && { echo ok; exit 0; }
	case "$2" in
	"hl.monitor("*"disabled = true"*) set_key "$(field "$2" output)" disabled true ;;
	"hl.monitor("*"disabled = false"*) [ -f "$here/asleep" ] || set_key "$(field "$2" output)" disabled false ;;
	"hl.dispatch(hl.dsp.dpms("*)
		if [ "$(field "$2" action)" = on ]; then set_key "$(field "$2" monitor)" dpms true; else set_key "$(field "$2" monitor)" dpms false; fi ;;
	*) echo "error: unknown"; exit 7 ;;
	esac
	echo ok ;;
-j)
	printf '['
	first=1
	for f in "$here"/outputs/*; do
		disabled=false; dpms=true; backend=drm
		. "$f"
		[ $first = 1 ] || printf ','
		first=0
		printf '{"name":"%s","disabled":%s,"dpmsStatus":%s,"width":1920,"height":1080,"refreshRate":144.149,"x":0,"y":0,"scale":1,"transform":0,"mirrorOf":"none","hardwareDetails":{"backend":"%s"}}' "$(basename "$f")" "$disabled" "$dpms" "$backend"
	done
	printf ']\n' ;;
*) echo "error: unknown request"; exit 1 ;;
esac
"#;

/// A stand-in ddcutil: the brightness of its one monitor in `vcp`, each call in `calls`.
const DDCUTIL: &str = r#"#!/bin/sh
here=$(dirname "$0")
printf '%s\n' "$*" >> "$here/calls"
case "$3" in
getvcp) printf 'VCP 10 C %s 100\n' "$(cat "$here/vcp")" ;;
setvcp) printf '%s' "$5" > "$here/vcp" ;;
esac
"#;

pub(crate) struct Fake {
	pub(crate) dir: PathBuf,
	pub(crate) d: Arc<Displays>,
}

impl Fake {
	/// eDP-1 off and DP-1 on, both screens, and a stream's output; DP-1's DDC on i2c-14.
	pub(crate) fn new(def: DisplaysDef) -> Fake {
		let dir = std::env::temp_dir().join(format!("elamond-displays-{}", data_encoding::HEXLOWER.encode(&crate::store::random_bytes::<6>())));
		std::fs::create_dir_all(dir.join("bin/outputs")).unwrap();
		let write_exe = |name: &str, text: &str| {
			use std::os::unix::fs::PermissionsExt;
			let p = dir.join("bin").join(name);
			std::fs::write(&p, text).unwrap();
			std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
			p
		};
		let hyprctl = write_exe("hyprctl", HYPRCTL);
		let ddcutil = write_exe("ddcutil", DDCUTIL);
		std::fs::write(dir.join("bin/vcp"), "60").unwrap();
		let dp = dir.join("sys/class/drm/card1-DP-1");
		std::fs::create_dir_all(&dp).unwrap();
		std::os::unix::fs::symlink("../../i2c-14", dp.join("ddc")).unwrap();
		std::fs::create_dir_all(dir.join("dev")).unwrap();
		std::fs::write(dir.join("dev/i2c-14"), "").unwrap();
		let f = Fake { d: Arc::new(Displays::given(Some(hyprctl), Some(ddcutil), dir.join("sys"), dir.join("dev"))), dir };
		f.instances(&[("one_1", 100)]);
		f.output("eDP-1", "disabled=true\n");
		f.output("DP-1", "");
		f.output("stream", "backend=headless\n");
		f.d.apply(&def);
		f
	}

	fn instances(&self, list: &[(&str, u64)]) {
		let items: Vec<_> = list.iter().map(|(s, t)| json!({ "instance": s, "time": t, "pid": 1, "wl_socket": "wayland-1" })).collect();
		std::fs::write(self.dir.join("bin/instances"), serde_json::to_string(&items).unwrap()).unwrap();
	}

	fn output(&self, name: &str, text: &str) {
		std::fs::write(self.dir.join("bin/outputs").join(name), format!("disabled=false\ndpms=true\nbackend=drm\n{text}")).unwrap();
	}

	fn flag(&self, name: &str, on: bool) {
		let p = self.dir.join("bin").join(name);
		if on {
			std::fs::write(p, "").unwrap();
		} else {
			let _ = std::fs::remove_file(p);
		}
	}

	pub(crate) fn state(&self, name: &str) -> String {
		std::fs::read_to_string(self.dir.join("bin/outputs").join(name)).unwrap()
	}

	fn lines(&self, file: &str) -> Vec<String> {
		std::fs::read_to_string(self.dir.join("bin").join(file)).unwrap_or_default().lines().map(String::from).collect()
	}

	fn evals(&self) -> Vec<String> {
		self.lines("evals")
	}

	async fn act(&self, id: &str, params: serde_json::Value, text: Option<&str>) -> Result<(), (ErrorCode, Option<String>)> {
		let params: BTreeMap<String, Value> = params.as_object().unwrap().clone().into_iter().collect();
		self.d.act(&crate::bus::Bus::new(), id, &params, text).await
	}
}

impl Drop for Fake {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.dir);
	}
}

fn quick() -> DisplaysDef {
	DisplaysDef { wait: 1, ..DisplaysDef::default() }
}

fn ids(seen: &Seen, def: &DisplaysDef) -> Vec<String> {
	specs(seen, def).into_iter().map(|s| s.id).collect()
}

fn typed(seen: &Seen, def: &DisplaysDef, id: &str) -> Option<String> {
	specs(seen, def).into_iter().find(|s| s.id == id)?.params[0].typed.get("off").cloned()
}

#[test]
fn what_hyprland_shows_and_what_may_be_done() {
	let f = Fake::new(quick());
	let seen = f.d.refresh();
	assert_eq!(seen.instance.as_deref(), Some("one_1"));
	assert_eq!(seen.outputs.iter().map(|o| o.name.as_str()).collect::<Vec<_>>(), ["DP-1", "eDP-1", "stream"]);
	let def = quick();
	// The stream's output is left out; the panel, off, has no picture to put to sleep.
	assert_eq!(ids(&seen, &def), ["display.output.DP-1", "display.dpms.DP-1", "display.brightness.DP-1", "display.output.eDP-1"]);
	assert_eq!(typed(&seen, &def, "display.output.DP-1").as_deref(), Some("DP-1"));
	assert_eq!(typed(&seen, &def, "display.dpms.DP-1").as_deref(), Some("DP-1"));
	assert_eq!(typed(&seen, &def, "display.output.eDP-1"), None);
	assert_eq!(seen.brightness("DP-1"), Some(60.0));
	// Shown too, the stream's output never counts as a screen showing a picture.
	let all = DisplaysDef { headless: true, ..quick() };
	assert!(ids(&seen, &all).contains(&"display.output.stream".to_string()));
	assert_eq!(typed(&seen, &all, "display.output.stream"), None, "a stream's output leaves nobody in the dark");
	assert_eq!(typed(&seen, &all, "display.output.DP-1").as_deref(), Some("DP-1"));
	// With the panel on as well, neither is the last.
	f.output("eDP-1", "");
	let seen = f.d.refresh();
	assert_eq!((typed(&seen, &def, "display.output.DP-1"), typed(&seen, &def, "display.output.eDP-1")), (None, None));
	// Asleep, DP-1 no longer counts: the panel is the one showing a picture; DP-1's brightness
	// isn't read while it sleeps, so it has no slider.
	f.output("DP-1", "dpms=false\n");
	let seen = f.d.refresh();
	assert_eq!(typed(&seen, &def, "display.output.eDP-1").as_deref(), Some("eDP-1"));
	assert_eq!(seen.brightness("DP-1"), None);
	assert!(!ids(&seen, &def).contains(&"display.brightness.DP-1".to_string()));
}

#[tokio::test]
async fn the_last_screen_goes_off_only_with_its_name_typed() {
	let f = Fake::new(quick());
	f.d.refresh();
	for text in [None, Some("off"), Some("dp-1"), Some("DP-1 ")] {
		assert_eq!(f.act("display.output.DP-1", json!({ "state": "off" }), text).await, Err((ErrorCode::ConfirmRequired, Some("DP-1".into()))), "{text:?}");
		assert_eq!(f.act("display.dpms.DP-1", json!({ "state": "off" }), text).await, Err((ErrorCode::ConfirmRequired, Some("DP-1".into()))), "{text:?}");
	}
	assert!(f.evals().is_empty(), "nothing reached Hyprland: {:?}", f.evals());
	assert_eq!(f.act("display.output.DP-1", json!({ "state": "off" }), Some("DP-1")).await, Ok(()));
	assert!(f.state("DP-1").contains("disabled=true"));
	assert_eq!(f.evals(), ["hl.monitor({ output = 'DP-1', disabled = true })"]);
	// Turned on with what Hyprland kept for it; no name asked.
	assert_eq!(f.act("display.output.DP-1", json!({ "state": "on" }), None).await, Ok(()));
	assert_eq!(f.evals()[1], "hl.monitor({ output = 'DP-1', mode = '1920x1080@144.149', position = '0x0', scale = 1, transform = 0, disabled = false })");
	// The panel on, DP-1 is no longer the last: its picture goes to sleep as asked.
	assert_eq!(f.act("display.output.eDP-1", json!({ "state": "on" }), None).await, Ok(()));
	assert_eq!(f.act("display.dpms.DP-1", json!({ "state": "off" }), None).await, Ok(()));
	assert!(f.state("DP-1").contains("dpms=false"));
	assert_eq!(f.evals().last().unwrap(), "hl.dispatch(hl.dsp.dpms({ action = 'off', monitor = 'DP-1' }))");
	// Now the panel is the last: DPMS off instead of off is asked the same.
	assert_eq!(f.act("display.dpms.eDP-1", json!({ "state": "off" }), None).await, Err((ErrorCode::ConfirmRequired, Some("eDP-1".into()))));
	assert_eq!(f.act("display.output.eDP-1", json!({ "state": "off" }), None).await, Err((ErrorCode::ConfirmRequired, Some("eDP-1".into()))));
	// Done already: nothing to do, nothing sent.
	let before = f.evals().len();
	assert_eq!(f.act("display.output.eDP-1", json!({ "state": "on" }), None).await, Ok(()));
	assert_eq!(f.evals().len(), before);
}

#[tokio::test]
async fn two_actions_at_once_are_told_on_what_the_first_left() {
	let f = Fake::new(quick());
	f.output("eDP-1", "");
	f.d.refresh();
	// Both screens on: each alone may go without its name, but not both.
	let (a, b) = tokio::join!(f.act("display.output.DP-1", json!({ "state": "off" }), None), f.act("display.output.eDP-1", json!({ "state": "off" }), None));
	let results = [a, b];
	assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1, "{results:?}");
	assert!(results.iter().any(|r| matches!(r, Err((ErrorCode::ConfirmRequired, _)))), "{results:?}");
	assert_eq!([f.state("DP-1"), f.state("eDP-1")].iter().filter(|s| s.contains("disabled=true")).count(), 1);
}

#[tokio::test]
async fn an_action_is_done_once_hyprland_shows_it() {
	let f = Fake::new(quick());
	f.output("eDP-1", "");
	f.d.refresh();
	f.flag("stuck", true);
	assert_eq!(f.act("display.output.eDP-1", json!({ "state": "off" }), None).await, Err((ErrorCode::Failed, Some(reason::NOT_APPLIED.into()))));
	f.flag("stuck", false);
	// Every screen asleep: one turned on waits for one to wake.
	f.output("DP-1", "dpms=false\n");
	f.output("eDP-1", "disabled=true\n");
	f.flag("asleep", true);
	assert_eq!(f.act("display.output.eDP-1", json!({ "state": "on" }), None).await, Err((ErrorCode::Failed, Some(reason::WAITING_FOR_A_SCREEN.into()))));
}

#[tokio::test]
async fn an_output_gone_or_never_there_is_no_action() {
	let f = Fake::new(quick());
	f.d.refresh();
	std::fs::remove_file(f.dir.join("bin/outputs/eDP-1")).unwrap();
	assert_eq!(f.act("display.output.eDP-1", json!({ "state": "on" }), None).await, Err((ErrorCode::NoSuchAction, None)));
	for id in ["display.output.HDMI-A-1", "display.output.stream", "display.output.a'b", "display.output.x})", "display.fan.DP-1", "display.output."] {
		assert_eq!(f.act(id, json!({ "state": "on" }), None).await, Err((ErrorCode::NoSuchAction, None)), "{id}");
	}
	assert!(f.evals().is_empty());
}

#[test]
fn a_name_with_more_than_letters_digits_and_dashes_is_never_acted_on() {
	let f = Fake::new(quick());
	f.output("a'b", "");
	f.output("x})", "");
	let seen = f.d.refresh();
	assert_eq!(seen.outputs.len(), 5);
	let ids = ids(&seen, &quick());
	assert!(ids.iter().all(|id| !id.contains('\'') && !id.contains('}')), "{ids:?}");
	assert!(f.d.listing(&quick()).contains("not acted on: its name"));
}

#[test]
fn hyprland_is_found_again_and_a_hanging_hyprctl_is_left_behind() {
	let f = Fake::new(quick());
	assert_eq!(f.d.refresh().instance.as_deref(), Some("one_1"));
	// Hyprland started again: the old instance answers no more, the new one is taken at once.
	f.instances(&[("two_2", 200)]);
	assert_eq!(f.d.refresh().instance.as_deref(), Some("two_2"));
	// Two run: the newest.
	f.instances(&[("two_2", 200), ("three_3", 300)]);
	f.d.lock().instance = None;
	assert_eq!(f.d.read(true).instance.as_deref(), Some("three_3"));
	// Settings that aren't Lua: none, and no actions.
	f.d.lock().instance = None;
	f.flag("hyprlang", true);
	let seen = f.d.read(true);
	assert_eq!((seen.instance.as_deref(), seen.why.as_deref()), (None, Some("Hyprland's settings aren't Lua")));
	assert!(specs(&seen, &quick()).is_empty());
	f.flag("hyprlang", false);
	// A hyprctl that hangs is killed in its time, and says so.
	f.flag("hang", true);
	let started = Instant::now();
	let seen = f.d.read(true);
	assert!(started.elapsed() < crate::consts::timeout::HYPRCTL * 2, "{:?}", started.elapsed());
	assert!(seen.why.as_deref().is_some_and(|w| w.contains("took too long")), "{:?}", seen.why);
	f.flag("hang", false);
	// Without hyprctl at all.
	let none = Displays::given(None, None, f.dir.join("sys"), f.dir.join("dev"));
	assert_eq!(none.refresh().why.as_deref(), Some("hyprctl isn't there"));
	assert_eq!(none.check(), [(false, "monitors: not turned on and off from a phone: hyprctl isn't there".to_string())]);
}

#[tokio::test]
async fn an_external_monitors_brightness_through_ddc() {
	let f = Fake::new(quick());
	let seen = f.d.refresh();
	assert_eq!(seen.light.get("DP-1"), Some(&(Way::Ddc(14), Some(Level { raw: 60, max: 100 }))));
	assert_eq!(f.act("display.brightness.DP-1", json!({ "level": 35 }), None).await, Ok(()));
	assert_eq!(std::fs::read_to_string(f.dir.join("bin/vcp")).unwrap(), "35");
	assert!(f.lines("calls").iter().any(|l| l == "--bus 14 setvcp 10 35 --noverify"), "{:?}", f.lines("calls"));
	assert_eq!(f.d.seen().brightness("DP-1"), Some(35.0));
	// Read again only after ddc_every, and never while the monitor sleeps.
	let reads = || f.lines("calls").iter().filter(|l| l.contains("getvcp")).count();
	let n = reads();
	f.d.refresh();
	assert_eq!(reads(), n);
	f.d.lock().ddc.clear();
	f.output("DP-1", "dpms=false\n");
	assert_eq!(f.d.refresh().light["DP-1"].1, None);
	assert_eq!(reads(), n);
	// Without access to the bus: no slider, and check says why.
	std::fs::remove_file(f.dir.join("dev/i2c-14")).unwrap();
	f.output("DP-1", "");
	let seen = f.d.refresh();
	assert_eq!(seen.light["DP-1"].1, None);
	assert!(!ids(&seen, &quick()).contains(&"display.brightness.DP-1".to_string()));
	assert!(f.d.check().iter().any(|(ok, l)| !ok && l.contains("/dev/i2c-14")), "{:?}", f.d.check());
}

#[test]
fn hyprland_put_on_drms_monitors() {
	let f = Fake::new(quick());
	let seen = f.d.refresh();
	let drm = |name: &str| crate::model::Display {
		connector: name.into(),
		gpu: "0000:00:02.0".into(),
		connected: true,
		active: true,
		dpms: Some("On".into()),
		internal: name.starts_with("eDP"),
		vendor: None,
		model: None,
		year: None,
		width_mm: None,
		height_mm: None,
		mode: None,
		preferred: None,
		max_refresh: None,
		position: None,
		controls: None,
	};
	let mut displays = vec![drm("eDP-1"), drm("DP-1"), drm("HDMI-A-1")];
	merge(&mut displays, &seen, &DisplaysDef { headless: true, ..quick() });
	let names: Vec<(&str, bool, Option<bool>)> = displays.iter().map(|d| (d.connector.as_str(), d.active, d.controls.as_ref().map(|c| c.enabled))).collect();
	assert_eq!(names, [("eDP-1", false, Some(false)), ("DP-1", true, Some(true)), ("HDMI-A-1", true, None), ("stream", true, Some(true))]);
	assert!(displays[3].controls.as_ref().unwrap().headless);
	let mut hidden = vec![drm("DP-1")];
	merge(&mut hidden, &seen, &quick());
	assert_eq!(hidden.len(), 1);
}

#[test]
fn hyprland_is_asked_only_while_someone_looks() {
	let f = Fake::new(DisplaysDef { every: 1, ..quick() });
	f.d.start();
	std::thread::sleep(Duration::from_millis(500));
	// Read once at start; then not at all while nobody wants them.
	let at_start = f.lines("log").len();
	assert!(at_start > 0 && f.d.seen().instance.is_some());
	std::thread::sleep(Duration::from_millis(2500));
	assert_eq!(f.lines("log").len(), at_start);
	// Wanted: read at once and every second.
	f.d.want(true);
	std::thread::sleep(Duration::from_millis(1600));
	let wanted = f.lines("log").len();
	assert!(wanted >= at_start + 2, "{at_start} → {wanted}");
	// No longer wanted: the reading stops once the last want has lingered out.
	std::thread::sleep(cadence::DISPLAYS_LINGER + Duration::from_millis(1200));
	let after = f.lines("log").len();
	std::thread::sleep(Duration::from_millis(2500));
	assert_eq!(f.lines("log").len(), after);
}

#[test]
fn settings_without_displays_get_the_starters() {
	let starter = crate::settings::read_sources(&[crate::settings::Source::new("defaults", crate::settings::DEFAULTS)]).unwrap();
	let bare = crate::settings::read_sources(&[crate::settings::Source::new("old.toml", "schema = 3\n")]).unwrap();
	assert_eq!(starter.displays, DisplaysDef::default());
	assert_eq!(bare.displays, DisplaysDef::default());
	let p = bare.profile("default");
	assert_eq!((p.widget.as_str(), p.tiles.as_str(), p.card.as_str(), p.widget_minutes), ("", "", "", crate::settings::WIDGET_MINUTES));
}
