//! The dashboard editor (docs/protocol.md, "The dashboard editor"): forms for dashboards and the
//! profiles' tabs, which the page and the app build from what the service describes here — the
//! widget types and their keys from the same tables `elamond schema` is made from, the values,
//! actions and dashboards there are — and writes the service makes, in TOML of its own, to a drop-in
//! of its own (`[web] editor_file`). It writes dashboards and the tabs, header and Android
//! dashboards of profiles, and nothing else: no action, source, link, rule or target.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use axum::extract::{Extension, Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table};

use super::files::{self, Named};
use super::{bad_settings, record, sources_of, values_now};
use crate::access::Caller;
use crate::errors::ErrorCode;
use crate::server::{error_with, Hub};
use crate::settings::{self, Key, Mistake, Source, Ty};
use crate::store;

/// The keys of a dashboard the editor writes: all a dashboard has.
pub const DASHBOARD_KEYS: [&str; 5] = ["title", "icon", "role", "for_each", "widgets"];
/// The keys of a profile it writes.
pub const PROFILE_KEYS: [&str; 3] = ["dashboards", "header", "android"];
/// Of `android`, those it writes: the dashboards the app shows outside itself.
pub const ANDROID_KEYS: [&str; 3] = ["widget", "tiles", "card"];

/// What the editor's file says of itself.
pub const HEADER: &str = "# Written by Elamon's dashboard editor (the web page and the app): dashboards, and the tabs,\n\
# header and Android dashboards of profiles. A change by hand here is kept: elamond rewrites only\n\
# the tables it is asked to. Read after the other drop-ins whose names sort before this one's.\n";

/// The editor's changes, as one: a dashboard or a profile's part by id, `null` to take the
/// editor's table out (back to what the other files say).
#[derive(Deserialize, Default, Debug)]
pub struct Draft {
	#[serde(default)]
	pub dashboards: BTreeMap<String, Option<Map<String, Value>>>,
	#[serde(default)]
	pub profiles: BTreeMap<String, Option<Map<String, Value>>>,
}

/// The editor's file of the settings now.
fn editor_file(hub: &Hub) -> Named {
	let file = hub.config().web.editor_file.clone();
	Named { name: format!("{}/{file}", store::DROPIN_DIR), path: hub.place.dropins().join(file), kind: "dropin" }
}

/// The editor is on: `[web] editor`; 403 `admin_off` with `editor` otherwise.
fn editor_off(hub: &Hub) -> Option<Response> {
	(!hub.config().web.editor).then(|| error_with(ErrorCode::AdminOff, "editor"))
}

// ---- the draft, as TOML -------------------------------------------------------------------

/// A JSON value as TOML; `null` leaves the key out, a number keeps whether it is whole.
fn toml_value(v: &Value) -> Option<toml_edit::Value> {
	Some(match v {
		Value::Null => return None,
		Value::Bool(b) => (*b).into(),
		Value::Number(n) => match n.as_i64() {
			Some(i) => i.into(),
			None => n.as_f64()?.into(),
		},
		Value::String(s) => s.as_str().into(),
		Value::Array(a) => {
			let mut out = toml_edit::Array::new();
			for x in a.iter().filter_map(toml_value) {
				out.push(x);
			}
			toml_edit::Value::Array(out)
		}
		Value::Object(o) => {
			let mut out = toml_edit::InlineTable::new();
			for (k, x) in o {
				if let Some(x) = toml_value(x) {
					out.insert(k, x);
				}
			}
			toml_edit::Value::InlineTable(out)
		}
	})
}

/// A dashboard as its table: its own keys, then each widget as `[[dashboards.<id>.widgets]]`.
fn dashboard_table(def: &Map<String, Value>) -> Table {
	let mut t = Table::new();
	for key in DASHBOARD_KEYS.iter().filter(|k| **k != "widgets") {
		if let Some(v) = def.get(*key).and_then(toml_value) {
			t.insert(key, Item::Value(v));
		}
	}
	if let Some(Value::Array(widgets)) = def.get("widgets") {
		let mut list = ArrayOfTables::new();
		for w in widgets {
			let mut wt = Table::new();
			if let Value::Object(o) = w {
				for (k, x) in o {
					if let Some(x) = toml_value(x) {
						wt.insert(k, Item::Value(x));
					}
				}
			}
			list.push(wt);
		}
		t.insert("widgets", Item::ArrayOfTables(list));
	}
	t
}

