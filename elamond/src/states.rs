//! The stream's `data` lines (docs/protocol.md): the state of each widget of the dashboard on
//! screen, and of the header's, ready to draw — values in the client's units, levels judged,
//! strings filled in, table rows filtered, sorted and cut.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

use crate::format;
use crate::settings::{Bound, Labels, Widget};
use crate::ui::{Dashboard, Obj, Sources, UiWidget, J};
use crate::values::{self, Item, Level, Limits, Scalar, View, V};

/// What the states of a client's widgets are built from.
pub struct States<'a> {
	pub view: View<'a>,
	/// Whether clients may act now: action widgets are enabled.
	pub control: bool,
	/// The other order a client asked a table for (`POST /api/v1/stream/{id}`), by its id.
	pub sorts: &'a HashMap<String, String>,
	/// The costlier sources the reading was made with: a widget that needs another has no state
	/// yet, rather than one of what isn't read (a table of no processes).
	pub read: Sources,
}

/// Rows a table sends at most unless it says.
pub const LIMIT: u64 = 500;
pub const LIMIT_MAX: u64 = 10_000;

/// A `data` line: the states of `dashboard`'s widgets and of the header's (unless it is the
/// header), and `history` when given (the first line after opening, or after the dashboard
/// changed).
pub fn data(t: u64, dashboard: &Dashboard, header: Option<&Dashboard>, s: &States, history: Option<Obj>) -> Obj {
	let mut widgets = widgets(dashboard, s);
	if let Some(h) = header.filter(|h| h.id != dashboard.id) {
		widgets.extend(self::widgets(h, s));
	}
	let mut o = Obj::new();
	o.put("type", "data");
	o.put("t", t);
	o.put("dashboard", dashboard.id.as_str());
	o.put("widgets", widgets);
	o.opt("history", history);
	o
}

/// The state of each widget of `d` that has one, by id, those in groups included; none for a widget
/// whose sources the reading wasn't made with.
pub fn widgets(d: &Dashboard, s: &States) -> Obj {
	let mut out = Obj::new();
	for w in d.all_widgets() {
		if !s.read.covers(w.needs()) {
			continue;
		}
		if let Some(state) = state(w, s) {
			out.put(&w.id, state);
		}
	}
	out
}

/// A string with its values filled in as they are now.
fn fill(s: &States, text: &str) -> String {
	format::fill(text, &|a, f| s.view.text(a, f))
}

/// One widget's state; none for a group or a text without values.
pub fn state(w: &UiWidget, s: &States) -> Option<Obj> {
	let d = &w.def;
	let mut o = Obj::new();
	// A caption whose values are all missing fills to nothing: then there's none, as for a widget without.
	let caption = d.caption.as_deref().filter(|c| format::holds_values(c)).map(|c| fill(s, c)).filter(|c| !c.is_empty());
	match w.kind.as_str() {
		"value" | "gauge" | "meter" | "status" => value_state(w, s, &mut o),
		"graph" => {
			let series: Vec<Option<Scalar>> = d.values.iter().flatten().map(|a| s.view.get(a)).collect();
			o.put("v", J::Arr(series.iter().map(|x| x.as_ref().map_or(J::Null, |x| J::from(x.v.json()))).collect()));
			if let Some(Bound::Name(a)) = &d.max {
				o.put("max", s.view.get(a).map_or(J::Null, |x| J::from(x.v.json())));
			}
			if series.first().is_some_and(Option::is_none)
				&& let Some(e) = d.empty.as_deref().map(|e| fill(s, e)).filter(|e| !e.is_empty())
			{
				o.put("text", e);
			}
		}
		"bars" => bars_state(d, s, &mut o),
		"list" => {
			let items: Vec<J> = d.items.iter().flatten().map(|i| J::Obj(item_state(i, s))).collect();
			o.put("items", J::Arr(items));
		}
		"table" => {
			o.opt("caption", caption.clone());
			table_state(w, s, &mut o);
		}
		"button" => o.put("enabled", d.action.is_none() || s.control),
		"toggle" | "select" | "slider" => {
			o.put("v", d.value.as_deref().and_then(|a| s.view.get(a)).map_or(J::Null, |x| J::from(x.v.json())));
			o.put("enabled", s.control);
		}
		"text" => {
			if let Some(t) = d.text.as_deref().filter(|t| format::holds_values(t)) {
				o.put("text", fill(s, t));
				o.put("level", d.level.clone().unwrap_or_else(|| "normal".into()));
			}
		}
		"media" => media_state(s, &mut o),
		_ => {}
	}
	if w.kind != "table" {
		o.opt("caption", caption);
	}
	(!o.is_empty()).then_some(o)
}

