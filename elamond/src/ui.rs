//! `GET /api/v1/ui` (docs/protocol.md): what one client shows, built from the settings, the values
//! the computer has now, the client's role and profile, and the widget types it draws. The
//! dashboards come out resolved — templates applied, `for_each` spread, `when` decided, what the
//! role doesn't see left out, defaults written — both as the document the client gets and as
//! `Dashboard`s the stream's states are built from.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};
use serde_json::Value;

use crate::access::Role;
use crate::actions::{self, Caps, ParamKind, Spec};
use crate::format;
use crate::settings::{self, Bound, Config, ForEach, Labels, ListItem, Profile, Widget};
use crate::values::{self, Item, Units, Values, View};

// ---- JSON with its keys in the order written ------------------------------------------------

/// A JSON value whose objects keep their keys in the order they were put in, so the documents
/// read as the protocol shows them.
#[derive(Clone, Debug, PartialEq)]
pub enum J {
	Null,
	Bool(bool),
	Num(serde_json::Number),
	Str(String),
	Arr(Vec<J>),
	Obj(Obj),
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Obj(Vec<(String, J)>);

impl Obj {
	/// As a JSON value, for tests.
	#[cfg(test)]
	pub fn to_value_map(&self) -> Value {
		serde_json::to_value(self).unwrap_or_default()
	}

	pub fn new() -> Obj {
		Obj(Vec::new())
	}

	/// Sets `key`, in its place when it is there already, else at the end.
	pub fn put(&mut self, key: &str, v: impl Into<J>) {
		let v = v.into();
		match self.0.iter_mut().find(|(k, _)| k == key) {
			Some(e) => e.1 = v,
			None => self.0.push((key.to_string(), v)),
		}
	}

	pub fn opt<T: Into<J>>(&mut self, key: &str, v: Option<T>) {
		if let Some(v) = v {
			self.put(key, v);
		}
	}

	#[cfg(test)]
	pub fn get(&self, key: &str) -> Option<&J> {
		self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
	}

	pub fn remove(&mut self, key: &str) -> Option<J> {
		let i = self.0.iter().position(|(k, _)| k == key)?;
		Some(self.0.remove(i).1)
	}

	pub fn is_empty(&self) -> bool {
		self.0.is_empty()
	}

	#[cfg(test)]
	pub fn keys(&self) -> impl Iterator<Item = &str> {
		self.0.iter().map(|(k, _)| k.as_str())
	}

	/// Every key of `other` after these.
	pub fn extend(&mut self, other: Obj) {
		for (k, v) in other.0 {
			self.put(&k, v);
		}
	}
}

impl J {
	#[cfg(test)]
	pub fn to_value(&self) -> Value {
		serde_json::to_value(self).unwrap_or_default()
	}
}

impl Serialize for J {
	fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
		match self {
			J::Null => s.serialize_unit(),
			J::Bool(b) => s.serialize_bool(*b),
			J::Num(n) => n.serialize(s),
			J::Str(t) => s.serialize_str(t),
			J::Arr(a) => {
				let mut seq = s.serialize_seq(Some(a.len()))?;
				for e in a {
					seq.serialize_element(e)?;
				}
				seq.end()
			}
			J::Obj(o) => o.serialize(s),
		}
	}
}

impl Serialize for Obj {
	fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
		let mut map = s.serialize_map(Some(self.0.len()))?;
		for (k, v) in &self.0 {
			map.serialize_entry(k, v)?;
		}
		map.end()
	}
}

impl From<Value> for J {
	fn from(v: Value) -> J {
		match v {
			Value::Null => J::Null,
			Value::Bool(b) => J::Bool(b),
			Value::Number(n) => J::Num(n),
			Value::String(s) => J::Str(s),
			Value::Array(a) => J::Arr(a.into_iter().map(J::from).collect()),
			Value::Object(o) => J::Obj(Obj(o.into_iter().map(|(k, v)| (k, J::from(v))).collect())),
		}
	}
}

impl From<&Value> for J {
	fn from(v: &Value) -> J {
		J::from(v.clone())
	}
}

impl From<Obj> for J {
	fn from(o: Obj) -> J {
		J::Obj(o)
	}
}

impl From<Vec<J>> for J {
	fn from(a: Vec<J>) -> J {
		J::Arr(a)
	}
}

impl From<&str> for J {
	fn from(s: &str) -> J {
		J::Str(s.to_string())
	}
}

impl From<String> for J {
	fn from(s: String) -> J {
		J::Str(s)
	}
}

impl From<&String> for J {
	fn from(s: &String) -> J {
		J::Str(s.clone())
	}
}

impl From<bool> for J {
	fn from(b: bool) -> J {
		J::Bool(b)
	}
}

impl From<u64> for J {
	fn from(n: u64) -> J {
		J::Num(n.into())
	}
}

impl From<u8> for J {
	fn from(n: u8) -> J {
		J::Num(n.into())
	}
}

impl From<i64> for J {
	fn from(n: i64) -> J {
		J::Num(n.into())
	}
}

impl From<f64> for J {
	fn from(x: f64) -> J {
		serde_json::Number::from_f64(x).map_or(J::Null, J::Num)
	}
}

impl From<serde_json::Number> for J {
	fn from(n: serde_json::Number) -> J {
		J::Num(n)
	}
}

// ---- what a client is shown -----------------------------------------------------------------

/// The service, as `ui` names it.
#[derive(Clone, Debug, PartialEq)]
pub struct Service {
	pub id: String,
	pub name: String,
	pub version: String,
	pub interval_ms: u64,
	pub history_seconds: u64,
	/// What the computer can offer at all, sorted: only for paired clients, not in `hello`.
	pub features: Vec<&'static str>,
}

/// Whether clients may act, and why not.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Control {
	pub enabled: bool,
	/// `settings`, `held_off` or `settings_unreadable`.
	pub off_reason: Option<&'static str>,
}

/// Everything `ui` is built from.
pub struct Input<'a> {
	pub config: &'a Config,
	pub values: &'a Values,
	pub caps: &'a Caps,
	pub service: Service,
	pub control: Control,
	pub role: Role,
	/// The client's profile; one the settings don't have is `default_profile`.
	pub profile: &'a str,
	/// `android`, later `web`: which part of the profile it gets.
	pub kind: &'a str,
	/// The widget types it draws; `None` for all of them.
	pub draws: Option<&'a BTreeSet<String>>,
}

/// A widget as described: its id, its type (after what the client can't draw was replaced), its
/// keys as resolved, and the widgets in it.
#[derive(Clone, Debug, PartialEq)]
pub struct UiWidget {
	pub id: String,
	pub kind: String,
	pub span: u8,
	/// The settings' widget, its template applied and its item filled in; a list's items and a
	/// table's row actions as far as this client sees them.
	pub def: Widget,
	pub widgets: Vec<UiWidget>,
}

/// A dashboard as described.
#[derive(Clone, Debug, PartialEq)]
pub struct Dashboard {
	pub id: String,
	pub title: String,
	pub icon: String,
	pub widgets: Vec<UiWidget>,
}