/// The table `path` of `doc`, made (implicitly, without a header of its own) when it isn't there.
fn table_at<'a>(doc: &'a mut DocumentMut, path: &[&str]) -> Option<&'a mut Table> {
	let mut t = doc.as_table_mut();
	for key in path {
		if !t.contains_key(key) {
			let mut new = Table::new();
			new.set_implicit(true);
			t.insert(key, Item::Table(new));
		}
		t = t.get_mut(key)?.as_table_mut()?;
	}
	Some(t)
}

/// The editor's file's `text` with `draft` made in it: each dashboard's table replaced whole (or
/// taken out), each profile's keys the editor writes set (or taken out); the rest of the file, its
/// comments among it, as it was.
pub fn edited(text: &str, draft: &Draft) -> Result<String, String> {
	// The file's header is its own, kept above whatever table comes first.
	let body = text.strip_prefix(HEADER).unwrap_or(text);
	let mut doc: DocumentMut = body.parse().map_err(|e: toml_edit::TomlError| format!("the editor's file doesn't parse: {}", e.message()))?;
	for (id, def) in &draft.dashboards {
		let dashboards = table_at(&mut doc, &["dashboards"]).ok_or("dashboards isn't a table in the editor's file")?;
		dashboards.remove(id);
		if let Some(def) = def {
			let mut t = dashboard_table(def);
			t.set_implicit(false);
			dashboards.insert(id, Item::Table(t));
		}
	}
	for (id, part) in &draft.profiles {
		let profile = table_at(&mut doc, &["profiles", id]).ok_or("profiles isn't a table in the editor's file")?;
		for key in ["dashboards", "header"] {
			profile.remove(key);
		}
		let mut android_left = true;
		if let Some(a) = profile.get_mut("android").and_then(Item::as_table_mut) {
			for key in ANDROID_KEYS {
				a.remove(key);
			}
			android_left = !a.is_empty();
		}
		if !android_left {
			profile.remove("android");
		}
		if let Some(part) = part {
			for key in ["dashboards", "header"] {
				if let Some(v) = part.get(key).and_then(toml_value) {
					profile.insert(key, Item::Value(v));
				}
			}
			if let Some(Value::Object(a)) = part.get("android") {
				for key in ANDROID_KEYS {
					if let Some(v) = a.get(key).and_then(toml_value) {
						let android = profile.entry("android").or_insert_with(|| Item::Table(Table::new())).as_table_mut().ok_or("android isn't a table")?;
						android.insert(key, Item::Value(v));
					}
				}
			}
		}
		let only_tables = profile.iter().all(|(_, v)| v.is_table());
		profile.set_implicit(only_tables);
	}
	// Tables the draft emptied go.
	for top in ["dashboards", "profiles"] {
		if let Some(t) = doc.get_mut(top).and_then(Item::as_table_mut) {
			let empty: Vec<String> = t.iter().filter(|(_, v)| v.as_table().is_some_and(|x| x.is_empty())).map(|(k, _)| k.to_string()).collect();
			for k in empty {
				t.remove(&k);
			}
			if t.is_empty() {
				doc.remove(top);
			}
		}
	}
	Ok(format!("{HEADER}\n{}", doc.to_string().trim_start()))
}

/// What is wrong with the draft's shape before it is written: ids, and keys the editor doesn't write.
fn shape(draft: &Draft, file: &str) -> Vec<Mistake> {
	let mut out = Vec::new();
	let mut push = |path: String, message: String| out.push(Mistake { file: Some(file.to_string()), line: None, path, message });
	for (id, def) in &draft.dashboards {
		let path = settings::join("dashboards", id);
		if !settings::is_id(id) {
			push(path.clone(), format!("\"{id}\" isn't an id: 1 to 32 of a-z, 0-9, _ and -, starting with a letter or a digit"));
		}
		for key in def.iter().flat_map(|d| d.keys()).filter(|k| !DASHBOARD_KEYS.contains(&k.as_str())) {
			push(settings::join(&path, key), format!("a dashboard has no key {key} ({})", DASHBOARD_KEYS.join(", ")));
		}
	}
	for (id, part) in &draft.profiles {
		let path = settings::join("profiles", id);
		if !settings::is_id(id) {
			push(path.clone(), format!("\"{id}\" isn't an id"));
		}
		for key in part.iter().flat_map(|p| p.keys()).filter(|k| !PROFILE_KEYS.contains(&k.as_str())) {
			push(settings::join(&path, key), format!("the editor writes {} of a profile, not {key}", PROFILE_KEYS.join(", ")));
		}
		if let Some(Value::Object(a)) = part.as_ref().and_then(|p| p.get("android")) {
			for key in a.keys().filter(|k| !ANDROID_KEYS.contains(&k.as_str())) {
				push(settings::join(&settings::join(&path, "android"), key), format!("the editor writes {} of android, not {key}", ANDROID_KEYS.join(", ")));
			}
		}
	}
	out
}

