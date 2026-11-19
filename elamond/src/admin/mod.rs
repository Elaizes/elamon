//! The admin API (docs/protocol.md, "Admin API"): for the role `[web] role` names and above, by a
//! browser's cookie or a phone's token alike. What the service is and how it runs, the settings as
//! merged and where each part comes from, the clients and their roles, pairing from the page,
//! `control off`; the settings files (`files`), a preview of a draft (`preview`) and the dashboard
//! editor (`editor`). Every change is a line in the activity log.

// A handler's refusal is its answer, built where the refusal is found and returned as is.
#![allow(clippy::result_large_err)]

pub mod editor;
pub mod files;
pub mod preview;
#[cfg(test)]
mod tests;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{DefaultBodyLimit, Extension, Path, Query, Request, State};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::access::{Caller, Role};
use crate::activity::{self, Entry};
use crate::consts::limit;
use crate::errors::ErrorCode;
use crate::server::{error, error_with, Hub};
use crate::settings::{self, WebDef};
use crate::store;

/// The admin API's routes, behind its gate; the client's token was checked before.
pub fn routes(hub: Arc<Hub>) -> Router<Arc<Hub>> {
	let small = || DefaultBodyLimit::max(limit::BODY_MAX);
	let big = || DefaultBodyLimit::max(limit::SETTINGS_BODY_MAX);
	Router::new()
		.route("/api/v1/admin/state", get(state))
		.route("/api/v1/admin/settings", get(settings_view))
		.route("/api/v1/admin/clients", get(clients))
		.route("/api/v1/admin/clients/{id}/role", post(client_role).layer(small()))
		.route("/api/v1/admin/clients/{id}/profile", post(client_profile).layer(small()))
		.route("/api/v1/admin/clients/{id}/revoke", post(client_revoke).layer(small()))
		.route("/api/v1/admin/pairing", get(pairing).post(pairing_new).layer(small()))
		.route("/api/v1/admin/pairing/answer", post(pairing_answer).layer(small()))
		.route("/api/v1/admin/pairing/withdraw", post(pairing_withdraw).layer(small()))
		.route("/api/v1/admin/control", post(control).layer(small()))
		.route("/api/v1/admin/activity", get(activity_list))
		.route("/api/v1/admin/events", get(events_list))
		.route("/api/v1/admin/values", get(values))
		.route("/api/v1/admin/file", get(files::read))
		.route("/api/v1/admin/file/backup", get(files::backup))
		.route("/api/v1/admin/file/check", post(files::check).layer(big()))
		.route("/api/v1/admin/file/save", post(files::save).layer(big()))
		.route("/api/v1/admin/file/restore", post(files::restore).layer(small()))
		.route("/api/v1/admin/file/rename", post(files::rename).layer(small()))
		.route("/api/v1/admin/file/delete", post(files::delete).layer(small()))
		.route("/api/v1/admin/preview", post(preview::preview).layer(big()))
		.route("/api/v1/admin/editor", get(editor::describe))
		.route("/api/v1/admin/editor/dashboard", post(editor::dashboard).layer(big()))
		.route("/api/v1/admin/editor/profile", post(editor::profile).layer(small()))
		.route("/api/v1/admin/editor/revert", post(editor::revert).layer(small()))
		.route("/api/v1/admin/editor/delete", post(editor::delete).layer(small()))
		.route_layer(middleware::from_fn_with_state(hub, gate))
}

/// The admin API is for `[web] role` and above while `[web] admin` is on: 403 `admin_off`
/// otherwise for every role, 403 `not_allowed` (in the activity log) for a role below.
async fn gate(State(hub): State<Arc<Hub>>, req: Request, next: Next) -> Response {
	let Some(who) = req.extensions().get::<Caller>().cloned() else { return error(ErrorCode::Unauthorized) };
	let web = hub.config().web.clone();
	if !web.admin {
		return error(ErrorCode::AdminOff);
	}
	if who.role < web.role {
		record(&hub, &who, format!("admin: {} {}", req.method(), req.uri().path()), ErrorCode::NotAllowed.as_str(), None);
		return error(ErrorCode::NotAllowed);
	}
	next.run(req).await
}