impl Dashboard {
	/// Every widget, those in groups included, in order.
	pub fn all_widgets(&self) -> Vec<&UiWidget> {
		fn walk<'a>(ws: &'a [UiWidget], out: &mut Vec<&'a UiWidget>) {
			for w in ws {
				out.push(w);
				walk(&w.widgets, out);
			}
		}
		let mut out = Vec::new();
		walk(&self.widgets, &mut out);
		out
	}

	/// The costlier sources its widgets need read.
	pub fn needs(&self) -> Sources {
		self.all_widgets().into_iter().fold(Sources::default(), |n, w| n.union(w.needs()))
	}
}

impl UiWidget {
	/// The costlier sources this widget's own state needs read (not those of the widgets in it):
	/// the collection of a table or bars, and the addresses it shows, is judged by or holds in its
	/// strings.
	pub fn needs(&self) -> Sources {
		let mut n = Sources::default();
		let d = &self.def;
		if let Some(s) = &d.source {
			n.collection(s);
		}
		if self.kind == "media" {
			n.media = true;
			n.volume = true;
		}
		for a in addresses(d) {
			n.address(a);
		}
		let mut texts: Vec<&str> = [&d.label, &d.caption, &d.text, &d.empty].into_iter().flatten().map(String::as_str).collect();
		for i in d.items.iter().flatten() {
			texts.extend([&i.label, &i.text, &i.empty].into_iter().flatten().map(String::as_str));
			for a in [&i.value, &i.when].into_iter().flatten() {
				n.address(a);
			}
		}
		for t in texts {
			for p in format::placeholders(t) {
				n.address(format::split_placeholder(p.inner).0);
			}
		}
		if let Some(a) = &d.when {
			n.address(a);
		}
		n
	}
}

/// The costlier sources the collector reads only while a dashboard some client has on screen needs
/// them: the rest it reads every tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Sources {
	/// The process list, whose rates take two readings.
	pub processes: bool,
	/// MPRIS players.
	pub media: bool,
	/// The default output's volume, which costs running a command.
	pub volume: bool,
	/// NVML, which wakes a sleeping NVIDIA GPU and keeps it awake.
	pub gpus: bool,
	/// Monitors, from the DRM card nodes.
	pub displays: bool,
	/// Whether links answer, and the states of their units.
	pub links: bool,
}

impl Sources {
	/// Every source.
	pub const ALL: Sources = Sources { processes: true, media: true, volume: true, gpus: true, displays: true, links: true };

	/// What either needs.
	pub fn union(self, o: Sources) -> Sources {
		Sources {
			processes: self.processes || o.processes,
			media: self.media || o.media,
			volume: self.volume || o.volume,
			gpus: self.gpus || o.gpus,
			displays: self.displays || o.displays,
			links: self.links || o.links,
		}
	}

	/// Whether a reading made with these has everything `need` asks for.
	pub fn covers(self, need: Sources) -> bool {
		self.union(need) == self
	}

	fn collection(&mut self, name: &str) {
		match name {
			"processes" => self.processes = true,
			"gpus" => self.gpus = true,
			"displays" => self.displays = true,
			"links" => self.links = true,
			_ => {}
		}
	}

	fn address(&mut self, a: &str) {
		let head = a.split('.').next().unwrap_or("");
		match head {
			"gpu" => self.gpus = true,
			"displays" => self.displays = true,
			"volume" => self.volume = true,
			"link" => self.links = true,
			_ => {}
		}
	}

	/// The words for the journal: "processes, media".
	pub fn words(self) -> Vec<&'static str> {
		[
			(self.processes, "processes"),
			(self.media, "media"),
			(self.volume, "volume"),
			(self.gpus, "GPUs"),
			(self.displays, "monitors"),
			(self.links, "links"),
		]
		.into_iter()
		.filter_map(|(on, w)| on.then_some(w))
		.collect()
	}
}

/// What `GET /api/v1/ui` answers, and what the stream needs of it.
#[derive(Clone, Debug)]
pub struct Ui {
	/// The document, `version` included.
	pub doc: J,
	pub version: String,
	pub profile: Profile,
	pub units: Units,
	pub tabs: Vec<String>,
	pub header: Option<String>,
	/// The tabs, the header, then the dashboards reached through `open`.
	pub dashboards: Vec<Dashboard>,
	/// The actions this client may run.
	pub actions: Vec<Spec>,
}

impl Ui {
	pub fn dashboard(&self, id: &str) -> Option<&Dashboard> {
		self.dashboards.iter().find(|d| d.id == id)
	}

	/// The document as JSON text.
	pub fn json(&self) -> String {
		serde_json::to_string(&self.doc).unwrap_or_default()
	}
}

/// A client's units, from its profile.
pub fn units_of(p: &Profile) -> Units {
	Units { fahrenheit: p.fahrenheit, bits: p.network_bits }
}

/// What resolving dashboards needs.
struct Ctx<'a> {
	config: &'a Config,
	view: View<'a>,
	/// The actions the client may run, by id.
	actions: BTreeMap<String, Spec>,
	draws: Option<&'a BTreeSet<String>>,
	history_seconds: u64,
}

/// What `{item}` stands for in a widget made by `for_each`: an item of a collection, or a string.
#[derive(Clone, Debug)]
enum ItemOf {
	Item(&'static str, Item),
	Str(String),
}

impl ItemOf {
	/// What a widget's id ends with.
	fn key(&self) -> String {
		match self {
			ItemOf::Item(_, i) => i.key.clone(),
			ItemOf::Str(s) => values::key_of(s),
		}
	}
}

impl Ctx<'_> {
	fn draws(&self, kind: &str) -> bool {
		self.draws.is_none_or(|d| d.contains(kind))
	}

	/// A string with its values filled in as they are now.
	fn fill(&self, text: &str) -> String {
		format::fill(text, &|a, f| self.view.text(a, f))
	}

	fn items_of(&self, fe: &ForEach) -> Vec<ItemOf> {
		match fe {
			ForEach::List(l) => l.iter().map(|s| ItemOf::Str(s.clone())).collect(),
			ForEach::Collection(name) => {
				let Some(def) = values::collection_def(name) else { return Vec::new() };
				self.view.collection(name).unwrap_or_default().into_iter().map(|i| ItemOf::Item(def.name, i)).collect()
			}
		}
	}

	/// `{item}` and `{item.<field>}` in every string of `v` filled in; a part with a `for_each` of
	/// its own is left for that one.
	fn fill_item(&self, v: &mut Value, item: &ItemOf, top: bool) {
		match v {
			Value::String(s) => *s = self.item_text(s, item),
			Value::Array(a) => a.iter_mut().for_each(|e| self.fill_item(e, item, false)),
			Value::Object(o) => {
				if !top && o.contains_key("for_each") {
					return;
				}
				o.values_mut().for_each(|e| self.fill_item(e, item, false));
			}
			_ => {}
		}
	}