/// Keys of a dashboard a file below the editor's sets, which the draft leaves out: a merge can't
/// take them away, so what is on screen wouldn't be what the settings say.
fn taken_out(lower: &[Source], draft: &Draft, shown: impl Fn(&str) -> String) -> Vec<Mistake> {
	let whence = settings::whence_of(lower, 3);
	let mut out = Vec::new();
	for (id, def) in &draft.dashboards {
		let Some(def) = def else { continue };
		for key in DASHBOARD_KEYS.iter().filter(|k| !def.contains_key(**k)) {
			let path = settings::join(&settings::join("dashboards", id), key);
			if let Some((i, line)) = whence.get(&path) {
				let file = shown(&lower[*i].name);
				out.push(Mistake {
					file: Some(file.clone()),
					line: Some(*line),
					path,
					message: format!("{key} is set in {file}:{line}; the editor can change it but not take it away"),
				});
			}
		}
	}
	out
}

/// The settings' sources with the editor's file made as `draft` says: what a preview reads and a
/// write checks. Mistakes of the draft's shape, or keys it would take away, come first.
pub fn sources_with_draft(hub: &Hub, mut sources: Vec<Source>, draft: &Draft) -> Result<Vec<Source>, Vec<Mistake>> {
	let file = editor_file(hub);
	let at = file.path.display().to_string();
	let mut mistakes = shape(draft, &file.name);
	let lower: Vec<Source> = sources.iter().filter(|s| s.name != at).cloned().collect();
	mistakes.extend(taken_out(&lower, draft, |n| files::shown_name(&hub.place, n)));
	if !mistakes.is_empty() {
		return Err(mistakes);
	}
	let text = sources.iter().find(|s| s.name == at).map(|s| s.text.clone()).unwrap_or_default();
	let new = edited(&text, draft).map_err(|m| vec![Mistake { file: Some(file.name.clone()), line: None, path: String::new(), message: m }])?;
	match sources.iter_mut().find(|s| s.name == at) {
		Some(s) => s.text = new,
		None => sources = crate::commands::sources_with(&hub.place, Some(&file.path), Some(new))?,
	}
	Ok(sources)
}

// ---- writing ------------------------------------------------------------------------------

/// Writes `draft` to the editor's file for `who`, as a save of the file is written (`base`, its
/// hash as the client read it).
async fn write_draft(hub: &Arc<Hub>, who: &Caller, what: String, draft: Draft, base: Option<String>) -> Response {
	if let Some(r) = editor_off(hub) {
		return r;
	}
	let file = editor_file(hub);
	let sources = match settings::read_files(&hub.place.files()) {
		Ok(s) => s,
		Err(m) => return bad_settings(&m),
	};
	let at = file.path.display().to_string();
	let new = match sources_with_draft(hub, sources, &draft) {
		Ok(s) => s.into_iter().find(|s| s.name == at).map(|s| s.text).unwrap_or_default(),
		Err(m) => {
			record(hub, who, what, ErrorCode::BadSettings.as_str(), None);
			return bad_settings(&m);
		}
	};
	let path = file.path.clone();
	files::write(hub, who, what, file, base, Some((path, new)), None).await
}

#[derive(Deserialize)]
pub struct DashboardRequest {
	id: String,
	dashboard: Map<String, Value>,
	base: Option<String>,
}