/// `features` a client of `role` gets besides the computer's: `admin` when the admin API is open to
/// it, `editor` when the dashboard editor is on too.
pub fn features_for(role: Role, web: &WebDef) -> Vec<&'static str> {
	let mut out = Vec::new();
	if web.admin && role >= web.role {
		out.push("admin");
		if web.editor {
			out.push("editor");
		}
	}
	out
}

/// A line in the activity log under the client that asked.
pub(crate) fn record(hub: &Hub, who: &Caller, what: String, result: &str, message: Option<String>) {
	hub.activity.record(Entry {
		t: activity::now_ms(),
		phone: who.name.clone(),
		client_id: who.id.clone(),
		what: activity::clean(&what),
		result: result.to_string(),
		message: message.map(|m| activity::clean(&m)),
		family: (who.family != who.id).then(|| who.family.clone()),
	});
}

/// Why the settings may not be written from here now, if they may not: `[web] write = false`, or
/// `elamond control off`.
pub(crate) fn writes_refused(hub: &Hub) -> Option<Response> {
	if !hub.config().web.write {
		return Some(error_with(ErrorCode::WritesOff, "settings"));
	}
	clients_refused(hub)
}

/// Why clients and pairing may not be changed from here now: `elamond control off`.
pub(crate) fn clients_refused(hub: &Hub) -> Option<Response> {
	store::control_held_off(&hub.dir).then(|| error_with(ErrorCode::WritesOff, "held_off"))
}

/// A refusal with fields of its own beside `error`: `mistakes`, `hash`.
pub(crate) fn error_body(code: ErrorCode, extra: Map<String, Value>) -> Response {
	let mut body = Map::new();
	body.insert("error".into(), json!(code.as_str()));
	body.extend(extra);
	(code.status(), Json(Value::Object(body))).into_response()
}

/// 422 `bad_settings` with each mistake.
pub(crate) fn bad_settings(mistakes: &[settings::Mistake]) -> Response {
	let mut extra = Map::new();
	extra.insert("mistakes".into(), mistakes_json(mistakes));
	error_body(ErrorCode::BadSettings, extra)
}

pub(crate) fn mistakes_json(mistakes: &[settings::Mistake]) -> Value {
	Value::Array(mistakes.iter().map(|m| json!({ "file": m.file, "line": m.line, "path": m.path, "message": m.message })).collect())
}

// ---- the service -------------------------------------------------------------------------

/// What `elamond setup --check` says, read at most once a minute: it looks at the unit, the
/// firewall, polkit and the buses, and asks the service's own port.
/// What `elamond setup --check` said, and when.
type SetupSaid = Option<(Instant, Vec<(bool, String)>)>;

async fn setup_lines() -> Vec<(bool, String)> {
	static LAST: Mutex<SetupSaid> = Mutex::new(None);
	if let Some((at, lines)) = LAST.lock().unwrap_or_else(|p| p.into_inner()).as_ref()
		&& at.elapsed() < crate::consts::cadence::ADMIN_SETUP_CHECK
	{
		return lines.clone();
	}
	let lines = match tokio::time::timeout(Duration::from_secs(15), tokio::task::spawn_blocking(crate::setup::check_lines)).await {
		Ok(Ok(l)) => l,
		_ => vec![(false, "elamond setup --check didn't finish in time".into())],
	};
	*LAST.lock().unwrap_or_else(|p| p.into_inner()) = Some((Instant::now(), lines.clone()));
	lines
}