	fn item_text(&self, s: &str, item: &ItemOf) -> String {
		let mut out = String::new();
		let mut rest = s;
		while let Some(at) = rest.find("{item") {
			out.push_str(&rest[..at]);
			let after = &rest[at + 5..];
			if let Some(tail) = after.strip_prefix('}') {
				out.push_str(&match item {
					ItemOf::Item(_, i) => i.key.clone(),
					ItemOf::Str(s) => s.clone(),
				});
				rest = tail;
			} else if let Some((field, tail)) = after.strip_prefix('.').and_then(|f| f.split_once('}')) {
				if let ItemOf::Item(collection, i) = item
					&& let Some(v) = i.get(field)
				{
					let (_, unit, fmt) = values::field_def(collection, field).unwrap_or(("", "", "text"));
					let unit = if unit.is_empty() { "" } else { self.view.field_unit(collection, field) };
					out.push_str(&format::format(v.shown(), fmt, unit));
				}
				rest = tail;
			} else {
				out.push_str("{item");
				rest = after;
			}
		}
		out.push_str(rest);
		out
	}

	/// The dashboard `id` names, as this client sees it, if it sees it.
	fn dashboard(&self, id: &str) -> Option<Dashboard> {
		let (base, d) = self.config.dashboard_of(id)?;
		if self.view.role < d.role.unwrap_or(Role::Glance) {
			return None;
		}
		let item = match &d.for_each {
			Some(name) => {
				let key = id.strip_prefix(base)?.strip_prefix('.')?;
				let def = values::collection_def(name)?;
				let item = self.view.collection(name)?.into_iter().find(|i| i.key == key)?;
				Some(ItemOf::Item(def.name, item))
			}
			None if id != base => return None,
			None => None,
		};
		let mut title = Value::String(d.title.clone());
		if let Some(it) = &item {
			self.fill_item(&mut title, it, true);
		}
		let widgets = self.widgets(&d.widgets, id, item.as_ref(), None);
		if !d.widgets.is_empty() && widgets.is_empty() {
			return None;
		}
		Some(Dashboard {
			id: id.to_string(),
			title: self.fill(title.as_str().unwrap_or("")),
			icon: d.icon.clone().unwrap_or_else(|| "overview".into()),
			widgets,
		})
	}

	/// The dashboards a profile's tab names: itself, or one for each item of its `for_each`.
	fn expand(&self, id: &str) -> Vec<String> {
		match self.config.dashboards.get(id).and_then(|d| d.for_each.as_deref()) {
			Some(name) => self.view.collection(name).unwrap_or_default().iter().map(|i| format!("{id}.{}", i.key)).collect(),
			None => vec![id.to_string()],
		}
	}

	/// A list of widgets under `prefix`, `item` filled in; with `suffix`, each id ends with it.
	fn widgets(&self, defs: &[settings::WidgetDef], prefix: &str, item: Option<&ItemOf>, suffix: Option<&str>) -> Vec<UiWidget> {
		let mut out = Vec::new();
		for (i, raw) in defs.iter().enumerate() {
			let merged = settings::apply_template(raw, &self.config.templates).unwrap_or_else(|_| raw.clone());
			let mut v = Value::Object(merged);
			if let Some(it) = item {
				self.fill_item(&mut v, it, false);
			}
			let id = v.get("id").and_then(Value::as_str).map_or_else(|| i.to_string(), str::to_string);
			let id = match suffix {
				Some(s) => format!("{id}.{s}"),
				None => id,
			};
			let fe: Option<ForEach> = v.get("for_each").and_then(|f| serde_json::from_value(f.clone()).ok());
			let is_group = v.get("type").and_then(Value::as_str) == Some("group");
			match fe {
				// A group's for_each repeats what is in it, inside the one card.
				Some(fe) if is_group => {
					let full = format!("{prefix}/{id}");
					let inner: Vec<settings::WidgetDef> =
						v.get("widgets").and_then(Value::as_array).map(|a| a.iter().filter_map(|w| w.as_object().cloned()).collect()).unwrap_or_default();
					let mut children = Vec::new();
					for it in self.items_of(&fe) {
						children.extend(self.widgets(&inner, &full, Some(&it), Some(&it.key())));
					}
					if let Some(w) = self.widget(v, prefix, &id, Some(children)) {
						out.push(w);
					}
				}
				Some(fe) => {
					for it in self.items_of(&fe) {
						let mut each = v.clone();
						if let Some(o) = each.as_object_mut() {
							o.remove("for_each");
						}
						self.fill_item(&mut each, &it, true);
						if let Some(w) = self.widget(each, prefix, &format!("{id}.{}", it.key()), None) {
							out.push(w);
						}
					}
				}
				None => {
					if let Some(w) = self.widget(v, prefix, &id, None) {
						out.push(w);
					}
				}
			}
		}
		// A group the client doesn't draw gives way to what is in it.
		out.into_iter()
			.flat_map(|w| if w.kind == "group" && !self.draws("group") { w.widgets } else { vec![w] })
			.collect()
	}

	/// One widget, as this client sees it, if it sees it.
	fn widget(&self, v: Value, prefix: &str, id: &str, children: Option<Vec<UiWidget>>) -> Option<UiWidget> {
		let mut w: Widget = serde_json::from_value(v).ok()?;
		let kind = w.kind.clone()?;
		if w.when.as_deref().is_some_and(|a| self.view.get(a).is_none()) {
			return None;
		}
		// The least role its values, collection and action need, or its own.
		let mut need = w.role.unwrap_or(Role::Glance);
		for a in addresses(&w) {
			need = need.max(values::address_role(a));
		}
		if let Some(s) = &w.source {
			need = need.max(values::collection_role(s));
		}
		if kind == "media" {
			need = need.max(Role::Monitor);
		}
		if self.view.role < need {
			return None;
		}
		if w.action.as_deref().is_some_and(|a| !self.actions.contains_key(a)) {
			return None;
		}
		let full = format!("{prefix}/{id}");
		let span = w.span.unwrap_or(if settings::WIDE.contains(&kind.as_str()) { 4 } else { 2 });
		let mut widgets = Vec::new();
		match kind.as_str() {
			"group" => {
				widgets = match children {
					Some(c) => c,
					None => self.widgets(w.widgets.as_deref().unwrap_or_default(), &full, None, None),
				};
				if widgets.is_empty() {
					return None;
				}
			}
			"list" => {
				let items = self.list_items(w.items.as_deref().unwrap_or_default());
				if items.is_empty() {
					return None;
				}
				w.items = Some(items);
			}
			"table" => {
				if let Some(ra) = &mut w.row_actions {
					ra.retain(|a| self.actions.contains_key(&a.action));
				}
			}
			_ => {}
		}
		Some(self.substitute(UiWidget { id: full, kind, span, def: w, widgets }))
	}

	/// A list's rows this client sees now: `for_each` spread, `when` decided, and a row with
	/// nothing to show left out.
	fn list_items(&self, items: &[ListItem]) -> Vec<ListItem> {
		let mut out = Vec::new();
		for item in items {
			let each: Vec<ListItem> = match &item.for_each {
				None => vec![item.clone()],
				Some(fe) => self
					.items_of(fe)
					.into_iter()
					.filter_map(|it| {
						let mut v = serde_json::to_value(ListItemOut(item)).ok()?;
						if let Some(o) = v.as_object_mut() {
							o.remove("for_each");
						}
						self.fill_item(&mut v, &it, true);
						serde_json::from_value(v).ok()
					})
					.collect(),
			};
			for i in each {
				if i.when.as_deref().is_some_and(|a| self.view.get(a).is_none()) {
					continue;
				}
				let shows = match (&i.value, &i.text) {
					(Some(a), _) => {
						self.view.role >= values::address_role(a) && (self.view.get(a).is_some() || i.empty.as_deref().is_some_and(|e| !self.fill(e).is_empty()))
					}
					(None, Some(t)) => !self.fill(t).is_empty(),
					_ => false,
				};
				if shows {
					out.push(i);
				}
			}
		}
		out
	}

