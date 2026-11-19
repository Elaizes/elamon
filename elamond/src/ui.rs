//! `GET /api/v1/ui` (docs/protocol.md): what one client shows, built from the settings, the values
//! the computer has now, the client's role and profile, and the widget types it draws. The
//! dashboards come out resolved — templates applied, `for_each` spread, `when` decided, what the
//! role doesn't see left out, defaults written — both as the document the client gets and as
//! `Dashboard`s the stream's states are built from.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use serde::ser::{Serialize, SerializeMap, Serializer};
use serde_json::Value;

use crate::access::Role;
use crate::actions::{self, Caps, ParamKind, Spec};
use crate::format;
use crate::settings::{self, Bound, Config, ForEach, Labels, ListItem, Profile, Widget};
use crate::values::{self, Item, Units, Values, View};

// ---- JSON with its keys in the order written ------------------------------------------------

/// An object of JSON that keeps its keys in the order they were put in (serde_json's
/// `preserve_order`), so the documents read as the protocol shows them.
pub type Obj = serde_json::Map<String, Value>;

/// `put` and `opt` for `Obj`: sets a key in its place when it is there already, else at the end.
pub trait Put {
	fn put(&mut self, key: &str, v: impl Into<Value>);
	fn opt<T: Into<Value>>(&mut self, key: &str, v: Option<T>);
}

impl Put for Obj {
	fn put(&mut self, key: &str, v: impl Into<Value>) {
		self.insert(key.to_string(), v.into());
	}