/// `GET /api/v1/admin/state`.
async fn state(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>) -> Response {
	let config = hub.config();
	let frame = hub.frame.borrow().clone();
	let mut features = crate::server::features_of(&frame.features, config.control);
	features.extend(features_for(who.role, &config.web));
	features.sort_unstable();
	features.dedup();
	let setup = setup_lines().await;
	let cert = hub.web_cert.read().ok().and_then(|c| c.clone());
	let broken = hub.settings_broken();
	let place = hub.place.clone();
	let editor_file = config.web.editor_file.clone();
	let files = tokio::task::spawn_blocking(move || files::list(&place, &editor_file)).await.unwrap_or_default();
	Json(json!({
		"service": {
			"id": hub.id,
			"name": hub.name,
			"version": env!("CARGO_PKG_VERSION"),
			"started": hub.started,
			"features": features,
			"fingerprint": hub.fingerprint(),
			"listening": hub.listening().iter().map(ToString::to_string).collect::<Vec<_>>(),
			"endpoints": hub.endpoints().to_vec(),
		},
		"control": {
			"enabled": hub.control_on(),
			"off_reason": hub.off_reason(),
			"held_off_since": store::control_off_since(&hub.dir),
		},
		"web": {
			"page": config.web.page,
			"admin": config.web.admin,
			"write": config.web.write,
			"editor": config.web.editor,
			"role": config.web.role,
			"editor_file": format!("{}/{}", store::DROPIN_DIR, config.web.editor_file),
			"certificate": cert.map(|c| json!({ "name": c.name, "ok": c.ok, "until": c.until, "message": c.message })),
		},
		"settings": { "sound": broken.is_none(), "message": broken, "generation": hub.config_gen() },
		"setup": setup.into_iter().map(|(ok, text)| json!({ "ok": ok, "text": text })).collect::<Vec<_>>(),
		"files": files,
		"you": { "id": who.id, "role": who.role },
	}))
	.into_response()
}

/// `GET /api/v1/admin/settings`: the settings as merged, where each part of them came from, what
/// differs from the defaults and what `elamond check` warns of.
async fn settings_view(State(hub): State<Arc<Hub>>) -> Response {
	let place = hub.place.clone();
	let view = tokio::task::spawn_blocking(move || settings_of(&place)).await.unwrap_or_else(|e| json!({ "error": e.to_string() }));
	Json(view).into_response()
}

/// The settings view of `place`'s files.
pub(crate) fn settings_of(place: &store::Place) -> Value {
	let sources = match settings::read_files(&place.files()) {
		Ok(s) => s,
		Err(m) => return json!({ "files": [], "settings": null, "whence": {}, "changes": [], "warnings": [], "mistakes": mistakes_json(&m) }),
	};
	let names: Vec<String> = sources.iter().map(|s| files::shown_name(place, &s.name)).collect();
	let whence = settings::whence_of(&sources, 3);
	let whence_json: Map<String, Value> = whence.iter().map(|(p, (i, l))| (p.clone(), json!([i, l]))).collect();
	let merged = settings::read_merged(&sources);
	let (config, raw) = match merged {
		Ok(m) => m,
		Err(m) => {
			let mistakes: Vec<settings::Mistake> = m.into_iter().map(|mut x| {
				x.file = x.file.map(|f| files::shown_name(place, &f));
				x
			}).collect();
			return json!({ "files": names, "settings": null, "whence": whence_json, "changes": [], "warnings": [], "mistakes": mistakes_json(&mistakes) });
		}
	};
	let defaults = crate::commands::as_json(settings::DEFAULTS).unwrap_or_default();
	let at = |path: &str| {
		let mut p = path.to_string();
		loop {
			if let Some((i, l)) = whence.get(&p) {
				return (Some(names[*i].clone()), Some(*l));
			}
			match p.rfind('.') {
				Some(cut) => p.truncate(cut),
				None => return (None, None),
			}
		}
	};
	let mut changes = Vec::new();
	for c in crate::commands::diff(&raw, &defaults) {
		if c.unit == "(top level)" {
			for k in &c.keys {
				let (file, line) = at(k);
				changes.push(json!({ "kind": c.kind.word(), "path": k, "keys": [], "file": file, "line": line }));
			}
		} else {
			let (file, line) = if c.kind == crate::commands::Kind::Removed { (None, None) } else { at(&c.unit) };
			changes.push(json!({ "kind": c.kind.word(), "path": c.unit, "keys": c.keys, "file": file, "line": line }));
		}
	}
	json!({
		"files": names,
		"settings": raw,
		"whence": whence_json,
		"changes": changes,
		"warnings": settings::warnings(&config),
		"mistakes": [],
	})
}

// ---- clients ------------------------------------------------------------------------------