	/// A widget the client doesn't draw, as the nearest it does, or a word that the app needs
	/// updating.
	fn substitute(&self, mut w: UiWidget) -> UiWidget {
		// A group the client doesn't draw is spread out where it stands (`widgets`).
		if self.draws(&w.kind) || w.kind == "group" {
			return w;
		}
		match w.kind.as_str() {
			"gauge" | "meter" | "status" if self.draws("value") => {
				w.kind = "value".into();
				return w;
			}
			"slider" if self.draws("select") => {
				w.kind = "select".into();
				return w;
			}
			"select" | "slider" if self.draws("group") && self.draws("button") => {
				if let Some(g) = self.buttons(&w) {
					return g;
				}
			}
			_ => {}
		}
		let label = w.def.label.clone();
		let text = format!("Update the app to see this {} widget.", w.kind);
		let def = Widget { kind: Some("text".into()), label, text: Some(text), level: Some("warning".into()), ..Default::default() };
		UiWidget { id: w.id, kind: "text".into(), span: w.span, def, widgets: Vec::new() }
	}

	/// A select or slider as a group of buttons, one for each of its values (at most 11 of a range).
	fn buttons(&self, w: &UiWidget) -> Option<UiWidget> {
		let action = w.def.action.as_deref()?;
		let param = w.def.param.as_deref()?;
		let spec = self.actions.get(action)?;
		let p = spec.params.iter().find(|p| p.id == param)?;
		let choices: Vec<(Value, String)> = match &p.kind {
			ParamKind::Choice(c) => c.iter().map(|(v, l)| (Value::String(v.clone()), l.clone())).collect(),
			ParamKind::Range { min, max, step, unit } => {
				let (lo, hi, st) = (min.as_f64()?, max.as_f64()?, step.as_f64()?.max(f64::EPSILON));
				let steps = ((hi - lo) / st).floor() as u64;
				let picks: Vec<u64> = if steps <= 10 { (0..=steps).collect() } else { (0..=10).map(|i| (i * steps + 5) / 10).collect() };
				let whole = min.is_i64() && step.is_i64();
				picks
					.into_iter()
					.map(|n| {
						let x = lo + n as f64 * st;
						let v = if whole { Value::from(x.round() as i64) } else { values::num(x) };
						(v, format::format(format::Shown::Num(x), "number", unit.as_deref().unwrap_or("")))
					})
					.collect()
			}
			ParamKind::Bool => vec![(Value::Bool(true), "On".into()), (Value::Bool(false), "Off".into())],
			_ => return None,
		};
		let buttons = choices
			.into_iter()
			.map(|(v, label)| {
				let mut params = serde_json::Map::new();
				params.insert(param.to_string(), v.clone());
				let key = values::key_of(&match &v {
					Value::String(s) => s.clone(),
					other => other.to_string(),
				});
				let def = Widget { kind: Some("button".into()), label: Some(label), action: Some(action.into()), params: Some(params), ..Default::default() };
				UiWidget { id: format!("{}/{key}", w.id), kind: "button".into(), span: 1, def, widgets: Vec::new() }
			})
			.collect();
		let def = Widget {
			kind: Some("group".into()),
			label: w.def.label.clone(),
			icon: w.def.icon.clone(),
			color: w.def.color.clone(),
			caption: w.def.caption.clone(),
			..Default::default()
		};
		Some(UiWidget { id: w.id.clone(), kind: "group".into(), span: w.span, def, widgets: buttons })
	}
}

/// A list's row as the settings write it, to fill its item in.
struct ListItemOut<'a>(&'a ListItem);

impl Serialize for ListItemOut<'_> {
	fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
		let i = self.0;
		let mut m = s.serialize_map(None)?;
		for (k, v) in [("label", &i.label), ("value", &i.value), ("text", &i.text), ("format", &i.format), ("unit", &i.unit), ("empty", &i.empty), ("when", &i.when)] {
			if let Some(v) = v {
				m.serialize_entry(k, v)?;
			}
		}
		if let Some(map) = &i.map {
			m.serialize_entry("map", map)?;
		}
		m.end()
	}
}

/// The addresses a widget shows or is judged by.
pub fn addresses(w: &Widget) -> Vec<&str> {
	let mut out: Vec<&str> = Vec::new();
	out.extend(w.value.as_deref());
	out.extend(w.values.iter().flatten().map(String::as_str));
	for b in [&w.warning_above, &w.critical_above, &w.warning_below, &w.critical_below, &w.min, &w.max].into_iter().flatten() {
		if let Bound::Name(a) = b {
			out.push(a);
		}
	}
	out
}

/// The fields a table's search looks in: those it names, or with `search = true` its text columns
/// and the text fields it shows when a row opens, each once; none without a search.
pub fn search_fields(d: &Widget) -> Vec<String> {
	let source = d.source.as_deref().unwrap_or("");
	let mut out: Vec<String> = Vec::new();
	match &d.search {
		Some(settings::Search::Fields(f)) => out.extend(f.iter().cloned()),
		Some(settings::Search::On(true)) => {
			for c in d.columns.iter().flatten().chain(d.expand.iter().flatten()) {
				let mapped = c.map.as_ref().is_some_and(|m| !m.is_empty());
				let format = if mapped {
					"text".to_string()
				} else {
					c.format.clone().unwrap_or_else(|| values::field_def(source, &c.field).map_or("text", |f| f.2).to_string())
				};
				if format == "text" {
					out.push(c.field.clone());
				}
			}
		}
		_ => {}
	}
	let mut seen = HashSet::new();
	out.retain(|f| seen.insert(f.clone()));
	out
}

/// Drops `open` to a dashboard the client doesn't get, and a button left with nothing to do.
fn prune(widgets: &mut Vec<UiWidget>, visible: &HashSet<String>) {
	for w in widgets.iter_mut() {
		if w.def.open.as_ref().is_some_and(|o| !visible.contains(o)) {
			w.def.open = None;
		}
		prune(&mut w.widgets, visible);
	}
	widgets.retain(|w| !(w.kind == "button" && w.def.action.is_none() && w.def.open.is_none()) && !(w.kind == "group" && w.widgets.is_empty()));
}

/// Every `open` of these widgets, in order.
fn opens(widgets: &[UiWidget], out: &mut Vec<String>) {
	for w in widgets {
		out.extend(w.def.open.clone());
		opens(&w.widgets, out);
	}
}