/// `POST /api/v1/admin/editor/dashboard`.
pub async fn dashboard(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Json(req): Json<DashboardRequest>) -> Response {
	let n = req.dashboard.get("widgets").and_then(Value::as_array).map_or(0, Vec::len);
	let what = format!("editor: dashboard {} ({n} widgets)", store::sanitize_name(&req.id));
	let mut draft = Draft::default();
	draft.dashboards.insert(req.id, Some(req.dashboard));
	write_draft(&hub, &who, what, draft, req.base).await
}

#[derive(Deserialize)]
pub struct ProfileRequest {
	id: String,
	profile: Map<String, Value>,
	base: Option<String>,
}

/// `POST /api/v1/admin/editor/profile`.
pub async fn profile(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Json(req): Json<ProfileRequest>) -> Response {
	let what = format!("editor: tabs of profile {}", store::sanitize_name(&req.id));
	let mut draft = Draft::default();
	draft.profiles.insert(req.id, Some(req.profile));
	write_draft(&hub, &who, what, draft, req.base).await
}

#[derive(Deserialize)]
pub struct RevertRequest {
	#[serde(default)]
	dashboard: Option<String>,
	#[serde(default)]
	profile: Option<String>,
	base: Option<String>,
}

/// `POST /api/v1/admin/editor/revert`: the editor's table taken out.
pub async fn revert(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Json(req): Json<RevertRequest>) -> Response {
	let mut draft = Draft::default();
	let what = match (&req.dashboard, &req.profile) {
		(Some(d), None) => {
			draft.dashboards.insert(d.clone(), None);
			format!("editor: dashboard {} back to the files", store::sanitize_name(d))
		}
		(None, Some(p)) => {
			draft.profiles.insert(p.clone(), None);
			format!("editor: profile {} back to the files", store::sanitize_name(p))
		}
		_ => return error_with(ErrorCode::BadParam, "dashboard"),
	};
	write_draft(&hub, &who, what, draft, req.base).await
}

#[derive(Deserialize)]
pub struct DeleteRequest {
	dashboard: String,
	base: Option<String>,
}

/// `POST /api/v1/admin/editor/delete`: a dashboard only the editor's file has, and its place among
/// the tabs the editor's file gives.
pub async fn delete(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Json(req): Json<DeleteRequest>) -> Response {
	let what = format!("editor: deleted dashboard {}", store::sanitize_name(&req.dashboard));
	let file = editor_file(&hub);
	let at = file.path.display().to_string();
	let sources = settings::read_files(&hub.place.files()).unwrap_or_default();
	let path = settings::join("dashboards", &req.dashboard);
	let elsewhere = sources.iter().filter(|s| s.name != at).any(|s| settings::lines_of(&s.text).contains_key(&path));
	if elsewhere {
		return error_with(ErrorCode::BadParam, "dashboard: another settings file has it too; Back to the files takes the editor's out");
	}
	let mut draft = Draft::default();
	draft.dashboards.insert(req.dashboard.clone(), None);
	// The editor's tabs without it.
	if let Some(s) = sources.iter().find(|s| s.name == at)
		&& let Ok(t) = s.text.parse::<toml::Table>()
		&& let Some(profiles) = t.get("profiles").and_then(toml::Value::as_table)
	{
		for (id, p) in profiles {
			let Some(tabs) = p.get("dashboards").and_then(toml::Value::as_array) else { continue };
			if !tabs.iter().any(|d| d.as_str() == Some(req.dashboard.as_str())) {
				continue;
			}
			let mut part = Map::new();
			part.insert("dashboards".into(), json!(tabs.iter().filter_map(toml::Value::as_str).filter(|d| *d != req.dashboard).collect::<Vec<_>>()));
			if let Some(h) = p.get("header").and_then(toml::Value::as_str) {
				part.insert("header".into(), json!(h));
			}
			if let Some(a) = p.get("android").and_then(toml::Value::as_table) {
				let android: Map<String, Value> = ANDROID_KEYS.iter().filter_map(|k| a.get(*k).and_then(toml::Value::as_str).map(|v| (k.to_string(), json!(v)))).collect();
				part.insert("android".into(), Value::Object(android));
			}
			draft.profiles.insert(id.clone(), Some(part));
		}
	}
	write_draft(&hub, &who, what, draft, req.base).await
}

// ---- the description ----------------------------------------------------------------------