/// `GET /api/v1/admin/clients`.
async fn clients(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>) -> Response {
	let list = match store::load_clients_in(&hub.dir) {
		Ok(l) => l,
		Err(e) => return error_with(ErrorCode::Storage, e.to_string()),
	};
	let config = hub.config();
	let rows: Vec<Value> = list
		.iter()
		.map(|c| {
			let kind = if c.web { "web" } else { "android" };
			let added: Vec<&str> = store::added_with(&list, &c.id).iter().map(|a| a.id.as_str()).collect();
			let profile = config.profile(c.profile.as_deref().unwrap_or(&config.default_profile)).id;
			json!({
				"id": c.id, "name": c.name, "role": c.role, "profile": profile, "own_profile": c.own_profile, "kind": kind,
				"app": c.app, "address": c.address, "seen": c.seen, "created": c.created, "from": c.from, "added": added,
				"you": c.id == who.id,
			})
		})
		.collect();
	let can_give: Vec<Role> = Role::ALL.into_iter().filter(|r| *r <= who.role).collect();
	Json(json!({ "clients": rows, "roles": Role::ALL, "can_give": can_give, "profiles": config.profiles.keys().collect::<Vec<_>>() })).into_response()
}

#[derive(Deserialize)]
struct RoleRequest {
	role: String,
	#[serde(default)]
	with_added: bool,
}

#[derive(Deserialize)]
struct ProfileRequest {
	profile: Option<String>,
	#[serde(default)]
	with_added: bool,
}

#[derive(Deserialize, Default)]
struct RevokeRequest {
	#[serde(default)]
	with_added: bool,
}

/// Changes the clients `id` names (and those added with its key, with `with_added`) under the lock
/// of clients.json, as `elamond clients` does: `change` returns whether it changed one.
fn change_clients(hub: &Hub, id: &str, with_added: bool, mut change: impl FnMut(&mut store::Client) -> bool) -> Result<Vec<String>, ErrorCode> {
	let _lock = store::lock_clients(&hub.dir).map_err(|_| ErrorCode::Storage)?;
	let mut list = store::load_clients_in(&hub.dir).map_err(|_| ErrorCode::Storage)?;
	let (chosen, _) = crate::choose(&list, id, with_added).map_err(|_| ErrorCode::NotFound)?;
	let mut changed = Vec::new();
	for c in list.iter_mut().filter(|c| chosen.contains(&c.id)) {
		if change(c) {
			changed.push(c.id.clone());
		}
	}
	if !changed.is_empty() {
		store::save_clients_in(&hub.dir, &list).map_err(|_| ErrorCode::Storage)?;
	}
	Ok(changed)
}

/// `POST /api/v1/admin/clients/{id}/role`: a role up to the asker's own.
async fn client_role(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Path(id): Path<String>, Json(req): Json<RoleRequest>) -> Response {
	let what = format!("role of {}: {}", store::sanitize_name(&id), store::sanitize_name(&req.role));
	let answer = (|| {
		if let Some(r) = clients_refused(&hub) {
			return Err(r);
		}
		let Some(role) = Role::parse(&req.role) else { return Err(error_with(ErrorCode::BadParam, "role")) };
		if role > who.role {
			return Err(error(ErrorCode::NotAllowed));
		}
		let changed = change_clients(&hub, &id, req.with_added, |c| std::mem::replace(&mut c.role, role) != role).map_err(error)?;
		Ok(changed)
	})();
	done(&hub, &who, what, answer)
}

/// `POST /api/v1/admin/clients/{id}/profile`: a profile the settings have, or `null` for the default.
async fn client_profile(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Path(id): Path<String>, Json(req): Json<ProfileRequest>) -> Response {
	let what = format!("profile of {}: {}", store::sanitize_name(&id), req.profile.as_deref().map(store::sanitize_name).unwrap_or_else(|| "default".into()));
	let answer = (|| {
		if let Some(r) = clients_refused(&hub) {
			return Err(r);
		}
		let config = hub.config();
		if let Some(p) = &req.profile
			&& !config.profiles.contains_key(p)
		{
			return Err(error_with(ErrorCode::BadParam, "profile"));
		}
		let profile = req.profile.clone();
		change_clients(&hub, &id, req.with_added, |c| std::mem::replace(&mut c.profile, profile.clone()) != profile).map_err(error)
	})();
	done(&hub, &who, what, answer)
}