/// Builds `ui` for one client.
pub fn build(input: &Input) -> Ui {
	let profile = input.config.profile(input.profile);
	let units = units_of(&profile);
	let view = View::new(input.values, input.role, units);
	let available: Vec<Spec> = actions::available(input.config, input.caps).into_iter().filter(|s| input.role >= s.role).collect();
	let ctx = Ctx {
		config: input.config,
		view,
		actions: available.iter().map(|s| (s.id.clone(), s.clone())).collect(),
		draws: input.draws,
		history_seconds: input.service.history_seconds,
	};

	// The tabs, the header, then what their widgets open, each once.
	let tab_ids: Vec<String> = profile.dashboards.iter().flat_map(|d| ctx.expand(d)).collect();
	let header_id = Some(profile.header.clone()).filter(|h| !h.is_empty());
	let mut queue: VecDeque<String> = tab_ids.iter().cloned().chain(header_id.clone()).collect();
	let mut seen = HashSet::new();
	let mut dashboards: Vec<Dashboard> = Vec::new();
	while let Some(id) = queue.pop_front() {
		if !seen.insert(id.clone()) {
			continue;
		}
		if let Some(d) = ctx.dashboard(&id) {
			let mut more = Vec::new();
			opens(&d.widgets, &mut more);
			queue.extend(more);
			dashboards.push(d);
		}
	}
	let visible: HashSet<String> = dashboards.iter().map(|d| d.id.clone()).collect();
	for d in &mut dashboards {
		prune(&mut d.widgets, &visible);
	}
	let tabs: Vec<String> = tab_ids.into_iter().filter(|t| visible.contains(t)).collect();
	let header = header_id.filter(|h| visible.contains(h));

	let mut doc = Obj::new();
	doc.put("service", service_json(&input.service));
	doc.put("role", input.role.as_str());
	let mut control = Obj::new();
	control.put("enabled", input.control.enabled);
	control.put("off_reason", input.control.off_reason.map_or(J::Null, J::from));
	doc.put("control", control);
	doc.put("profile", profile_json(&profile, input.kind, input.service.interval_ms));
	doc.put("tabs", J::Arr(tabs.iter().map(J::from).collect()));
	doc.put("header", header.as_ref().map_or(J::Null, J::from));
	doc.put("dashboards", J::Arr(dashboards.iter().map(|d| J::Obj(ctx.dashboard_json(d))).collect()));
	doc.put("actions", J::Arr(available.iter().map(|s| J::Obj(action_json(s, &profile))).collect()));
	let version = version_of(&doc);
	let mut full = Obj::new();
	full.put("version", version.clone());
	full.extend(doc);
	Ui { doc: J::Obj(full), version, profile, units, tabs, header, dashboards, actions: available }
}

/// A short hash of the document, which changes whenever anything in it does.
pub fn version_of(doc: &Obj) -> String {
	let text = serde_json::to_string(doc).unwrap_or_default();
	crate::store::sha256_hex(text.as_bytes())[..16].to_string()
}

/// Every dashboard the settings have, as an admin sees them with every action there is: what the
/// history keeps values for.
pub fn all_dashboards(config: &Config, values: &Values) -> Vec<Dashboard> {
	let caps = Caps::all(&values.power_profiles);
	let ctx = Ctx {
		config,
		view: View::new(values, Role::Admin, Units::default()),
		actions: actions::available(config, &caps).into_iter().map(|s| (s.id.clone(), s)).collect(),
		draws: None,
		history_seconds: u64::MAX,
	};
	config.dashboards.keys().flat_map(|id| ctx.expand(id)).filter_map(|id| ctx.dashboard(&id)).collect()
}

/// The addresses whose history some graph or spark of the settings shows: what the history ring
/// keeps.
pub fn tracked(config: &Config, values: &Values) -> BTreeSet<String> {
	let mut out = BTreeSet::new();
	for d in all_dashboards(config, values) {
		for w in d.all_widgets() {
			match w.kind.as_str() {
				"graph" => out.extend(w.def.values.iter().flatten().cloned()),
				"value" if w.def.spark == Some(true) => out.extend(w.def.value.clone()),
				_ => {}
			}
		}
	}
	out
}

fn service_json(s: &Service) -> Obj {
	let mut o = Obj::new();
	o.put("id", s.id.as_str());
	o.put("name", s.name.as_str());
	o.put("version", s.version.as_str());
	o.put("interval_ms", s.interval_ms);
	o.put("history_seconds", s.history_seconds);
	o.put("features", J::Arr(s.features.iter().map(|f| J::from(*f)).collect()));
	o
}

/// The profile's part every client reads, and the part for `kind`.
fn profile_json(p: &Profile, kind: &str, tick_ms: u64) -> Obj {
	let mut o = Obj::new();
	o.put("id", p.id.as_str());
	o.put("name", p.name.as_str());
	o.put("theme", p.theme.as_str());
	o.put("pure_black", p.pure_black);
	o.put("animations", p.animations);
	o.put("interval_ms", p.interval_ms.max(tick_ms));
	o.put("fahrenheit", p.fahrenheit);
	o.put("network_bits", p.network_bits);
	o.put("confirm_actions", p.confirm_actions);
	let mut unlock = Obj::new();
	unlock.put("for", J::Arr(p.unlock_for.iter().map(J::from).collect()));
	unlock.put("grace", p.unlock_grace.as_str());
	o.put("unlock", unlock);
	match kind {
		"web" => o.put("web", J::from(Value::Object(p.web.clone()))),
		_ => {
			let mut a = Obj::new();
			for (k, _) in settings::ANDROID {
				a.opt(k, p.android.get(k).copied());
			}
			for (k, v) in &p.android {
				a.put(k, *v);
			}
			o.put("android", a);
		}
	}
	o
}

/// A parameter as `ui` describes it.
fn param_json(p: &actions::Param) -> Obj {
	let mut o = Obj::new();
	o.put("id", p.id.as_str());
	o.put("kind", p.kind.name());
	o.put("label", p.label.as_str());
	match &p.kind {
		ParamKind::Choice(c) => {
			let choices = c
				.iter()
				.map(|(v, l)| {
					let mut ch = Obj::new();
					ch.put("value", v.as_str());
					ch.put("label", l.as_str());
					J::Obj(ch)
				})
				.collect();
			o.put("choices", J::Arr(choices));
		}
		ParamKind::Range { min, max, step, unit } => {
			o.put("min", min.clone());
			o.put("max", max.clone());
			o.put("step", step.clone());
			o.opt("unit", unit.as_deref());
		}
		ParamKind::Number { unit } => o.opt("unit", unit.as_deref()),
		_ => {}
	}
	if p.optional {
		o.put("optional", true);
	}
	o.opt("default", p.default.as_ref().map(J::from));
	o
}

/// An action as `ui` lists it: `confirm` and `unlock` final, the profile applied.
fn action_json(s: &Spec, p: &Profile) -> Obj {
	let mut o = Obj::new();
	o.put("id", s.id.as_str());
	o.put("name", s.name.as_str());
	o.put("kind", s.kind);
	o.put("icon", s.icon.as_str());
	o.put("confirm", s.confirm && p.confirm_actions);
	o.put("unlock", s.unlock || p.unlock_for.iter().any(|k| k == s.kind));
	o.put("params", J::Arr(s.params.iter().map(|p| J::Obj(param_json(p))).collect()));
	o
}

fn bound_num(b: &Option<Bound>) -> Option<J> {
	match b {
		Some(Bound::Num(n)) => Some(J::Num(n.clone())),
		_ => None,
	}
}