/// The level of a value, and how far it has turned (0 left out): by `levels` when the widget maps
/// its values to levels, else by its limits (the widget's over the address's own). A number
/// without limits is `normal`; a duration or a moment, and a text, have none.
fn level_of(d_levels: Option<&BTreeMap<String, String>>, w: Option<&Widget>, format: &str, scalar: Option<&Scalar>, s: &States) -> Option<(Level, f64)> {
	let scalar = scalar?;
	if let Some(levels) = d_levels.filter(|l| !l.is_empty()) {
		return Some((levels.get(&scalar.v.map_key()).and_then(|l| Level::parse(l)).unwrap_or(Level::Normal), 0.0));
	}
	let x = scalar.v.num()?;
	let mut limits = scalar.limits.clone();
	if let Some(w) = w {
		let bound = |b: &Option<Bound>| match b {
			Some(Bound::Num(n)) => n.as_f64(),
			Some(Bound::Name(a)) => s.view.get(a).and_then(|v| v.v.num()),
			None => None,
		};
		let given = [bound(&w.warning_above), bound(&w.critical_above), bound(&w.warning_below), bound(&w.critical_below)];
		if given.iter().any(Option::is_some) || w.soft.is_some() {
			let mut l = limits.unwrap_or_default();
			l.warning_above = given[0].or(l.warning_above);
			l.critical_above = given[1].or(l.critical_above);
			l.warning_below = given[2].or(l.warning_below);
			l.critical_below = given[3].or(l.critical_below);
			l.soft = w.soft.unwrap_or(l.soft);
			limits = Some(l);
		}
	}
	match limits {
		Some(l) if l != Limits::default() => Some(l.judge(x)),
		_ if format == "duration" || format == "time" => None,
		_ => Some((Level::Normal, 0.0)),
	}
}

fn put_level(o: &mut Obj, level: Option<(Level, f64)>) {
	if let Some((l, blend)) = level {
		o.put("level", l.as_str());
		let blend = format::round(blend, 3);
		if blend > 0.0 {
			o.put("blend", blend);
		}
	}
}

/// What a missing value shows, or a mapped one says.
fn text_of(map: Option<&BTreeMap<String, String>>, empty: Option<&str>, scalar: Option<&Scalar>, s: &States) -> Option<String> {
	match scalar {
		Some(x) => map.and_then(|m| m.get(&x.v.map_key())).cloned(),
		None => empty.map(|e| fill(s, e)).filter(|e| !e.is_empty()),
	}
}

fn value_state(w: &UiWidget, s: &States, o: &mut Obj) {
	let d = &w.def;
	let scalar = d.value.as_deref().and_then(|a| s.view.get(a));
	o.put("v", scalar.as_ref().map_or(J::Null, |x| J::from(x.v.json())));
	o.opt("text", text_of(d.map.as_ref(), d.empty.as_deref(), scalar.as_ref(), s));
	let format = d.format.as_deref().or(scalar.as_ref().map(|x| x.format)).unwrap_or("number");
	put_level(o, level_of(d.levels.as_ref(), Some(d), format, scalar.as_ref(), s));
	// The full scale, when it is another value.
	let max = match &d.max {
		Some(Bound::Name(a)) => Some(a.clone()),
		None if matches!(w.kind.as_str(), "gauge" | "meter") || d.kind.as_deref().is_some_and(|k| k == "gauge" || k == "meter") => {
			scalar.as_ref().and_then(|x| match &x.max {
				Some(values::Max::Address(a)) => Some(a.clone()),
				_ => None,
			})
		}
		_ => None,
	};
	if let Some(a) = max {
		o.put("max", s.view.get(&a).map_or(J::Null, |x| J::from(x.v.json())));
	}
}