/// `POST /api/v1/admin/clients/{id}/revoke`: unpaired as `elamond revoke` does it; a browser that
/// revokes itself loses its cookie.
async fn client_revoke(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Path(id): Path<String>, body: Option<Json<RevokeRequest>>) -> Response {
	let req = body.map(|b| b.0).unwrap_or_default();
	let what = format!("revoke {}", store::sanitize_name(&id));
	if let Some(r) = clients_refused(&hub) {
		record(&hub, &who, what, ErrorCode::WritesOff.as_str(), None);
		return r;
	}
	let (dir, place) = (hub.dir.clone(), hub.place.clone());
	let (target, with_added) = (id.clone(), req.with_added);
	let gone = tokio::task::spawn_blocking(move || -> Result<Vec<store::Client>, ErrorCode> {
		let _lock = store::lock_clients(&dir).map_err(|_| ErrorCode::Storage)?;
		let list = store::load_clients_in(&dir).map_err(|_| ErrorCode::Storage)?;
		let (chosen, _) = crate::choose(&list, &target, with_added).map_err(|_| ErrorCode::NotFound)?;
		let keep: Vec<store::Client> = list.iter().filter(|c| !chosen.contains(&c.id)).cloned().collect();
		store::save_clients_in(&dir, &keep).map_err(|_| ErrorCode::Storage)?;
		let gone: Vec<store::Client> = list.into_iter().filter(|c| chosen.contains(&c.id)).collect();
		for c in &gone {
			crate::phones::release(&place, c, &keep, store::now(), false);
		}
		Ok(gone)
	})
	.await
	.unwrap_or(Err(ErrorCode::Storage));
	let ids: Result<Vec<String>, Response> = gone.map(|g| g.iter().map(|c| c.id.clone()).collect()).map_err(error);
	let itself = ids.as_ref().is_ok_and(|i| i.contains(&who.id));
	if let Ok(i) = &ids {
		eprintln!("admin: \"{}\" ({}) unpaired {}", who.name, who.id, i.join(", "));
	}
	let mut res = done(&hub, &who, what, ids);
	if itself && who.web {
		res.headers_mut().append(axum::http::header::SET_COOKIE, crate::web::clear_cookie());
	}
	res
}

/// The answer to a change of clients, and its line in the activity log.
fn done(hub: &Hub, who: &Caller, what: String, answer: Result<Vec<String>, Response>) -> Response {
	match answer {
		Ok(changed) => {
			let note = (!changed.is_empty()).then(|| changed.join(", "));
			record(hub, who, what, "ok", note);
			Json(json!({ "ok": true, "changed": changed })).into_response()
		}
		Err(r) => {
			record(hub, who, what, &result_of(&r), None);
			r
		}
	}
}

/// The code an error answer carries, for the activity log.
fn result_of(r: &Response) -> String {
	match r.status().as_u16() {
		400 => "bad_param",
		403 => "refused",
		404 => "not_found",
		409 => "changed_on_disk",
		422 => "bad_settings",
		_ => "failed",
	}
	.to_string()
}

// ---- pairing from the page ----------------------------------------------------------------

#[derive(Deserialize)]
struct PairingRequest {
	#[serde(default)]
	role: Option<String>,
	#[serde(default)]
	profile: Option<String>,
}

/// `POST /api/v1/admin/pairing`: a code, in place of any pending one, that waits to be let in.
async fn pairing_new(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Json(req): Json<PairingRequest>) -> Response {
	let config = hub.config();
	let role = match req.role.as_deref() {
		Some(r) => match Role::parse(r) {
			Some(r) => r,
			None => return error_with(ErrorCode::BadParam, "role"),
		},
		None => config.default_role,
	};
	let what = format!("pairing code for {}", role.as_str());
	if let Some(r) = clients_refused(&hub) {
		record(&hub, &who, what, ErrorCode::WritesOff.as_str(), None);
		return r;
	}
	if role > who.role {
		record(&hub, &who, what, ErrorCode::NotAllowed.as_str(), None);
		return error(ErrorCode::NotAllowed);
	}
	if let Some(p) = &req.profile
		&& !config.profiles.contains_key(p)
	{
		return error_with(ErrorCode::BadParam, "profile");
	}
	match store::new_pairing_from_page(&hub.dir, role, req.profile.as_deref()) {
		Ok(_) => {
			eprintln!("pairing: \"{}\" ({}) made a code from the page for {}", who.name, who.id, role.as_str());
			record(&hub, &who, what, "ok", None);
			pairing_json(&hub)
		}
		Err(e) => error_with(ErrorCode::Storage, e.to_string()),
	}
}