	fn opt<T: Into<Value>>(&mut self, key: &str, v: Option<T>) {
		if let Some(v) = v {
			self.put(key, v);
		}
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
	/// Where the computer is now: the addresses a phone could reach it at, then names.
	pub endpoints: Vec<String>,
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
	/// The first parts of the addresses its widgets name that aren't the computer's (`app` of
	/// `app.state`): of the widgets as the settings write them, those a client doesn't see now (a
	/// `when` not yet met, a list's rows without values) included, and as `for_each` spread them.
	/// The sources a client with it on screen needs read are those whose ids it has.
	pub heads: BTreeSet<String>,
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
	/// the collection of a table or bars, and its addresses (`each_address`).
	pub fn needs(&self) -> Sources {
		let mut n = Sources::default();
		if let Some(s) = &self.def.source {
			n.collection(s);
		}
		if self.kind == "media" {
			n.media = true;
			n.volume = true;
		}
		self.each_address(|a| n.address(a));
		n
	}

	/// Each address this widget's own state shows, is judged by or holds in its strings.
	pub fn each_address<'a>(&'a self, mut f: impl FnMut(&'a str)) {
		let d = &self.def;
		for a in addresses(d) {
			f(a);
		}
		let mut texts: Vec<&str> = [&d.label, &d.caption, &d.text, &d.empty].into_iter().flatten().map(String::as_str).collect();
		for i in d.items.iter().flatten() {
			texts.extend([&i.label, &i.text, &i.empty].into_iter().flatten().map(String::as_str));
			for a in [&i.value, &i.when].into_iter().flatten() {
				f(a);
			}
		}
		for t in texts {
			for p in format::placeholders(t) {
				f(format::split_placeholder(p.inner).0);
			}
		}
		if let Some(a) = &d.when {
			f(a);
		}
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
	/// Some source's values: which sources, `Dashboard::heads` says.
	pub sources: bool,
}

impl Sources {
	/// Every source.
	pub const ALL: Sources = Sources { processes: true, media: true, volume: true, gpus: true, displays: true, links: true, sources: true };

	/// What either needs.
	pub fn union(self, o: Sources) -> Sources {
		Sources {
			processes: self.processes || o.processes,
			media: self.media || o.media,
			volume: self.volume || o.volume,
			gpus: self.gpus || o.gpus,
			displays: self.displays || o.displays,
			links: self.links || o.links,
			sources: self.sources || o.sources,
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
			"displays" | "display" => self.displays = true,
			"volume" => self.volume = true,
			"link" => self.links = true,
			// Not one of the computer's: a source's.
			_ if !values::known_prefix(a) => self.sources = true,
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
			(self.sources, "sources"),
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
	pub doc: Value,
	pub version: String,
	pub profile: Profile,
	pub units: Units,
	pub tabs: Vec<String>,
	pub header: Option<String>,
	/// The tabs, the header, then the dashboards reached through `open`, a widget's then a row's.
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
	/// The profile's header, whose chips a `log` makes no sense among.
	header: Option<&'a str>,
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
		let mut named = Value::Array(d.widgets.iter().cloned().map(Value::Object).collect());
		if let Some(it) = &item {
			self.fill_item(&mut named, it, false);
		}
		let mut heads = BTreeSet::new();
		self.heads_in(&named, false, &mut heads);
		let mut out = Dashboard {
			id: id.to_string(),
			title: self.fill(title.as_str().unwrap_or("")),
			icon: d.icon.clone().unwrap_or_else(|| "overview".into()),
			widgets,
			heads: BTreeSet::new(),
		};
		for w in out.all_widgets() {
			w.each_address(|a| heads.extend(head_of(a).map(str::to_string)));
		}
		out.heads = heads;
		Some(out)
	}

	/// The heads of the addresses in `v`, widgets as the settings write them, templates applied:
	/// of each placeholder, and of `v` itself when `address` (a key that holds an address).
	fn heads_in(&self, v: &Value, address: bool, out: &mut BTreeSet<String>) {
		let mut add = |a: &str| out.extend(head_of(a).map(str::to_string));
		match v {
			Value::String(s) => {
				for p in format::placeholders(s) {
					add(format::split_placeholder(p.inner).0);
				}
				if address {
					add(s);
				}
			}
			Value::Array(a) => a.iter().for_each(|e| self.heads_in(e, address, out)),
			Value::Object(o) => {
				let merged = if o.contains_key("use") { settings::apply_template(o, &self.config.templates).unwrap_or_else(|_| o.clone()) } else { o.clone() };
				// A table's row actions fill in the row's fields, not values.
				for (k, e) in merged.iter().filter(|(k, _)| *k != "row_actions") {
					self.heads_in(e, ADDRESS_KEYS.contains(&k.as_str()), out);
				}
			}
			_ => {}
		}
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
		if w.when.as_deref().is_some_and(|a| !self.view.exists(a)) {
			return None;
		}
		// The least role its values, collection and action need, or its own.
		let mut need = w.role.unwrap_or(Role::Glance);
		for a in addresses(&w) {
			need = need.max(self.view.role_of(a));
		}
		// A timeline's source is a station, which its status shows to all.
		if let Some(s) = w.source.as_ref().filter(|_| kind != "timeline") {
			need = need.max(values::collection_role(s));
		}
		if kind == "media" || kind == "log" {
			need = need.max(Role::Monitor);
		}
		// The events name units, disks and sources: for the role the settings give them.
		if kind == "log" && w.scope.as_deref() == Some("events") {
			need = need.max(self.config.events.role);
		}
		if self.view.role < need {
			return None;
		}
		// A journal shows in a dashboard, not among the header's chips; a unit's, only of a link's unit.
		if kind == "log" && (prefix.split('/').next() == self.header || crate::journal::Key::of(&w, self.config).is_none()) {
			return None;
		}
		if w.action.as_deref().is_some_and(|a| !self.actions.contains_key(a)) {
			return None;
		}
		// A toggle's `on` and `off` are values its action's parameter takes, or it is left out: a
		// station's command the station doesn't offer, or its source doesn't allow.
		if kind == "toggle"
			&& let Some(spec) = w.action.as_deref().and_then(|a| self.actions.get(a))
		{
			let param = w.param.as_deref().and_then(|p| spec.params.iter().find(|x| x.id == p));
			let (on, off) = (w.on.clone().unwrap_or(Value::Bool(true)), w.off.clone().unwrap_or(Value::Bool(false)));
			if !param.is_some_and(|p| actions::param_takes(p, &on) && actions::param_takes(p, &off)) {
				return None;
			}
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
				// One that opens a dashboard goes to a client that reads the journal (an app before
				// 0.8 needs `action`).
				if let Some(ra) = &mut w.row_actions {
					ra.retain(|a| match &a.action {
						Some(action) => self.actions.contains_key(action),
						None => a.open.is_some() && self.draws("log"),
					});
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
				if i.when.as_deref().is_some_and(|a| !self.view.exists(a)) {
					continue;
				}
				let shows = match (&i.value, &i.text) {
					(Some(a), _) => {
						self.view.role >= self.view.role_of(a) && (self.view.exists(a) || i.empty.as_deref().is_some_and(|e| !self.fill(e).is_empty()))
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
			// The source's status and how long it has been so, coloured by the status.
			"timeline" if self.draws("text") => {
				let src = w.def.source.clone().unwrap_or_default();
				let text = format!("{{{src}.status}}{}{{{src}.status.since:duration}}", format::PARTS);
				let def = Widget {
					kind: Some("text".into()),
					label: w.def.label.clone(),
					icon: w.def.icon.clone(),
					text: Some(text),
					value: Some(format!("{src}.status")),
					..Default::default()
				};
				return UiWidget { id: w.id, kind: "text".into(), span: w.span, def, widgets: Vec::new() };
			}
			_ => {}
		}
		let label = w.def.label.clone();
		let text = match w.kind.as_str() {
			"log" => "Update the app to read the journal.".to_string(),
			kind => format!("Update the app to see this {kind} widget."),
		};
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
				// As many as fit, for a range wider than any count of steps.
				let steps = ((hi - lo) / st).floor() as u64;
				let picks: Vec<u64> =
					if steps <= 10 { (0..=steps).collect() } else { (0..=10u64).map(|i| ((u128::from(i) * u128::from(steps) + 5) / 10) as u64).collect() };
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

/// The first part of `address` when it isn't one of the computer's: what may be a source's id.
fn head_of(address: &str) -> Option<&str> {
	let (head, _) = address.split_once('.')?;
	(settings::is_id(head) && head != "item" && !values::known_prefix(address)).then_some(head)
}

/// The keys of a widget, or of a list's row, that hold an address.
const ADDRESS_KEYS: [&str; 9] = ["value", "values", "when", "warning_above", "critical_above", "warning_below", "critical_below", "min", "max"];

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

/// Every `open` of these widgets, in order; and of their tables' rows.
fn opens(widgets: &[UiWidget], out: &mut Vec<String>, rows: &mut Vec<String>) {
	for w in widgets {
		out.extend(w.def.open.clone());
		rows.extend(w.def.row_actions.iter().flatten().filter_map(|a| a.open.clone()));
		opens(&w.widgets, out, rows);
	}
}

/// Builds `ui` for one client.
pub fn build(input: &Input) -> Ui {
	let profile = input.config.profile(input.profile);
	let units = units_of(&profile);
	let view = View::new(input.values, input.role, units);
	let available: Vec<Spec> = actions::available(input.config, input.caps).into_iter().filter(|s| input.role >= s.role).collect();
	let header_id = Some(profile.header.clone()).filter(|h| !h.is_empty());
	let ctx = Ctx {
		config: input.config,
		view,
		actions: available.iter().map(|s| (s.id.clone(), s.clone())).collect(),
		draws: input.draws,
		history_seconds: input.service.history_seconds,
		header: header_id.as_deref(),
	};

	// The tabs, the header, then what their widgets open, each once; then the dashboards of the
	// rows of their tables, and what those open.
	let tab_ids: Vec<String> = profile.dashboards.iter().flat_map(|d| ctx.expand(d)).collect();
	// An Android client's home-screen widget, tiles and card show dashboards of their own.
	let outside: Vec<String> = if input.kind == "android" { profile.android_dashboards().map(String::from).collect() } else { Vec::new() };
	let mut queue: VecDeque<String> = tab_ids.iter().cloned().chain(header_id.clone()).chain(outside).collect();
	let mut later: VecDeque<String> = VecDeque::new();
	let mut seen = HashSet::new();
	let mut dashboards: Vec<Dashboard> = Vec::new();
	while let Some(id) = queue.pop_front().or_else(|| later.pop_front()) {
		if !seen.insert(id.clone()) {
			continue;
		}
		if let Some(d) = ctx.dashboard(&id) {
			let (mut more, mut rows) = (Vec::new(), Vec::new());
			opens(&d.widgets, &mut more, &mut rows);
			queue.extend(more);
			later.extend(rows.iter().flat_map(|r| ctx.expand(r)));
			dashboards.push(d);
		}
	}
	let visible: HashSet<String> = dashboards.iter().map(|d| d.id.clone()).collect();
	for d in &mut dashboards {
		prune(&mut d.widgets, &visible);
	}
	let tabs: Vec<String> = tab_ids.into_iter().filter(|t| visible.contains(t)).collect();
	let header = header_id.clone().filter(|h| visible.contains(h));

	let mut doc = Obj::new();
	doc.put("service", service_json(&input.service));
	doc.put("role", input.role.as_str());
	let mut control = Obj::new();
	control.put("enabled", input.control.enabled);
	control.put("off_reason", input.control.off_reason.map_or(Value::Null, Value::from));
	doc.put("control", control);
	let mut profile_doc = profile_json(&profile, input.kind, input.service.interval_ms);
	// Where to change it, for the person who may: a file and a line of the settings.
	if input.role >= Role::Admin
		&& let Some(w) = input.config.profile_sources.get(&profile.id)
	{
		let mut s = Obj::new();
		s.put("file", w.file.as_str());
		s.put("line", w.line as u64);
		profile_doc.put("source", s);
	}
	doc.put("profile", profile_doc);
	doc.put("tabs", Value::Array(tabs.iter().cloned().map(Value::from).collect()));
	doc.put("header", header.clone().map_or(Value::Null, Value::from));
	doc.put("dashboards", Value::Array(dashboards.iter().map(|d| Value::Object(ctx.dashboard_json(d))).collect()));
	doc.put("actions", Value::Array(available.iter().map(|s| Value::Object(action_json(s, &profile))).collect()));
	let version = version_of(&doc);
	let mut full = Obj::new();
	full.put("version", version.clone());
	full.extend(doc);
	Ui { doc: Value::Object(full), version, profile, units, tabs, header, dashboards, actions: available }
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
		header: None,
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

/// The sources the timelines of any dashboard show: asked in the background, as graphs' are.
pub fn timelines(config: &Config, values: &Values) -> BTreeSet<String> {
	let mut out = BTreeSet::new();
	for d in all_dashboards(config, values) {
		out.extend(d.all_widgets().into_iter().filter(|w| w.kind == "timeline").filter_map(|w| w.def.source.clone()));
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
	o.put("features", Value::Array(s.features.iter().map(|f| Value::from(*f)).collect()));
	o.put("endpoints", Value::Array(s.endpoints.iter().cloned().map(Value::from).collect()));
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
	unlock.put("for", Value::Array(p.unlock_for.iter().cloned().map(Value::from).collect()));
	unlock.put("grace", p.unlock_grace.as_str());
	o.put("unlock", unlock);
	match kind {
		"web" => o.put("web", Value::Object(p.web.clone())),
		_ => {
			let mut a = Obj::new();
			for (k, _) in settings::ANDROID {
				a.opt(k, p.android.get(k).copied());
			}
			for (k, v) in &p.android {
				a.put(k, *v);
			}
			for (k, d) in [("widget", &p.widget), ("tiles", &p.tiles), ("card", &p.card)] {
				a.opt(k, Some(d.as_str()).filter(|d| !d.is_empty()));
			}
			if !p.widget.is_empty() {
				a.put("widget_minutes", p.widget_minutes);
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
					// Typed in to run: the client asks for exactly this.
					if let Some(t) = p.typed.get(v) {
						ch.put("confirm_text", t.as_str());
					}
					Value::Object(ch)
				})
				.collect();
			o.put("choices", Value::Array(choices));
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
	o.opt("default", p.default.clone());
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
	o.put("params", Value::Array(s.params.iter().map(|p| Value::Object(param_json(p))).collect()));
	o
}

fn bound_num(b: &Option<Bound>) -> Option<Value> {
	match b {
		Some(Bound::Num(n)) => Some(Value::Number(n.clone())),
		_ => None,
	}
}

impl Ctx<'_> {
	fn dashboard_json(&self, d: &Dashboard) -> Obj {
		let mut o = Obj::new();
		o.put("id", d.id.as_str());
		o.put("title", d.title.as_str());
		o.put("icon", d.icon.as_str());
		o.put("widgets", Value::Array(d.widgets.iter().map(|w| Value::Object(self.widget_json(w))).collect()));
		o
	}

	/// The unit a value is shown in: the widget's, or its address's for this client.
	fn unit_of(&self, given: &Option<String>, address: Option<&str>) -> Option<String> {
		given.clone().or_else(|| address.map(|a| self.view.unit_of(a).to_string())).filter(|u| !u.is_empty())
	}

	/// The format a value is shown in: the widget's, or its address's.
	fn format_of(&self, given: &Option<String>, address: Option<&str>, otherwise: &str) -> String {
		given.clone().or_else(|| address.and_then(|a| self.view.format_of(a)).map(str::to_string)).unwrap_or_else(|| otherwise.to_string())
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
					o.put("min", bound_num(&d.min).unwrap_or(Value::from(0u64)));
					let own = value.and_then(|a| self.view.values.scalars.get(a)).and_then(|s| match &s.max {
						Some(values::Max::Num(n)) => Some(Value::Number(n.clone())),
						_ => None,
					});
					match &d.max {
						Some(Bound::Num(n)) => o.put("max", Value::Number(n.clone())),
						Some(Bound::Name(_)) => {}
						None => o.opt("max", own.or_else(|| value.and_then(|a| self.view.def(a)).filter(|a| a.unit == "%").map(|_| Value::from(100u64)))),
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
				let series: Vec<Value> = d
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
						Value::Object(s)
					})
					.collect();
				o.put("series", Value::Array(series));
				o.put("range", d.range.unwrap_or(self.history_seconds).min(self.history_seconds));
				o.put("height", d.height.clone().unwrap_or_else(|| "medium".into()));
				o.opt("min", bound_num(&d.min));
				o.opt("max", bound_num(&d.max));
				o.opt("floor", d.floor.clone());
				// The long history behind it, but for a small graph's.
				if d.ranges != Some(false) && d.height.as_deref() != Some("small") {
					o.put("ranges", Value::Array(crate::history::api::RANGES.iter().map(|r| Value::from(*r)).collect()));
				}
			}
			"timeline" => {
				o.put("range", d.range.unwrap_or(crate::consts::cadence::TIMELINE_RANGE));
				if d.ranges != Some(false) {
					o.put("ranges", Value::Array(crate::history::api::RANGES.iter().map(|r| Value::from(*r)).collect()));
				}
				// Only as the settings give it: without it, a client tells durations.
				o.opt("legend", d.legend.clone());
			}
			"list" => {
				let items: Vec<Value> = d
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
						Value::Object(it)
					})
					.collect();
				o.put("items", Value::Array(items));
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
				o.opt("params", d.params.clone().map(Value::Object));
			}
			"toggle" => {
				o.opt("action", d.action.as_deref());
				o.opt("param", d.param.as_deref());
				o.put("on", d.on.clone().unwrap_or(Value::Bool(true)));
				o.put("off", d.off.clone().unwrap_or(Value::Bool(false)));
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
				o.put("widgets", Value::Array(w.widgets.iter().map(|c| Value::Object(self.widget_json(c))).collect()));
			}
			"log" => {
				let scope = d.scope.as_deref().unwrap_or("system");
				o.put("scope", scope);
				if scope == "unit" {
					o.opt("unit", d.unit.as_deref());
				}
				o.put("level", d.level.as_deref().unwrap_or("info"));
				o.put("lines", d.lines.unwrap_or(crate::consts::limit::LOG_LINES));
				o.put("search", d.search != Some(settings::Search::On(false)));
			}
			_ => {}
		}
		o
	}

	fn column_json(&self, source: &str, c: &settings::Column, column: bool) -> Value {
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
						Bound::Num(n) => Value::Number(n.clone()),
						Bound::Name(s) => Value::from(s.as_str()),
					},
				);
				o.put("bar", b);
			}
			if c.grow == Some(true) {
				o.put("grow", true);
			}
		}
		Value::Object(o)
	}

	fn table_json(&self, d: &Widget, o: &mut Obj) {
		let source = d.source.as_deref().unwrap_or("");
		o.put("source", source);
		o.put("columns", Value::Array(d.columns.iter().flatten().map(|c| self.column_json(source, c, true)).collect()));
		if let Some(s) = &d.sort {
			let mut so = Obj::new();
			so.put("field", s.field.as_str());
			so.put("desc", s.desc);
			o.put("sort", so);
		}
		o.put("sorts", Value::Array(d.sorts.iter().flatten().cloned().map(Value::from).collect()));
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
			o.put("search_fields", Value::Array(fields.iter().cloned().map(Value::from).collect()));
		}
		o.put("hide_empty", d.hide_empty.unwrap_or(false));
		o.opt("row_open", d.row_open.as_deref());
		if let Some(ra) = d.row_actions.as_ref().filter(|r| !r.is_empty()) {
			let list: Vec<Value> = ra
				.iter()
				.map(|a| {
					let mut r = Obj::new();
					match &a.action {
						Some(action) => r.put("action", action.as_str()),
						None => r.opt("open", a.open.as_deref()),
					}
					r.opt("label", a.label.as_deref());
					r.opt("params", a.params.clone().filter(|_| a.action.is_some()).map(Value::Object));
					Value::Object(r)
				})
				.collect();
			o.put("row_actions", Value::Array(list));
		}
		if let Some(ex) = d.expand.as_ref().filter(|e| !e.is_empty()) {
			o.put("expand", Value::Array(ex.iter().map(|c| self.column_json(source, c, false)).collect()));
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
				endpoints: Vec::new(),
			},
			control: Control { enabled: config.control, off_reason: (!config.control).then_some("settings") },
			role,
			profile,
			kind: "android",
			draws,
		}
	}

	#[test]
	fn a_range_wider_than_any_count_of_steps_still_makes_buttons() {
		let config = crate::settings::read_over_defaults(&[crate::settings::Source::new(
			"a.toml",
			"[actions.big]\nname = \"Big\"\ncommand = [\"big\", \"{n}\"]\nparams.n = { range = [0, 9223372036854775807] }\n\
			 [actions.huge]\nname = \"Huge\"\ncommand = [\"huge\", \"{x}\"]\nparams.x = { range = [-1e300, 1e300], step = 1e-300 }\n\
			 [dashboards.big]\ntitle = \"Big\"\n\
			 [[dashboards.big.widgets]]\ntype = \"slider\"\naction = \"big\"\nparam = \"n\"\n\
			 [[dashboards.big.widgets]]\ntype = \"slider\"\naction = \"huge\"\nparam = \"x\"\n\
			 [profiles.default]\ndashboards = [\"big\"]\n",
		)])
		.unwrap();
		let values = crate::fixtures::values(&config);
		let caps = Caps::all(&values.power_profiles);
		let draws: BTreeSet<String> = ["group", "button", "text"].iter().map(|s| s.to_string()).collect();
		let ui = build(&input(&config, &values, &caps, Role::Admin, "default", Some(&draws)));
		let big = ui.dashboard("big").unwrap();
		for w in &big.widgets {
			assert_eq!((w.kind.as_str(), w.widgets.len()), ("group", 11), "{w:?}");
		}
		let last = big.widgets[0].widgets.last().unwrap().def.params.as_ref().unwrap()["n"].clone();
		assert_eq!(last, serde_json::json!(i64::MAX));
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
		let config = settings::read_over_defaults(&[settings::Source::new(
			"a.toml",
			"[profiles.hot]\nuse = \"default\"\nfahrenheit = true\nnetwork_bits = true\nconfirm_actions = false\nheader = \"\"\ndashboards = [\"overview\", \"gpu\"]\n[profiles.hot.android]\nhaptics = false\n",
		)])
		.unwrap();
		let values = crate::fixtures::values(&config);
		let caps = Caps::all(&values.power_profiles);
		let ui = build(&input(&config, &values, &caps, Role::Admin, "hot", None));
		let p = ui.doc.clone()["profile"].clone();
		assert_eq!((p["fahrenheit"].as_bool(), p["android"]["haptics"].as_bool(), p["android"]["blur"].as_bool()), (Some(true), Some(false), Some(true)));
		assert_eq!(ui.doc.clone()["header"], Value::Null);
		// A for_each dashboard as a tab is one tab for each item.
		assert_eq!(ui.tabs, ["overview", "gpu.nvidia"]);
		let temp = ui.dashboard("overview").unwrap().all_widgets().into_iter().find(|w| w.id == "overview/temperature").unwrap().clone();
		let json = ui.dashboards.iter().find(|d| d.id == "overview").map(|_| ui.doc.clone()).unwrap();
		let w = json["dashboards"][0]["widgets"].as_array().unwrap().iter().find(|w| w["id"] == "overview/temperature").unwrap().clone();
		assert_eq!((w["unit"].as_str(), temp.def.value.as_deref()), (Some("°F"), Some("cpu.temp")));
		assert!(ui.doc.clone()["actions"].as_array().unwrap().iter().all(|a| a["confirm"] == false));
		// The same document is the same version; another role, another.
		let again = build(&input(&config, &values, &caps, Role::Admin, "hot", None));
		assert_eq!(again.version, ui.version);
		assert_ne!(build(&input(&config, &values, &caps, Role::Monitor, "hot", None)).version, ui.version);
	}

	#[test]
	fn for_each_templates_and_groups_spread_out() {
		let config = settings::read_sources(&[
			// The standard Control with the fixture's actions.
			settings::Source::new(settings::DEFAULTS_NAME, settings::DEFAULTS),
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
		let doc = ui.doc.clone();
		let t_json = doc["dashboards"].as_array().unwrap().iter().find(|d| d["id"] == "t").unwrap().clone();
		assert_eq!(t_json["widgets"][0]["label"], "Net rx");
		assert_eq!(t_json["widgets"][2]["label"], "CPU at 2500 RPM");
		// The standard Control's group of the settings' actions: one button for each, in the one card.
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
		let config = settings::read_over_defaults(&[settings::Source::new(
			"a.toml",
			"[[dashboards.header.widgets]]\ntype = \"status\"\nvalue = \"volume.muted\"\n",
		)])
		.unwrap();
		let values = crate::fixtures::values(&config);
		let caps = Caps::all(&values.power_profiles);
		let glance = build(&input(&config, &values, &caps, Role::Glance, "default", None));
		assert_eq!((glance.header.as_deref(), glance.doc.clone()["header"].clone()), (None, Value::Null));
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
			settings::read_over_defaults(&[settings::Source::new("a.toml", text)])
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

	#[test]
	fn graphs_but_small_ones_offer_the_long_ranges_unless_told_not_to() {
		let config = settings::read_sources(&[settings::Source::new(
			"a.toml",
			"[profiles.default]\ndashboards = [\"g\"]\n[dashboards.g]\ntitle = \"G\"\n\
			 [[dashboards.g.widgets]]\nid = \"medium\"\ntype = \"graph\"\nvalues = [\"cpu.usage\"]\n\
			 [[dashboards.g.widgets]]\nid = \"large\"\ntype = \"graph\"\nvalues = [\"cpu.usage\"]\nheight = \"large\"\nranges = true\n\
			 [[dashboards.g.widgets]]\nid = \"small\"\ntype = \"graph\"\nvalues = [\"cpu.usage\"]\nheight = \"small\"\nranges = true\n\
			 [[dashboards.g.widgets]]\nid = \"off\"\ntype = \"graph\"\nvalues = [\"cpu.usage\"]\nranges = false\n",
		)])
		.unwrap();
		let values = crate::fixtures::values(&config);
		let caps = Caps::all(&values.power_profiles);
		let ui = build(&input(&config, &values, &caps, Role::Glance, "default", None));
		let widgets = ui.doc["dashboards"][0]["widgets"].as_array().unwrap().clone();
		let ranges: Vec<(&str, Option<&Value>)> = widgets.iter().map(|w| (w["id"].as_str().unwrap(), w.get("ranges"))).collect();
		let all = serde_json::json!(["1h", "24h", "7d", "30d", "1y", "all"]);
		assert_eq!(ranges, [("g/medium", Some(&all)), ("g/large", Some(&all)), ("g/small", None), ("g/off", None)]);
		assert!(settings::read_sources(&[settings::Source::new("b.toml", "[[dashboards.g.widgets]]\ntype = \"value\"\nvalue = \"cpu.usage\"\nranges = true\n")]).is_err());
	}

	/// The journal: for monitor and up, as the client draws it or as a word that the app needs
	/// updating; a unit's only of a link's unit; none among the header's chips; the dashboards the
	/// rows of a table open last, for a client that reads the journal.
	#[test]
	fn the_journal_goes_to_a_client_that_reads_it() {
		let config = settings::read_over_defaults(&[settings::Source::new(
			"a.toml",
			r#"
[links.web]
name = "Web"
url = "http://{host}:8080"
unit = "web.service"
scope = "system"
[links.plain]
name = "Plain"
url = "http://{host}:8081"
[profiles.default]
dashboards = ["overview", "control", "units"]
header = "chips"
[dashboards.chips]
title = "Chips"
[[dashboards.chips.widgets]]
type = "log"
[[dashboards.chips.widgets]]
type = "value"
value = "cpu.usage"
[dashboards.units]
title = "Units"
[[dashboards.units.widgets]]
id = "each"
type = "log"
scope = "unit"
for_each = ["web.service", "nothing.service"]
unit = "{item}"
"#,
		)])
		.unwrap();
		let values = crate::fixtures::values(&config);
		let caps = Caps::all(&values.power_profiles);
		let all = |role| build(&input(&config, &values, &caps, role, "default", None));
		let ui = all(Role::Monitor);
		let ids: Vec<&str> = ui.dashboards.iter().map(|d| d.id.as_str()).collect();
		assert_eq!((&ids[..4], ids.last().copied()), (&["overview", "control", "units", "chips"][..], Some("unit_log.web")), "{ids:?}");
		assert!(ids.contains(&"journal") && !ids.contains(&"unit_log.plain"), "{ids:?}");
		let widgets = |d: &str| -> Vec<String> { ui.dashboard(d).unwrap().all_widgets().iter().map(|w| format!("{} {}", w.id, w.kind)).collect() };
		assert_eq!(widgets("chips"), ["chips/1 value"]);
		assert_eq!(widgets("units"), ["units/each.web_service log"]);
		assert_eq!(widgets("unit_log.web"), ["unit_log.web/log log"]);
		let log = |ui: &Ui, id: &str| crate::fixtures::widget_of(&ui.doc, id);
		assert_eq!(
			log(&ui, "unit_log.web/log"),
			Some(serde_json::json!({ "id": "unit_log.web/log", "type": "log", "span": 4, "label": "Journal", "scope": "unit", "unit": "web.service", "level": "info", "lines": 200, "search": true }))
		);
		let links = log(&ui, "control/links").unwrap();
		assert_eq!(links["row_actions"].as_array().unwrap().last(), Some(&serde_json::json!({ "open": "unit_log", "label": "Journal" })));
		// Glance sees no journal at all.
		let glance = all(Role::Glance);
		assert!(glance.dashboards.iter().flat_map(|d| d.all_widgets()).all(|w| w.kind != "log"));
		assert!(glance.dashboard("journal").is_none() && glance.dashboard("unit_log.web").is_none());
		// An app before 0.8: the word, no row that opens a dashboard, nor the dashboards only those open.
		let draws: BTreeSet<String> = settings::WIDGET_TYPES.iter().filter(|t| **t != "log").map(|t| t.to_string()).collect();
		let old = build(&input(&config, &values, &caps, Role::Admin, "default", Some(&draws)));
		assert!(old.dashboard("unit_log.web").is_none());
		let system = log(&old, "journal/system").unwrap();
		assert_eq!((&system["type"], &system["text"], &system["level"]), (&Value::from("text"), &Value::from("Update the app to read the journal."), &Value::from("warning")));
		let links = log(&old, "control/links").unwrap();
		assert!(links["row_actions"].as_array().unwrap().iter().all(|a| a.get("action").is_some()), "{links}");
	}

	/// The sources a dashboard on screen needs read: of what its widgets name, those a client
	/// doesn't see yet included, and of what `for_each` spreads; the header's beside it.
	#[test]
	fn a_dashboard_names_the_sources_it_needs_read() {
		let config = settings::read_sources(&[settings::Source::new(
			"a.toml",
			r#"
default_profile = "p"
[sources.app]
kind = "http"
url = "http://localhost:8080/status"
[sources.web]
kind = "http"
url = "http://localhost:8081/status"
[sources.job]
kind = "command"
command = ["x"]
[sources.zone]
kind = "file"
path = "/sys/class/thermal/thermal_zone0/temp"
[sources.up]
kind = "command"
command = ["uptime"]
[sources.tpl]
kind = "command"
command = ["x"]

[templates.t]
type = "value"
value = "{src}.value"

[profiles.p]
dashboards = ["d"]
header = "h"

[dashboards.h]
title = "H"
[[dashboards.h.widgets]]
type = "text"
text = "{up.text}"

[dashboards.d]
title = "D"
[[dashboards.d.widgets]]
type = "value"
value = "job.files"
when = "job.files"
[[dashboards.d.widgets]]
type = "list"
items = [{ label = "Zone", value = "zone.text" }, { label = "CPU", value = "cpu.usage" }]
[[dashboards.d.widgets]]
type = "value"
for_each = ["app", "web"]
value = "{item}.state"
[[dashboards.d.widgets]]
use = "t"
with = { src = "tpl" }
[[dashboards.d.widgets]]
type = "table"
source = "links"
columns = [{ field = "name" }]
row_actions = [{ action = "unit.restart", params = { link = "{id}" } }]
"#,
		)])
		.unwrap();
		let mut values = crate::fixtures::values(&config);
		crate::feeds::put(&mut values, &config, &BTreeMap::new());
		let caps = Caps::all(&[]);
		let ui = build(&input(&config, &values, &caps, Role::Admin, "p", None));
		let d = ui.dashboard("d").unwrap();
		// None has been read: the value `when` holds to isn't shown, nor the list's row of the source.
		let shown: Vec<&str> = d.all_widgets().iter().map(|w| w.id.as_str()).collect();
		assert_eq!(shown, ["d/1", "d/2.app", "d/2.web", "d/3", "d/4"]);
		assert_eq!(d.widgets[0].def.items.as_ref().map(Vec::len), Some(1));
		assert_eq!(d.heads.iter().collect::<Vec<_>>(), ["app", "job", "tpl", "web", "zone"]);
		assert_eq!(crate::server::heads_of(&ui, d).iter().collect::<Vec<_>>(), ["app", "job", "tpl", "up", "web", "zone"]);
	}
}