/// A key as the editor's forms take it: its kind and what goes with it, its label and hint.
pub fn key_json(k: &Key) -> Value {
	let mut o = ty_json(&k.ty);
	o.insert("key".into(), json!(k.name));
	o.insert("label".into(), json!(settings::label(k.name)));
	o.insert("hint".into(), json!(settings::hint(k.name)));
	if k.required {
		o.insert("required".into(), json!(true));
	}
	Value::Object(o)
}

/// A type of key as the editor's forms take it (docs/protocol.md, "GET /api/v1/admin/editor").
fn ty_json(ty: &Ty) -> Map<String, Value> {
	let mut o = Map::new();
	let mut kind = |k: &str| {
		o.insert("kind".into(), json!(k));
	};
	match ty {
		Ty::Str => kind("string"),
		Ty::Text => kind("text"),
		Ty::Id => kind("id"),
		Ty::Address => kind("address"),
		Ty::Bool => kind("bool"),
		Ty::Num => kind("number"),
		Ty::Scalar => kind("scalar"),
		Ty::Int(lo, hi) => {
			kind("int");
			o.insert("min".into(), json!(lo));
			if *hi != i64::MAX {
				o.insert("max".into(), json!(hi));
			}
		}
		Ty::Enum(_, words) => {
			kind("enum");
			o.insert("choices".into(), json!(words));
		}
		Ty::Format => {
			kind("format");
			o.insert("choices".into(), json!(crate::format::FORMATS));
		}
		Ty::List(t) if matches!(**t, Ty::Widget) => kind("widgets"),
		Ty::List(t) => {
			kind("list");
			o.insert("of".into(), Value::Object(ty_json(t)));
		}
		Ty::Map(t) | Ty::IdMap(t) => {
			kind("map");
			o.insert("of".into(), Value::Object(ty_json(t)));
		}
		Ty::Rec(name) => {
			kind("record");
			o.insert("record".into(), json!(name));
		}
		Ty::Any(ts) => {
			kind("any");
			o.insert("of".into(), Value::Array(ts.iter().map(|t| Value::Object(ty_json(t))).collect()));
		}
		Ty::Widget => kind("widget"),
		Ty::Source => {
			kind("record");
			o.insert("record".into(), json!("source"));
		}
		Ty::Ref(to) => {
			kind("ref");
			o.insert("to".into(), json!(to));
		}
	}
	o
}

/// The records widgets' keys name, by name.
fn records_of(keys: &[Key], out: &mut BTreeMap<String, Value>) {
	fn walk(ty: &Ty, out: &mut BTreeMap<String, Value>) {
		match ty {
			Ty::Rec(name) if !out.contains_key(*name) => {
				let keys = settings::record(name);
				out.insert(name.to_string(), Value::Array(keys.iter().map(key_json).collect()));
				for k in &keys {
					walk(&k.ty, out);
				}
			}
			Ty::List(t) | Ty::Map(t) | Ty::IdMap(t) => walk(t, out),
			Ty::Any(ts) => ts.iter().for_each(|t| walk(t, out)),
			_ => {}
		}
	}
	for k in keys {
		walk(&k.ty, out);
	}
}

/// A widget type's name as a person reads it.
fn type_label(t: &str) -> String {
	match t {
		"toggle" => "Switch".into(),
		"select" => "Choice".into(),
		"log" => "Journal".into(),
		"bars" => "Bar chart".into(),
		other => settings::label(other),
	}
}