/// `GET /api/v1/admin/pairing`: the pending code, its QR codes and the client waiting with it.
async fn pairing(State(hub): State<Arc<Hub>>) -> Response {
	pairing_json(&hub)
}

fn pairing_json(hub: &Hub) -> Response {
	let Some(p) = store::load_pairing(&hub.dir) else { return Json(json!({ "code": null })).into_response() };
	let config = hub.config();
	let endpoints = hub.endpoints();
	let hosts: Vec<String> = endpoints.iter().take(crate::consts::protocol::PAIR_HOSTS_MAX).cloned().collect();
	let app_link = crate::pairing_link(&hub.id, &hub.name, config.port, &hub.fingerprint(), &p.code, &hosts);
	let web_link = hosts.first().map(|h| format!("https://{}:{}/#pair={}", bracketed(h), config.port, p.code));
	let ask = p.ask.as_ref().filter(|a| a.allow.is_none()).map(|a| {
		json!({ "id": a.id, "name": a.name, "address": a.address, "at": a.at, "replaces": a.replaces.as_ref().map(|r| json!({ "id": r.id, "name": r.name, "profile": r.profile })), "offer": a.offer })
	});
	Json(json!({
		"code": p.code,
		"expires": p.expires,
		"role": p.role,
		"profile": p.profile,
		"from_page": p.from_page,
		"app": app_link.as_deref().map(qr_json),
		"web": web_link.as_deref().map(qr_json),
		"ask": ask,
	}))
	.into_response()
}

/// An address as it goes in a URL: an IPv6 one in brackets.
fn bracketed(host: &str) -> String {
	if host.contains(':') && !host.starts_with('[') { format!("[{host}]") } else { host.to_string() }
}

/// A link and its QR code's modules, row by row, `1` dark.
fn qr_json(link: &str) -> Value {
	match qrcode::QrCode::new(link.as_bytes()) {
		Ok(code) => {
			let size = code.width();
			let modules: String = code.to_colors().iter().map(|c| if *c == qrcode::Color::Dark { '1' } else { '0' }).collect();
			json!({ "link": link, "qr": { "size": size, "modules": modules } })
		}
		Err(_) => json!({ "link": link, "qr": null }),
	}
}

#[derive(Deserialize)]
struct AnswerRequest {
	ask: String,
	allow: bool,
	#[serde(default)]
	take: Option<bool>,
}

/// `POST /api/v1/admin/pairing/answer`: lets the waiting client in, or turns it away.
async fn pairing_answer(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Json(req): Json<AnswerRequest>) -> Response {
	let what = format!("pairing: {} {}", if req.allow { "let in" } else { "turned away" }, store::sanitize_name(&req.ask));
	if let Some(r) = clients_refused(&hub) {
		record(&hub, &who, what, ErrorCode::WritesOff.as_str(), None);
		return r;
	}
	let Some(p) = store::load_pairing(&hub.dir).filter(|p| p.ask.as_ref().is_some_and(|a| a.id == req.ask && a.allow.is_none())) else {
		return error(ErrorCode::NotFound);
	};
	match store::answer_pairing(&hub.dir, &p.code, &req.ask, req.allow, req.take) {
		Ok(()) => {
			record(&hub, &who, what, "ok", None);
			Json(json!({ "ok": true })).into_response()
		}
		Err(e) => error_with(ErrorCode::Storage, e.to_string()),
	}
}

/// `POST /api/v1/admin/pairing/withdraw`.
async fn pairing_withdraw(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>) -> Response {
	if let Some(p) = store::read_pairing(&hub.dir) {
		store::clear_pairing_code(&hub.dir, &p.code);
		record(&hub, &who, "pairing code withdrawn".into(), "ok", None);
	}
	Json(json!({ "ok": true })).into_response()
}

// ---- control ------------------------------------------------------------------------------

#[derive(Deserialize)]
struct ControlRequest {
	on: bool,
}

/// `POST /api/v1/admin/control`: off, as `elamond control off`; on only at the computer.
async fn control(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Json(req): Json<ControlRequest>) -> Response {
	if req.on {
		record(&hub, &who, "control on".into(), ErrorCode::AtComputer.as_str(), None);
		return error(ErrorCode::AtComputer);
	}
	if let Err(e) = store::set_control_off(&hub.dir, true) {
		return error_with(ErrorCode::Storage, e.to_string());
	}
	hub.sync_control_off();
	eprintln!("control: held off from the admin panel by \"{}\" ({}); `elamond control on` at the computer lets go of it", who.name, who.id);
	record(&hub, &who, "control off".into(), "ok", None);
	Json(json!({ "enabled": hub.control_on(), "off_reason": hub.off_reason() })).into_response()
}