fn item_state(i: &crate::settings::ListItem, s: &States) -> Obj {
	let mut o = Obj::new();
	match (&i.value, &i.text) {
		(Some(a), _) => {
			let scalar = s.view.get(a);
			o.put("v", scalar.as_ref().map_or(J::Null, |x| J::from(x.v.json())));
			o.opt("text", text_of(i.map.as_ref(), i.empty.as_deref(), scalar.as_ref(), s));
			let format = i.format.as_deref().or(scalar.as_ref().map(|x| x.format)).unwrap_or("number");
			if i.map.as_ref().is_none_or(|m| m.is_empty()) {
				put_level(&mut o, level_of(None, None, format, scalar.as_ref(), s));
			}
		}
		(None, Some(t)) => o.put("text", fill(s, t)),
		_ => {}
	}
	o
}

fn bars_state(d: &Widget, s: &States, o: &mut Obj) {
	let source = d.source.as_deref().unwrap_or("");
	let field = d.field.as_deref().unwrap_or("");
	let items = s.view.collection(source).unwrap_or_default();
	o.put("v", J::Arr(items.iter().map(|i| i.get(field).map_or(J::Null, |v| J::from(v.json()))).collect()));
	if let Some(Labels::Field(f)) = &d.labels {
		let (_, unit, fmt) = values::field_def(source, f).unwrap_or(("", "", "text"));
		let labels = items.iter().map(|i| J::from(i.get(f).map(|v| format::format(v.shown(), fmt, unit)).unwrap_or_default())).collect();
		o.put("labels", J::Arr(labels));
	}
	let judged = values::collection_def(source).and_then(|c| c.level_field) == Some(field);
	let levels = items
		.iter()
		.map(|i| {
			let level = if judged { s.view.item_level(source, i).map_or(Level::Normal, |l| l.0) } else { Level::Normal };
			J::from(level.as_str())
		})
		.collect();
	o.put("levels", J::Arr(levels));
	if let Some(Bound::Name(a)) = &d.max {
		o.put("max", s.view.get(a).map_or(J::Null, |x| J::from(x.v.json())));
	}
}

/// Orders two values of a field: numbers by size, texts without regard to case; missing last.
fn compare(a: Option<&V>, b: Option<&V>) -> Ordering {
	match (a, b) {
		(None, None) => Ordering::Equal,
		(None, _) => Ordering::Greater,
		(_, None) => Ordering::Less,
		(Some(a), Some(b)) => match (a.num(), b.num()) {
			(Some(x), Some(y)) => x.total_cmp(&y),
			_ => a.map_key().to_lowercase().cmp(&b.map_key().to_lowercase()),
		},
	}
}

fn table_state(w: &UiWidget, s: &States, o: &mut Obj) {
	let d = &w.def;
	let source = d.source.as_deref().unwrap_or("");
	let mut items: Vec<Item> = s.view.collection(source).unwrap_or_default();
	if let Some(filter) = &d.filter {
		items.retain(|i| filter.iter().all(|(f, want)| i.get(f).is_some_and(|v| v.matches(want))));
	}
	let total = items.len();
	// Another of its sorts when the client asked for one (most first), else its own.
	let asked = s.sorts.get(&w.id).filter(|f| d.sorts.iter().flatten().any(|s| s == *f));
	let sort = match asked {
		Some(f) => Some((f.as_str(), true)),
		None => d.sort.as_ref().map(|s| (s.field.as_str(), s.desc)),
	};
	if let Some((field, desc)) = sort {
		items.sort_by(|a, b| {
			let (x, y) = (a.fields.get(field), b.fields.get(field));
			match (x.is_some(), y.is_some(), desc) {
				(true, true, true) => compare(y, x),
				_ => compare(x, y),
			}
		});
	}
	items.truncate(d.limit.unwrap_or(LIMIT).min(LIMIT_MAX) as usize);
	// Only the fields the columns, the opened row and its actions need.
	let mut fields: Vec<(&str, Option<&BTreeMap<String, String>>)> = Vec::new();
	fn need<'a>(fields: &mut Vec<(&'a str, Option<&'a BTreeMap<String, String>>)>, f: &'a str, map: Option<&'a BTreeMap<String, String>>) {
		if let Some(e) = fields.iter_mut().find(|(x, _)| *x == f) {
			e.1 = e.1.or(map);
		} else {
			fields.push((f, map));
		}
	}
	for c in d.columns.iter().flatten().chain(d.expand.iter().flatten()) {
		need(&mut fields, &c.field, c.map.as_ref());
		if let Some(crate::settings::Bar { max: Bound::Name(m) }) = &c.bar
			&& m != "top"
		{
			need(&mut fields, m, None);
		}
	}
	if let Some(f) = &d.row_open {
		need(&mut fields, f, None);
	}
	// The client searches these itself.
	let searched = crate::ui::search_fields(d);
	for f in &searched {
		need(&mut fields, f, None);
	}
	let placeholders: Vec<String> = d
		.row_actions
		.iter()
		.flatten()
		.flat_map(|a| a.params.iter().flatten())
		.filter_map(|(_, v)| v.as_str().and_then(|s| s.strip_prefix('{')).and_then(|s| s.strip_suffix('}')).map(str::to_string))
		.collect();
	for f in &placeholders {
		need(&mut fields, f, None);
	}
	let rows: Vec<J> = items
		.iter()
		.map(|i| {
			let mut row = Obj::new();
			row.put("key", i.key.as_str());
			put_level(&mut row, s.view.item_level(source, i));
			let mut f = Obj::new();
			for (field, map) in &fields {
				let Some(v) = i.get(field) else { continue };
				match map.and_then(|m| m.get(&v.map_key())) {
					Some(text) => f.put(field, text.as_str()),
					None => f.put(field, J::from(v.json())),
				}
			}
			row.put("f", f);
			J::Obj(row)
		})
		.collect();
	o.put("rows", J::Arr(rows));
	o.put("total", total as u64);
}