/// An example of a widget of type `t` to start from, with what this computer has.
fn example(t: &str, config: &settings::Config, actions: &[crate::actions::Spec]) -> Value {
	let action = |kinds: &[&str]| actions.iter().find(|a| kinds.contains(&a.kind) || kinds.is_empty()).map(|a| a.id.clone());
	let mut e = match t {
		"value" => json!({ "label": "Uptime", "value": "system.uptime" }),
		"gauge" => json!({ "label": "CPU", "value": "cpu.usage", "max": 100 }),
		"meter" => json!({ "label": "Memory", "value": "mem.percent", "max": 100 }),
		"status" => json!({ "label": "Power", "value": "power.on_ac", "map": { "true": "On AC", "false": "On battery" } }),
		"graph" => json!({ "label": "CPU", "values": ["cpu.usage"], "max": 100 }),
		"list" => json!({ "label": "System", "items": [{ "label": "OS", "value": "system.os" }, { "label": "Up", "value": "system.uptime" }] }),
		"bars" => json!({ "label": "Cores", "source": "cores", "field": "usage", "max": 100 }),
		"table" => json!({ "label": "Disks", "source": "disks", "columns": [{ "field": "mount", "label": "Mount" }, { "field": "percent", "label": "Used", "format": "percent" }] }),
		"button" => json!({ "label": "Run", "action": action(&["actions", "power", "profile"]) }),
		"toggle" => json!({ "label": "Switch", "action": action(&["actions"]) }),
		"select" => json!({ "label": "Profile", "value": "power.profile", "action": action(&["profile"]) }),
		"slider" => json!({ "label": "Volume", "action": action(&["media"]) }),
		"text" => json!({ "text": "Up {system.uptime}" }),
		"group" => json!({ "label": "More", "widgets": [{ "type": "value", "label": "OS", "value": "system.os" }] }),
		"media" => json!({ "label": "Media" }),
		"timeline" => json!({ "label": "Power", "source": config.sources.keys().next() }),
		"log" => json!({ "label": "Journal", "scope": "system", "level": "warning" }),
		_ => json!({}),
	};
	if let Some(o) = e.as_object_mut() {
		o.retain(|_, v| !v.is_null());
		let mut with_type = Map::new();
		with_type.insert("type".into(), json!(t));
		with_type.extend(std::mem::take(o));
		*o = with_type;
	}
	e
}

#[derive(Deserialize, Default)]
pub struct DescribeQuery {
	profile: Option<String>,
}