// ---- what happened, and what a source gives -----------------------------------------------

#[derive(Deserialize, Default)]
struct ListQuery {
	limit: Option<usize>,
}

/// `GET /api/v1/admin/activity`: the activity log, newest first.
async fn activity_list(State(hub): State<Arc<Hub>>, Query(q): Query<ListQuery>) -> Response {
	let n = q.limit.unwrap_or(limit::ADMIN_LIST_MAX).min(limit::ADMIN_LIST_MAX);
	Json(json!({ "entries": hub.activity.newest(n) })).into_response()
}

/// `GET /api/v1/admin/events`: the kept events, newest first, each with how sending it went.
async fn events_list(State(hub): State<Arc<Hub>>, Query(q): Query<ListQuery>) -> Response {
	let n = q.limit.unwrap_or(limit::ADMIN_LIST_MAX).min(limit::ADMIN_LIST_MAX);
	let events: Vec<Value> = hub
		.events
		.newest(n)
		.into_iter()
		.map(|(e, sent)| {
			let mut v = serde_json::to_value(&e).unwrap_or(Value::Null);
			if let Some(o) = v.as_object_mut() {
				o.insert("sent".into(), serde_json::to_value(&sent).unwrap_or(Value::Null));
			}
			v
		})
		.collect();
	Json(json!({ "events": events })).into_response()
}

#[derive(Deserialize, Default)]
struct ValuesQuery {
	source: Option<String>,
}

/// `GET /api/v1/admin/values`: the sources the settings have and the computer's values now; with
/// `source`, what `elamond values <source>` prints of it.
async fn values(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Query(q): Query<ValuesQuery>) -> Response {
	let config = hub.config();
	let Some(id) = q.source else {
		return Json(json!({ "sources": sources_of(&config), "values": values_now(&hub, who.role) })).into_response();
	};
	let kind = if config.feeds.contains_key(&id) {
		config.feeds.get(&id).map(|f| f.kind.as_str()).unwrap_or("command")
	} else if config.sources.contains_key(&id) {
		"nut"
	} else if id == "displays" {
		"displays"
	} else {
		return error(ErrorCode::NotFound);
	};
	let asked = id.clone();
	let listed = tokio::time::timeout(
		Duration::from_secs(20),
		tokio::task::spawn_blocking(move || match config.feeds.get(&id) {
			Some(feed) => crate::feeds::cli::listing(&config, &id, feed),
			None if config.sources.contains_key(&id) => crate::nut::cli::values(&config, &id),
			None => Ok(crate::displays::Displays::quiet().listing(&config.displays)),
		}),
	)
	.await;
	let (lines, err) = match listed {
		Ok(Ok(Ok(text))) => (text.lines().map(str::to_string).collect::<Vec<_>>(), None),
		Ok(Ok(Err(e))) => (Vec::new(), Some(e)),
		_ => (Vec::new(), Some("it took too long to read".to_string())),
	};
	Json(json!({ "id": asked, "kind": kind, "lines": lines, "error": err })).into_response()
}

/// The sources of the settings, by id and kind.
pub(crate) fn sources_of(config: &settings::Config) -> Vec<Value> {
	let mut out: Vec<Value> = config.sources.keys().map(|id| json!({ "id": id, "kind": "nut" })).collect();
	out.extend(config.feeds.iter().map(|(id, f)| json!({ "id": id, "kind": f.kind.as_str() })));
	out.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
	out
}

/// Every value of the reading now that `role` sees, by address, as text.
pub(crate) fn values_now(hub: &Hub, role: Role) -> Vec<Value> {
	let frame = hub.frame.borrow().clone();
	frame
		.values
		.scalars
		.iter()
		.filter(|(a, _)| crate::values::address_role(a) <= role)
		.map(|(a, s)| json!({ "address": a, "now": crate::format::format(s.v.shown(), s.format, s.unit), "unit": s.unit, "format": s.format }))
		.collect()
}