impl Ctx<'_> {
	fn dashboard_json(&self, d: &Dashboard) -> Obj {
		let mut o = Obj::new();
		o.put("id", d.id.as_str());
		o.put("title", d.title.as_str());
		o.put("icon", d.icon.as_str());
		o.put("widgets", J::Arr(d.widgets.iter().map(|w| J::Obj(self.widget_json(w))).collect()));
		o
	}

	/// The unit a value is shown in: the widget's, or its address's for this client.
	fn unit_of(&self, given: &Option<String>, address: Option<&str>) -> Option<String> {
		given.clone().or_else(|| address.map(|a| self.view.unit_of(a).to_string())).filter(|u| !u.is_empty())
	}

	/// The format a value is shown in: the widget's, or its address's.
	fn format_of(&self, given: &Option<String>, address: Option<&str>, otherwise: &str) -> String {
		given.clone().or_else(|| address.and_then(values::address_def).map(|d| d.format.to_string())).unwrap_or_else(|| otherwise.to_string())
	}

	fn widget_json(&self, w: &UiWidget) -> Obj {
		let d = &w.def;
		let mut o = Obj::new();
		o.put("id", w.id.as_str());
		o.put("type", w.kind.as_str());
		o.put("span", w.span);
		o.opt("label", d.label.as_deref().map(|l| self.fill(l)).filter(|l| !l.is_empty()));
		o.opt("icon", d.icon.as_deref());
		o.opt("color", d.color.as_deref());
		o.opt("open", d.open.as_deref());
		match d.caption.as_deref() {
			Some(c) if format::holds_values(c) => o.put("caption", true),
			Some(c) if !c.is_empty() => o.put("caption", c),
			_ => {}
		}
		match w.kind.as_str() {
			"group" => o.put("collapsed", d.collapsed.unwrap_or(false)),
			"table" => o.opt("collapsed", d.collapsed),
			_ => {}
		}
		let value = d.value.as_deref();
		match w.kind.as_str() {
			"value" | "gauge" | "meter" | "status" => {
				o.opt("unit", self.unit_of(&d.unit, value));
				o.put("format", self.format_of(&d.format, value, if w.kind == "status" { "text" } else { "number" }));
				if w.kind == "gauge" || w.kind == "meter" {
					o.put("min", bound_num(&d.min).unwrap_or(J::from(0u64)));
					let own = value.and_then(|a| self.view.values.scalars.get(a)).and_then(|s| match &s.max {
						Some(values::Max::Num(n)) => Some(J::Num(n.clone())),
						_ => None,
					});
					match &d.max {
						Some(Bound::Num(n)) => o.put("max", J::Num(n.clone())),
						Some(Bound::Name(_)) => {}
						None => o.opt("max", own.or_else(|| value.and_then(values::address_def).filter(|a| a.unit == "%").map(|_| J::from(100u64)))),
					}
				} else {
					o.opt("min", bound_num(&d.min));
					o.opt("max", bound_num(&d.max));
				}
				if w.kind == "value" && d.spark == Some(true) {
					o.put("spark", true);
				}
			}
			"graph" => {
				let labels = match &d.labels {
					Some(Labels::List(l)) => l.clone(),
					_ => Vec::new(),
				};
				let series: Vec<J> = d
					.values
					.iter()
					.flatten()
					.enumerate()
					.map(|(i, a)| {
						let mut s = Obj::new();
						s.opt("label", labels.get(i).map(|l| self.fill(l)));
						s.opt("color", d.colors.as_ref().and_then(|c| c.get(i)).cloned());
						s.opt("unit", self.unit_of(&d.unit, Some(a)));
						s.put("format", self.format_of(&d.format, Some(a), "number"));
						J::Obj(s)
					})
					.collect();
				o.put("series", J::Arr(series));
				o.put("range", d.range.unwrap_or(self.history_seconds).min(self.history_seconds));
				o.put("height", d.height.clone().unwrap_or_else(|| "medium".into()));
				o.opt("min", bound_num(&d.min));
				o.opt("max", bound_num(&d.max));
				o.opt("floor", d.floor.clone());
			}
			"list" => {
				let items: Vec<J> = d
					.items
					.iter()
					.flatten()
					.map(|i| {
						let mut it = Obj::new();
						it.opt("label", i.label.as_deref().map(|l| self.fill(l)));
						let mapped = i.map.as_ref().is_some_and(|m| !m.is_empty());
						match (&i.value, mapped) {
							(Some(a), false) => {
								it.opt("unit", self.unit_of(&i.unit, Some(a)));
								it.put("format", self.format_of(&i.format, Some(a), "number"));
							}
							_ => it.put("format", i.format.clone().unwrap_or_else(|| "text".into())),
						}
						J::Obj(it)
					})
					.collect();
				o.put("items", J::Arr(items));
			}
			"bars" => {
				let (source, field) = (d.source.as_deref().unwrap_or(""), d.field.as_deref().unwrap_or(""));
				let (_, unit, fmt) = values::field_def(source, field).unwrap_or(("", "", "number"));
				let unit = if unit.is_empty() { "" } else { self.view.field_unit(source, field) };
				o.opt("unit", d.unit.clone().or_else(|| Some(unit.to_string())).filter(|u| !u.is_empty()));
				o.put("format", d.format.clone().unwrap_or_else(|| fmt.to_string()));
				o.opt("max", bound_num(&d.max));
			}
			"table" => self.table_json(d, &mut o),
			"button" => {
				o.opt("action", d.action.as_deref());
				o.opt("params", d.params.clone().map(|p| J::from(Value::Object(p))));
			}
			"toggle" => {
				o.opt("action", d.action.as_deref());
				o.opt("param", d.param.as_deref());
				o.put("on", d.on.as_ref().map_or(J::Bool(true), J::from));
				o.put("off", d.off.as_ref().map_or(J::Bool(false), J::from));
			}
			"select" | "slider" => {
				o.opt("action", d.action.as_deref());
				o.opt("param", d.param.as_deref());
			}
			"text" => {
				if let Some(t) = d.text.as_deref().filter(|t| !format::holds_values(t)) {
					o.put("text", t);
				}
				o.put("level", d.level.clone().unwrap_or_else(|| "normal".into()));
			}
			"group" => {
				o.put("widgets", J::Arr(w.widgets.iter().map(|c| J::Obj(self.widget_json(c))).collect()));
			}
			_ => {}
		}
		o
	}

	fn column_json(&self, source: &str, c: &settings::Column, column: bool) -> J {
		let mut o = Obj::new();
		o.put("field", c.field.as_str());
		o.opt("label", c.label.as_deref());
		let mapped = c.map.as_ref().is_some_and(|m| !m.is_empty());
		let (_, unit, fmt) = values::field_def(source, &c.field).unwrap_or(("", "", "text"));
		if !mapped {
			let unit = c.unit.clone().or_else(|| (!unit.is_empty()).then(|| self.view.field_unit(source, &c.field).to_string()));
			o.opt("unit", unit);
		}
		o.put("format", c.format.clone().filter(|_| !mapped).unwrap_or_else(|| if mapped { "text".into() } else { fmt.to_string() }));
		if column {
			if let Some(bar) = &c.bar {
				let mut b = Obj::new();
				b.put(
					"max",
					match &bar.max {
						Bound::Num(n) => J::Num(n.clone()),
						Bound::Name(s) => J::from(s.as_str()),
					},
				);
				o.put("bar", b);
			}
			if c.grow == Some(true) {
				o.put("grow", true);
			}
		}
		J::Obj(o)
	}

	fn table_json(&self, d: &Widget, o: &mut Obj) {
		let source = d.source.as_deref().unwrap_or("");
		o.put("source", source);
		o.put("columns", J::Arr(d.columns.iter().flatten().map(|c| self.column_json(source, c, true)).collect()));
		if let Some(s) = &d.sort {
			let mut so = Obj::new();
			so.put("field", s.field.as_str());
			so.put("desc", s.desc);
			o.put("sort", so);
		}
		o.put("sorts", J::Arr(d.sorts.iter().flatten().map(J::from).collect()));
		if let Some(labels) = d.sort_labels.as_ref().filter(|l| !l.is_empty()) {
			let mut l = Obj::new();
			for (field, label) in labels {
				l.put(field, label.as_str());
			}
			o.put("sort_labels", l);
		}
		let fields = search_fields(d);
		o.put("search", !fields.is_empty());
		if !fields.is_empty() {
			o.put("search_fields", J::Arr(fields.iter().map(J::from).collect()));
		}
		o.put("hide_empty", d.hide_empty.unwrap_or(false));
		o.opt("row_open", d.row_open.as_deref());
		if let Some(ra) = d.row_actions.as_ref().filter(|r| !r.is_empty()) {
			let list: Vec<J> = ra
				.iter()
				.map(|a| {
					let mut r = Obj::new();
					r.put("action", a.action.as_str());
					r.opt("label", a.label.as_deref());
					r.opt("params", a.params.clone().map(|p| J::from(Value::Object(p))));
					J::Obj(r)
				})
				.collect();
			o.put("row_actions", J::Arr(list));
		}
		if let Some(ex) = d.expand.as_ref().filter(|e| !e.is_empty()) {
			o.put("expand", J::Arr(ex.iter().map(|c| self.column_json(source, c, false)).collect()));
		}
	}
}