/// `GET /api/v1/admin/editor`.
pub async fn describe(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Query(q): Query<DescribeQuery>) -> Response {
	if let Some(r) = editor_off(&hub) {
		return r;
	}
	let config = hub.config();
	let file = editor_file(&hub);
	let at = file.path.display().to_string();
	let sources = settings::read_files(&hub.place.files()).unwrap_or_default();
	let raw = settings::read_merged(&sources).map(|(_, raw)| raw).unwrap_or_default();
	let text = sources.iter().find(|s| s.name == at).map(|s| s.text.clone());
	let why = files::read_only(&hub.place, &file.path);
	let lines: Vec<(String, bool, HashMap<String, usize>)> = sources
		.iter()
		.map(|s| (files::shown_name(&hub.place, &s.name), store::from_nix_store(std::path::Path::new(&s.name)), settings::lines_of(&s.text)))
		.collect();
	let from = |path: &str| -> Vec<Value> {
		lines.iter().filter_map(|(f, nix, l)| l.get(path).map(|line| json!({ "file": f, "line": line, "nix": nix }))).collect()
	};
	let editor_has = |path: &str| text.as_deref().is_some_and(|t| settings::lines_of(t).contains_key(path));
	let frame = hub.frame.borrow().clone();
	let actions: Vec<crate::actions::Spec> = crate::actions::available(&config, &frame.caps).into_iter().filter(|a| a.role <= who.role).collect();

	let mut records = BTreeMap::new();
	let types: Vec<Value> = settings::WIDGET_TYPES
		.iter()
		.map(|t| {
			let keys = settings::type_keys(t);
			records_of(&keys, &mut records);
			json!({
				"type": t,
				"label": type_label(t),
				"span": if settings::WIDE.contains(t) { 4 } else { 2 },
				"keys": keys.iter().map(key_json).collect::<Vec<_>>(),
				"example": example(t, &config, &actions),
			})
		})
		.collect();
	let common: Vec<Key> = settings::common_keys().into_iter().filter(|k| k.name != "type").collect();
	records_of(&common, &mut records);
	let dashboard_keys = settings::record("dashboard");
	records_of(&dashboard_keys, &mut records);

	let empty = Map::new();
	let raw_dashboards = raw.get("dashboards").and_then(Value::as_object).unwrap_or(&empty);
	// What the files below the editor's set: keys it may change but not take away.
	let below = settings::whence_of(&sources.iter().filter(|s| s.name != at).cloned().collect::<Vec<_>>(), 3);
	let dashboards: Vec<Value> = raw_dashboards
		.iter()
		.map(|(id, def)| {
			let path = settings::join("dashboards", id);
			let from = from(&path);
			let editor = editor_has(&path);
			let fixed: Map<String, Value> = DASHBOARD_KEYS
				.iter()
				.filter_map(|k| {
					let (i, line) = below.get(&settings::join(&path, k))?;
					Some((k.to_string(), json!({ "file": files::shown_name(&hub.place, &sources.iter().filter(|s| s.name != at).nth(*i)?.name), "line": line })))
				})
				.collect();
			json!({
				"id": id,
				"title": def.get("title"),
				"def": def,
				"from": from,
				"below": fixed,
				"editor": editor,
				"own": editor && from.len() == 1,
			})
		})
		.collect();
	let clients = store::load_clients_in(&hub.dir).unwrap_or_default();
	let raw_profiles = raw.get("profiles").and_then(Value::as_object).unwrap_or(&empty);
	let profiles: Vec<Value> = raw_profiles
		.keys()
		.map(|id| {
			let p = config.profile(id);
			let path = settings::join("profiles", id);
			let users: Vec<&str> = clients
				.iter()
				.filter(|c| config.profile(c.profile.as_deref().unwrap_or(&config.default_profile)).id == *id)
				.map(|c| c.id.as_str())
				.collect();
			json!({
				"id": id,
				"name": p.name,
				"def": { "dashboards": p.dashboards, "header": p.header, "android": { "widget": p.widget, "tiles": p.tiles, "card": p.card } },
				"from": from(&path),
				"editor": editor_has(&path),
				"clients": users,
			})
		})
		.collect();
	let mine = q.profile.unwrap_or_else(|| config.profile(who.profile.as_deref().unwrap_or(&config.default_profile)).id);
	let collections: Vec<Value> = crate::values::COLLECTIONS
		.iter()
		.filter(|c| c.role <= who.role)
		.map(|c| json!({ "name": c.name, "fields": c.fields.iter().map(|f| f.0).collect::<Vec<_>>() }))
		.collect();
	Json(json!({
		"file": file.name,
		"hash": text.as_deref().map(|t| files::hash(t.as_bytes())),
		"writable": why.is_none(),
		"why": why,
		"profile": mine,
		"dashboard": { "keys": dashboard_keys.iter().map(key_json).collect::<Vec<_>>() },
		"profile_keys": settings::record("profile").iter().filter(|k| PROFILE_KEYS.contains(&k.name) && k.name != "android").map(key_json).collect::<Vec<_>>(),
		"android_keys": settings::record("android").iter().filter(|k| ANDROID_KEYS.contains(&k.name)).map(key_json).collect::<Vec<_>>(),
		"templates": raw.get("templates").cloned().unwrap_or_else(|| json!({})),
		"common": common.iter().map(key_json).collect::<Vec<_>>(),
		"types": types,
		"records": records,
		"values": values_now(&hub, who.role),
		"collections": collections,
		"actions": actions.iter().map(|a| json!({ "id": a.id, "name": a.name, "kind": a.kind })).collect::<Vec<_>>(),
		"links": config.links.iter().map(|(id, l)| json!({ "id": id, "name": l.name })).collect::<Vec<_>>(),
		"sources": sources_of(&config),
		"dashboards": dashboards,
		"profiles": profiles,
	}))
	.into_response()
}

#[cfg(test)]
mod tests {
	use super::*;

	fn draft(json: Value) -> Draft {
		serde_json::from_value(json).unwrap()
	}

	#[test]
	fn a_dashboard_written_whole_and_taken_out() {
		let d = draft(json!({ "dashboards": { "mine": { "title": "Mine", "icon": "gauge", "widgets": [
			{ "type": "gauge", "label": "CPU", "value": "cpu.usage", "max": 100, "span": 2 },
			{ "type": "group", "label": "More", "widgets": [{ "type": "value", "value": "system.os" }], "future_key": { "a": 1 } }
		] } } }));
		let text = edited("", &d).unwrap();
		assert!(text.starts_with(HEADER), "{text}");
		assert!(text.contains("[dashboards.mine]\ntitle = \"Mine\"\nicon = \"gauge\"\n"), "{text}");
		assert!(text.contains("[[dashboards.mine.widgets]]\ntype = \"gauge\""), "{text}");
		// A key no form knows is kept as it came.
		assert!(text.contains("future_key = { a = 1 }"), "{text}");
		let parsed: toml::Table = text.parse().unwrap();
		assert_eq!(parsed["dashboards"]["mine"]["widgets"].as_array().unwrap().len(), 2);
		// Another write of it replaces it whole, and a comment by hand stays.
		let by_hand = format!("{text}\n# mine, by hand\n[dashboards.other]\ntitle = \"Other\"\n");
		let again = edited(&by_hand, &draft(json!({ "dashboards": { "mine": { "title": "Mine 2", "widgets": [] } } }))).unwrap();
		assert!(again.contains("# mine, by hand") && again.contains("[dashboards.other]") && again.contains("title = \"Mine 2\""), "{again}");
		assert!(!again.contains("cpu.usage"), "{again}");
		// Back to the files: the editor's table goes.
		let back = edited(&again, &draft(json!({ "dashboards": { "mine": null } }))).unwrap();
		assert!(!back.contains("[dashboards.mine]") && back.contains("[dashboards.other]"), "{back}");
	}