fn media_state(s: &States, o: &mut Obj) {
	let media = s.view.values.media.as_ref();
	let players: Vec<J> = media
		.map(|m| m.players.as_slice())
		.unwrap_or_default()
		.iter()
		.map(|p| {
			let mut q = Obj::new();
			q.put("id", p.id.as_str());
			q.put("name", p.name.as_str());
			q.put("status", p.status.as_str());
			q.opt("title", p.title.as_deref());
			q.opt("artist", p.artist.as_deref());
			q.opt("album", p.album.as_deref());
			q.opt("length", p.length.map(|us| format::round(us as f64 / 1e6, 1)));
			q.opt("position", p.position.map(|us| format::round(us as f64 / 1e6, 1)));
			q.opt("volume", p.volume.map(|v| format::round(v * 100.0, 1)));
			q.put("has_art", p.has_art);
			q.put("can_next", p.can_next);
			q.put("can_previous", p.can_previous);
			q.put("can_play", p.can_play);
			q.put("can_pause", p.can_pause);
			q.put("can_seek", p.can_seek);
			J::Obj(q)
		})
		.collect();
	o.put("players", J::Arr(players));
	if let Some(v) = media.and_then(|m| m.volume.as_ref()) {
		let mut vol = Obj::new();
		vol.put("level", format::round(v.level as f64 * 100.0, 1));
		vol.put("muted", v.muted);
		o.put("volume", vol);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::access::Role;
	use crate::actions::Caps;
	use crate::values::Units;

	#[test]
	fn tables_filter_sort_and_cut() {
		let config = crate::settings::read_sources(&[crate::settings::Source::new(
			"a.toml",
			"[profiles.default]\ndashboards = [\"t\"]\n[dashboards.t]\ntitle = \"T\"\n[[dashboards.t.widgets]]\nid = \"p\"\ntype = \"table\"\nsource = \"processes\"\nlimit = 1\nsort = { field = \"name\" }\nsorts = [\"mem\"]\nfilter = { status = \"Sleeping\" }\ncolumns = [{ field = \"name\" }, { field = \"cpu\", map = { \"3.5\" = \"some\" } }]\n",
		)])
		.unwrap();
		let mut values = crate::fixtures::values(&config);
		let mut other = values.collections["processes"][0].clone();
		other.key = "7".into();
		other.fields.insert("name", V::Text("Another".into()));
		other.fields.insert("mem", V::Int(1));
		values.collections.get_mut("processes").unwrap().push(other);
		let caps = Caps::all(&[]);
		let ui = crate::ui::build(&crate::ui::tests::input(&config, &values, &caps, Role::Admin, "default", None));
		let d = ui.dashboard("t").unwrap();
		let mut sorts = HashMap::new();
		let s = States { view: View::new(&values, Role::Admin, Units::default()), control: true, sorts: &sorts, read: Sources::ALL };
		let st = widgets(d, &s).get("t/p").unwrap().to_value();
		assert_eq!((st["rows"][0]["key"].as_str(), st["total"].as_u64()), (Some("7"), Some(2)));
		sorts.insert("t/p".to_string(), "mem".to_string());
		let s = States { view: View::new(&values, Role::Admin, Units::default()), control: true, sorts: &sorts, read: Sources::ALL };
		let st = widgets(d, &s).get("t/p").unwrap().to_value();
		assert_eq!(st["rows"][0]["key"].as_str(), Some("4242"));
		assert_eq!(st["rows"][0]["f"], serde_json::json!({ "name": "some", "cpu": "some" }));
		// A sort it doesn't offer isn't taken.
		sorts.insert("t/p".to_string(), "pid".to_string());
		let s = States { view: View::new(&values, Role::Admin, Units::default()), control: true, sorts: &sorts, read: Sources::ALL };
		assert_eq!(widgets(d, &s).get("t/p").unwrap().to_value()["rows"][0]["key"].as_str(), Some("7"));
	}

	#[test]
	fn levels_units_and_control_in_the_states() {
		let config = crate::settings::read_sources(&[crate::settings::Source::new(
			"a.toml",
			r#"
control = true
[profiles.hot]
use = "default"
fahrenheit = true
dashboards = ["t"]
[actions.go]
name = "Go"
command = ["true"]
[dashboards.t]
title = "T"
[[dashboards.t.widgets]]
id = "temp"
type = "gauge"
value = "cpu.temp"
warning_above = "temp.nvme_composite"
soft = 0
caption = "{cpu.temp:temp} · {gpu.amd.temp:temp}"
[[dashboards.t.widgets]]
id = "gone"
type = "value"
value = "gpu.amd.usage"
empty = "No such GPU"
caption = "{gpu.amd.temp:temp} · {gpu.amd.power:watts}"
[[dashboards.t.widgets]]
id = "graph"
type = "graph"
values = ["gpu.amd.usage", "cpu.temp"]
empty = "Asleep"
[[dashboards.t.widgets]]
id = "go"
type = "button"
action = "go"
[[dashboards.t.widgets]]
id = "uptime"
type = "value"
value = "system.uptime"
"#,
		)])
		.unwrap();
		let values = crate::fixtures::values(&config);
		let caps = Caps::all(&[]);
		let ui = crate::ui::build(&crate::ui::tests::input(&config, &values, &caps, Role::Admin, "hot", None));
		let d = ui.dashboard("t").unwrap();
		let sorts = HashMap::new();
		let state = |control: bool| {
			let s = States { view: View::new(&values, Role::Admin, ui.units), control, sorts: &sorts, read: Sources::ALL };
			widgets(d, &s).to_value_map()
		};
		let on = state(true);
		// 55.5 °C is 131.9 °F; the NVMe's 41.5 °C (106.7 °F) is where it turns warning, and it is
		// near critical's 95 °C (203 °F) by (131.9 - 106.7) / (203 - 106.7).
		assert_eq!(on["t/temp"]["v"], 131.9);
		assert_eq!(on["t/temp"]["level"], "warning");
		assert_eq!(on["t/temp"]["blend"], 0.262);
		assert_eq!(on["t/temp"]["caption"], "132°F");
		assert_eq!(on["t/gone"], serde_json::json!({ "v": null, "text": "No such GPU" }));
		assert_eq!(on["t/graph"], serde_json::json!({ "v": [null, 131.9], "text": "Asleep" }));
		assert_eq!(on["t/go"]["enabled"], true);
		// A duration has no level.
		assert_eq!(on["t/uptime"], serde_json::json!({ "v": 3600 }));
		let off = state(false);
		assert_eq!(off["t/go"]["enabled"], false);
		// The history in °F too.
		let mut ring = crate::history::Ring::new(10);
		ring.push_points(1000, [("cpu.temp".to_string(), Some(100.0))]);
		let h = crate::history::json(d, &ring, ui.units, 2000).to_value_map();
		assert_eq!(h["t/graph"], serde_json::json!({ "t": [1000], "v": [[null], [212.0]] }));
	}
}