#[cfg(test)]
pub mod tests {
	use super::*;

	pub fn fixture_config() -> Config {
		settings::read_sources(&[settings::Source::new("ui-source.toml", include_str!("../fixtures/ui-source.toml"))]).unwrap()
	}

	pub fn input<'a>(config: &'a Config, values: &'a Values, caps: &'a Caps, role: Role, profile: &'a str, draws: Option<&'a BTreeSet<String>>) -> Input<'a> {
		let info = crate::fixtures::info();
		Input {
			config,
			values,
			caps,
			service: Service {
				id: info.id,
				name: info.name,
				version: info.service_version.into(),
				interval_ms: info.interval_ms,
				history_seconds: info.history_seconds,
				features: vec!["processes"],
			},
			control: Control { enabled: config.control, off_reason: (!config.control).then_some("settings") },
			role,
			profile,
			kind: "android",
			draws,
		}
	}

	#[test]
	fn what_a_client_that_draws_less_gets() {
		let config = fixture_config();
		let values = crate::fixtures::values(&config);
		let caps = Caps::all(&values.power_profiles);
		let draws: BTreeSet<String> = ["value", "text", "button", "group", "table", "graph"].iter().map(|s| s.to_string()).collect();
		let ui = build(&input(&config, &values, &caps, Role::Admin, "owner", Some(&draws)));
		let home = ui.dashboard("home").unwrap();
		let kind = |id: &str| home.all_widgets().iter().find(|w| w.id == id).map(|w| w.kind.clone());
		assert_eq!(kind("home/cpu").as_deref(), Some("value"));
		assert_eq!(kind("home/memory").as_deref(), Some("value"));
		assert_eq!(kind("home/battery").as_deref(), Some("value"));
		assert_eq!(kind("home/mode").as_deref(), Some("group"));
		assert_eq!(kind("home/mode/server").as_deref(), Some("button"));
		assert_eq!(kind("home/fan").as_deref(), Some("group"));
		let fan = home.all_widgets().into_iter().find(|w| w.id == "home/fan").unwrap();
		assert_eq!(fan.widgets.len(), 11);
		assert_eq!(fan.widgets[1].def.params.as_ref().unwrap()["speed"], 10);
		assert_eq!(kind("home/media").as_deref(), Some("text"));
		assert_eq!(kind("home/cores").as_deref(), Some("text"));
		let media = home.all_widgets().into_iter().find(|w| w.id == "home/media").unwrap();
		assert_eq!(media.def.level.as_deref(), Some("warning"));
		// Without groups, what was in them stands on its own.
		let draws: BTreeSet<String> = ["value", "button"].iter().map(|s| s.to_string()).collect();
		let ui = build(&input(&config, &values, &caps, Role::Admin, "owner", Some(&draws)));
		let ids: Vec<String> = ui.dashboard("home").unwrap().widgets.iter().map(|w| w.id.clone()).collect();
		assert!(ids.contains(&"home/power/reboot".to_string()) && !ids.contains(&"home/power".to_string()), "{ids:?}");
	}

	#[test]
	fn each_role_sees_its_own() {
		let config = fixture_config();
		let values = crate::fixtures::values(&config);
		let caps = Caps::all(&values.power_profiles);
		let ids = |role: Role| -> Vec<String> {
			let ui = build(&input(&config, &values, &caps, role, "owner", None));
			ui.dashboards.iter().flat_map(|d| d.all_widgets().into_iter().map(|w| w.id.clone())).collect()
		};
		let glance = ids(Role::Glance);
		for gone in ["home/links", "home/mute", "home/mode", "home/fan", "home/backup", "home/power", "home/media", "detail/procs"] {
			assert!(!glance.iter().any(|i| i == gone), "{gone} for glance");
		}
		assert!(glance.iter().any(|i| i == "home/cpu") && glance.iter().any(|i| i == "home/more"));
		let monitor = ids(Role::Monitor);
		assert!(monitor.iter().any(|i| i == "home/media") && monitor.iter().any(|i| i == "detail/procs") && !monitor.iter().any(|i| i == "home/mute"));
		let operate = ids(Role::Operate);
		assert!(operate.iter().any(|i| i == "home/mode") && !operate.iter().any(|i| i == "home/backup") && !operate.iter().any(|i| i == "home/power"));
		let glance_ui = build(&input(&config, &values, &caps, Role::Glance, "owner", None));
		assert!(glance_ui.actions.is_empty());
		// The computer can't: power left out, and the widgets for it.
		let caps = Caps { poweroff: false, reboot: false, suspend: false, ..Caps::all(&values.power_profiles) };
		let ui = build(&input(&config, &values, &caps, Role::Admin, "owner", None));
		assert!(ui.actions.iter().all(|a| a.id != "power.reboot" && a.id != "power.poweroff"));
		assert!(ui.dashboard("home").unwrap().all_widgets().iter().all(|w| w.id != "home/power"));
	}

	#[test]
	fn profiles_follow_their_use_and_units() {
		let config = settings::read_sources(&[settings::Source::new(
			"a.toml",
			"[profiles.hot]\nuse = \"default\"\nfahrenheit = true\nnetwork_bits = true\nconfirm_actions = false\nheader = \"\"\ndashboards = [\"overview\", \"gpu\"]\n[profiles.hot.android]\nhaptics = false\n",
		)])
		.unwrap();
		let values = crate::fixtures::values(&config);
		let caps = Caps::all(&values.power_profiles);
		let ui = build(&input(&config, &values, &caps, Role::Admin, "hot", None));
		let p = ui.doc.to_value()["profile"].clone();
		assert_eq!((p["fahrenheit"].as_bool(), p["android"]["haptics"].as_bool(), p["android"]["blur"].as_bool()), (Some(true), Some(false), Some(true)));
		assert_eq!(ui.doc.to_value()["header"], Value::Null);
		// A for_each dashboard as a tab is one tab for each item.
		assert_eq!(ui.tabs, ["overview", "gpu.nvidia"]);
		let temp = ui.dashboard("overview").unwrap().all_widgets().into_iter().find(|w| w.id == "overview/temperature").unwrap().clone();
		let json = ui.dashboards.iter().find(|d| d.id == "overview").map(|_| ui.doc.to_value()).unwrap();
		let w = json["dashboards"][0]["widgets"].as_array().unwrap().iter().find(|w| w["id"] == "overview/temperature").unwrap().clone();
		assert_eq!((w["unit"].as_str(), temp.def.value.as_deref()), (Some("°F"), Some("cpu.temp")));
		assert!(ui.doc.to_value()["actions"].as_array().unwrap().iter().all(|a| a["confirm"] == false));
		// The same document is the same version; another role, another.
		let again = build(&input(&config, &values, &caps, Role::Admin, "hot", None));
		assert_eq!(again.version, ui.version);
		assert_ne!(build(&input(&config, &values, &caps, Role::Monitor, "hot", None)).version, ui.version);
	}

	#[test]
	fn for_each_templates_and_groups_spread_out() {
		let config = settings::read_sources(&[
			settings::Source::new("ui-source.toml", include_str!("../fixtures/ui-source.toml")),
			settings::Source::new(
				"more.toml",
				r#"
[profiles.owner]
dashboards = ["home", "control", "t"]

[templates.hist]
type = "graph"
values = ["{src}.{item}"]
label = "{name} {item}"

[dashboards.t]
title = "T"

[[dashboards.t.widgets]]
id = "h"
use = "hist"
with = { src = "net", name = "Net" }
for_each = ["rx", "tx"]

[[dashboards.t.widgets]]
id = "fans"
type = "value"
for_each = "fans"
value = "fan.{item}"
label = "{item.label} at {item.rpm}"
"#,
			),
		])
		.unwrap();
		let values = crate::fixtures::values(&config);
		let caps = Caps::all(&values.power_profiles);
		let ui = build(&input(&config, &values, &caps, Role::Admin, "owner", None));
		let t = ui.dashboard("t").unwrap();
		let ids: Vec<&str> = t.widgets.iter().map(|w| w.id.as_str()).collect();
		assert_eq!(ids, ["t/h.rx", "t/h.tx", "t/fans.hp_cpu"]);
		assert_eq!(t.widgets[1].def.values.as_deref(), Some(&["net.tx".to_string()][..]));
		let doc = ui.doc.to_value();
		let t_json = doc["dashboards"].as_array().unwrap().iter().find(|d| d["id"] == "t").unwrap().clone();
		assert_eq!(t_json["widgets"][0]["label"], "Net rx");
		assert_eq!(t_json["widgets"][2]["label"], "CPU at 2500 RPM");
		// The built-in Control's group of the settings' actions: one button for each, in the one card.
		let control = ui.dashboard("control").unwrap();
		let group = control.widgets.iter().find(|w| w.id == "control/actions").unwrap();
		let buttons: Vec<(&str, Option<&str>)> = group.widgets.iter().map(|b| (b.id.as_str(), b.def.action.as_deref())).collect();
		assert_eq!(buttons, [("control/actions/0.backup", Some("backup")), ("control/actions/0.fan", Some("fan")), ("control/actions/0.mode", Some("mode"))]);
		assert_eq!(group.widgets[2].def.icon.as_deref(), Some("leaf"));
		let needs = control.needs();
		assert!(needs.links && needs.media && needs.volume && !needs.processes, "{needs:?}");
	}

	#[test]
	fn a_header_the_role_sees_none_of_is_none() {
		let config = settings::read_sources(&[settings::Source::new(
			"a.toml",
			"[[dashboards.header.widgets]]\ntype = \"status\"\nvalue = \"volume.muted\"\n",
		)])
		.unwrap();
		let values = crate::fixtures::values(&config);
		let caps = Caps::all(&values.power_profiles);
		let glance = build(&input(&config, &values, &caps, Role::Glance, "default", None));
		assert_eq!((glance.header.as_deref(), glance.doc.to_value()["header"].clone()), (None, Value::Null));
		assert!(glance.dashboard("header").is_none());
		let monitor = build(&input(&config, &values, &caps, Role::Monitor, "default", None));
		assert_eq!(monitor.header.as_deref(), Some("header"));
		// After the tabs, before what they open.
		let order: Vec<&str> = monitor.dashboards.iter().map(|d| d.id.as_str()).collect();
		assert_eq!(&order[..5], ["overview", "processes", "sensors", "control", "header"]);
	}

	#[test]
	fn a_table_searches_the_fields_it_names_or_its_text_columns() {
		let table = |extra: &str| {
			let text = format!("[dashboards.t]\ntitle = \"T\"\n[[dashboards.t.widgets]]\ntype = \"table\"\nsource = \"processes\"\ncolumns = [{{ field = \"name\" }}, {{ field = \"cpu\" }}, {{ field = \"status\", map = {{ R = \"Running\" }} }}]\nexpand = [{{ field = \"cmd\" }}, {{ field = \"name\" }}]\nsorts = [\"cpu\", \"mem\"]\n{extra}");
			settings::read_sources(&[settings::Source::new("a.toml", text)])
		};
		let fields = |extra: &str| {
			let config = table(extra).unwrap();
			let w: Widget = serde_json::from_value(Value::Object(config.dashboards["t"].widgets[0].clone())).unwrap();
			search_fields(&w)
		};
		assert_eq!(fields("search = true"), ["name", "status", "cmd"]);
		assert_eq!(fields("search = [\"pid\", \"user\"]"), ["pid", "user"]);
		assert!(fields("search = false").is_empty() && fields("").is_empty());
		// Fields the collection hasn't, and labels for orders the table doesn't take, are mistakes.
		let mistakes = |extra: &str| settings::join_mistakes(&table(extra).unwrap_err());
		assert!(mistakes("search = [\"nope\"]").contains("no field \"nope\" in processes"));
		assert!(mistakes("sort_labels = { name = \"Name\" }").contains("isn't one of the table's sorts"));
		assert!(table("sort_labels = { cpu = \"CPU\", mem = \"Memory\" }").is_ok());
	}

	#[test]
	fn the_history_keeps_what_graphs_and_sparks_show() {
		let config = Config::default();
		let values = crate::fixtures::values(&config);
		let tracked = tracked(&config, &values);
		for a in ["cpu.usage", "cpu.temp", "net.rx", "net.tx", "gpu.nvidia.usage", "gpu.nvidia.temp", "mem.used", "swap.used", "disk.read", "battery.power"] {
			assert!(tracked.contains(a), "{a}");
		}
		assert!(!tracked.contains("mem.total"));
	}
}