	#[test]
	fn a_profile_s_tabs_and_android_dashboards() {
		let d = draft(json!({ "profiles": { "pixel-9": { "dashboards": ["overview", "mine"], "header": "header", "android": { "widget": "phone", "tiles": "", "card": "" } } } }));
		let text = edited("", &d).unwrap();
		assert!(text.contains("[profiles.pixel-9]\ndashboards = [\"overview\", \"mine\"]\nheader = \"header\""), "{text}");
		assert!(text.contains("[profiles.pixel-9.android]\nwidget = \"phone\""), "{text}");
		let back = edited(&text, &draft(json!({ "profiles": { "pixel-9": null } }))).unwrap();
		assert!(!back.contains("pixel-9"), "{back}");
	}

	#[test]
	fn the_editor_writes_no_command() {
		let d = draft(json!({
			"dashboards": { "mine": { "title": "Mine", "command": ["sh", "-c", "true"] }, "Bad Id": { "title": "x" } },
			"profiles": { "p": { "use": "admin", "android": { "dynamic_color": false } } }
		}));
		let m = shape(&d, "elamond.d/zz-editor.toml");
		let paths: Vec<&str> = m.iter().map(|x| x.path.as_str()).collect();
		assert!(paths.contains(&"dashboards.mine.command"), "{paths:?}");
		assert!(paths.contains(&"dashboards.\"Bad Id\""), "{paths:?}");
		assert!(paths.contains(&"profiles.p.use"), "{paths:?}");
		assert!(paths.contains(&"profiles.p.android.dynamic_color"), "{paths:?}");
		// And the TOML it would make has no table but dashboards and profiles anyway.
		let t: toml::Table = edited("", &draft(json!({ "dashboards": { "mine": { "title": "Mine" } } }))).unwrap().parse().unwrap();
		assert_eq!(t.keys().collect::<Vec<_>>(), ["dashboards"]);
	}

	#[test]
	fn a_key_from_below_is_changed_but_not_taken_away() {
		let lower = [Source::new("/c/elamond.d/dashboards.toml", "[dashboards.cpu]\ntitle = \"CPU\"\nicon = \"chip\"\nwidgets = []\n")];
		let d = draft(json!({ "dashboards": { "cpu": { "title": "My CPU", "widgets": [] } } }));
		let m = taken_out(&lower, &d, |n| n.trim_start_matches("/c/").to_string());
		assert_eq!(m.len(), 1);
		assert_eq!(m[0].path, "dashboards.cpu.icon");
		assert!(m[0].message.contains("elamond.d/dashboards.toml:3"), "{}", m[0].message);
		let d = draft(json!({ "dashboards": { "cpu": { "title": "My CPU", "icon": "gauge", "widgets": [] } } }));
		assert!(taken_out(&lower, &d, |n| n.to_string()).is_empty());
	}

	#[test]
	fn every_widget_key_has_a_form_kind_and_words() {
		for t in settings::WIDGET_TYPES {
			for k in settings::type_keys(t).iter().chain(settings::common_keys().iter()) {
				let j = key_json(k);
				assert!(j["kind"].is_string(), "{t}.{}", k.name);
				assert!(!j["label"].as_str().unwrap().is_empty(), "{t}.{}", k.name);
				assert!(!j["hint"].as_str().unwrap().is_empty(), "{t}.{} has no hint", k.name);
			}
		}
	}
}
