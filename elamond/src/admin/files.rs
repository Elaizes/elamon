//! The settings files through the admin API (docs/protocol.md, "The settings files"): each file of
//! the set read, checked with a draft in its place as `elamond check FILE` does, and written only
//! when the settings with it are sound, in one step, with a backup of the file as it was. A file in
//! the Nix store, or one that leads out of the settings' folder, is only read; a write that finds
//! the file changed since its client read it writes nothing.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Extension, Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::{bad_settings, error_body, mistakes_json, record, writes_refused};
use crate::access::Caller;
use crate::consts::limit;
use crate::errors::ErrorCode;
use crate::server::{error, error_with, Hub};
use crate::settings::{self, Mistake, Source};
use crate::store::{self, Place};

/// Where the admin API keeps the files it replaced, in the settings' folder: never read as settings.
pub const BACKUPS_DIR: &str = "backups";

/// What a main file in the Nix store is said to be.
const FROM_NIX: &str = "set in Nix: change it in your NixOS or Home Manager configuration";

/// A file of the set, as the API names it.
#[derive(Clone, Debug, PartialEq)]
pub struct Named {
	/// `elamond.toml`, `elamond.d/mine.toml`.
	pub name: String,
	pub path: PathBuf,
	/// `main`, `own` or `dropin`.
	pub kind: &'static str,
}

/// The name of a settings file as the API gives it: within the settings' folder from there; the main
/// file `ELAMON_CONFIG` names, by its own name.
pub fn shown_name(place: &Place, file: &str) -> String {
	let path = Path::new(file);
	if let Ok(within) = path.strip_prefix(&place.dir) {
		return within.display().to_string();
	}
	path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| file.to_string())
}

/// The files of the set of `place`, in the order they are read (those not there left out).
pub fn named(place: &Place) -> Vec<Named> {
	let files = place.files();
	let main = place.main();
	files
		.into_iter()
		.filter(|p| fs::symlink_metadata(p).is_ok())
		.map(|p| {
			let kind = if p == main {
				"main"
			} else if p.parent() == Some(place.dropins().as_path()) {
				"dropin"
			} else {
				"own"
			};
			Named { name: shown_name(place, &p.display().to_string()), path: p, kind }
		})
		.collect()
}

/// The file the API's `name` is, of the set or a new drop-in: 400 `bad_name` for any other name.
fn resolve(place: &Place, name: &str) -> Result<Named, Response> {
	if let Some(n) = named(place).into_iter().find(|n| n.name == name) {
		return Ok(n);
	}
	if name == store::CONFIG_FILE && place.named.is_none() {
		return Ok(Named { name: name.into(), path: place.main(), kind: "main" });
	}
	match name.strip_prefix(&format!("{}/", store::DROPIN_DIR)) {
		Some(file) if settings::is_settings_file_name(file) => Ok(Named { name: name.into(), path: place.dropins().join(file), kind: "dropin" }),
		_ => Err(error(ErrorCode::BadName)),
	}
}

/// Why the file at `path` may not be written from here, if it may not: in the Nix store, a link,
/// out of the settings' folder, or one the service can't write.
pub fn read_only(place: &Place, path: &Path) -> Option<String> {
	if store::from_nix_store(path) {
		return Some(FROM_NIX.into());
	}
	if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
		return Some("a link to another file: change that file, or replace the link with a file".into());
	}
	let folder = path.parent().unwrap_or(Path::new("/"));
	let inside = [place.dir.clone(), place.dropins()].iter().any(|d| folder == d.as_path());
	if !inside {
		return Some(format!("out of the settings' folder ({})", store::shown(&place.dir)));
	}
	if let Ok(m) = fs::metadata(path)
		&& m.permissions().readonly()
	{
		return Some("the file may not be written (its permissions)".into());
	}
	None
}

/// 16 hex characters of the SHA-256 of `text`: what a write names as its `base`.
pub fn hash(text: &[u8]) -> String {
	store::sha256_hex(text)[..16].to_string()
}

/// A file's text as it is, if it is there; one too large, or not text, is an error.
fn text_of(path: &Path) -> io::Result<Option<String>> {
	match fs::metadata(path) {
		Ok(m) if m.len() as usize > limit::SETTINGS_BODY_MAX => Err(io::Error::other("larger than the admin API reads")),
		Ok(_) => fs::read_to_string(path).map(Some),
		Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
		Err(e) => Err(e),
	}
}

/// What `file` is to the person writing it: `phones` (pairing writes it too), `editor` (the
/// dashboard editor's), none.
fn note_of(file: &Named, editor_file: &str) -> Option<&'static str> {
	if file.kind != "dropin" {
		return None;
	}
	let base = file.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
	if base == crate::phones::FILE {
		Some("phones")
	} else if base == editor_file {
		Some("editor")
	} else {
		None
	}
}

/// The set's files for `admin/state`.
pub fn list(place: &Place, _editor_file: &str) -> Vec<Value> {
	named(place)
		.into_iter()
		.map(|f| {
			let meta = fs::metadata(&f.path).ok();
			let text = fs::read(&f.path).ok();
			let why = read_only(place, &f.path);
			json!({
				"name": f.name,
				"kind": f.kind,
				"writable": why.is_none(),
				"why": why,
				"hash": text.as_deref().map(hash),
				"size": meta.as_ref().map(|m| m.len()),
				"modified": meta.and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()),
			})
		})
		.collect()
}

// ---- backups ------------------------------------------------------------------------------

/// The folder of backups, and the start of the names of `name`'s.
fn backups_of(place: &Place, name: &str) -> (PathBuf, String) {
	(place.dir.join(BACKUPS_DIR), format!("{}.", name.replace('/', "_")))
}

/// `name`'s backups, newest first: id (Unix ms), Unix time, size.
pub fn backups(place: &Place, name: &str) -> Vec<(String, u64, u64)> {
	let (dir, prefix) = backups_of(place, name);
	let mut out: Vec<(String, u64, u64)> = fs::read_dir(&dir)
		.map(|d| {
			d.flatten()
				.filter_map(|e| {
					let file = e.file_name().to_string_lossy().into_owned();
					let id = file.strip_prefix(&prefix)?.to_string();
					let ms: u64 = id.parse().ok()?;
					Some((id, ms / 1000, e.metadata().map(|m| m.len()).unwrap_or(0)))
				})
				.collect()
		})
		.unwrap_or_default();
	out.sort_by(|a, b| b.0.cmp(&a.0));
	out
}

/// Keeps `text`, `name` as it was, among its backups, `keep` at most; its id, none when none are kept.
fn back_up(place: &Place, name: &str, text: &str, mode: u32, keep: u64) -> io::Result<Option<String>> {
	if keep == 0 {
		return Ok(None);
	}
	let (dir, prefix) = backups_of(place, name);
	store::private_dir(&dir)?;
	let mut ms = crate::activity::now_ms();
	// Two writes within a millisecond keep two backups.
	while dir.join(format!("{prefix}{ms}")).exists() {
		ms += 1;
	}
	let id = ms.to_string();
	store::replace_as(&dir.join(format!("{prefix}{id}")), text.as_bytes(), mode)?;
	for (old, _, _) in backups(place, name).into_iter().skip(keep as usize) {
		let _ = fs::remove_file(dir.join(format!("{prefix}{old}")));
	}
	Ok(Some(id))
}

// ---- reading and checking -----------------------------------------------------------------

#[derive(Deserialize)]
pub struct NameQuery {
	name: String,
	#[serde(default)]
	id: Option<String>,
}

/// `GET /api/v1/admin/file?name=`.
pub async fn read(State(hub): State<Arc<Hub>>, Query(q): Query<NameQuery>) -> Response {
	let file = match resolve(&hub.place, &q.name) {
		Ok(f) => f,
		Err(r) => return r,
	};
	let text = match text_of(&file.path) {
		Ok(Some(t)) => t,
		Ok(None) => return error(ErrorCode::NotFound),
		Err(e) => return error_with(ErrorCode::Storage, e.to_string()),
	};
	let why = read_only(&hub.place, &file.path);
	let config = hub.config();
	let backups: Vec<Value> = backups(&hub.place, &file.name).into_iter().map(|(id, at, size)| json!({ "id": id, "at": at, "size": size })).collect();
	Json(json!({
		"name": file.name,
		"kind": file.kind,
		"text": text,
		"hash": hash(text.as_bytes()),
		"writable": why.is_none(),
		"why": why,
		"note": note_of(&file, &config.web.editor_file),
		"backups": backups,
	}))
	.into_response()
}

/// `GET /api/v1/admin/file/backup?name=&id=`.
pub async fn backup(State(hub): State<Arc<Hub>>, Query(q): Query<NameQuery>) -> Response {
	let Some(id) = q.id.filter(|i| !i.is_empty() && i.bytes().all(|b| b.is_ascii_digit())) else { return error_with(ErrorCode::BadParam, "id") };
	let file = match resolve(&hub.place, &q.name) {
		Ok(f) => f,
		Err(r) => return r,
	};
	let (dir, prefix) = backups_of(&hub.place, &file.name);
	match text_of(&dir.join(format!("{prefix}{id}"))) {
		Ok(Some(text)) => Json(json!({ "name": file.name, "id": id, "at": id.parse::<u64>().unwrap_or(0) / 1000, "text": text })).into_response(),
		Ok(None) => error(ErrorCode::NotFound),
		Err(e) => error_with(ErrorCode::Storage, e.to_string()),
	}
}

/// The set of `place` as the service would read it with `change` made: a file's text in its place
/// (or a new drop-in), a file taken out, or both (a rename).
pub fn sources_with_change(place: &Place, remove: Option<&Path>, put: Option<(&Path, &str)>) -> Result<Vec<Source>, Vec<Mistake>> {
	let mut sources = match put {
		Some((path, text)) => crate::commands::sources_with(place, Some(path), Some(text.to_string()))?,
		None => settings::read_files(&place.files())?,
	};
	if let Some(gone) = remove {
		let gone = gone.display().to_string();
		sources.retain(|s| s.name != gone);
	}
	Ok(sources)
}

/// Mistakes with their files as the API names them.
fn shown_mistakes(place: &Place, m: Vec<Mistake>) -> Vec<Mistake> {
	m.into_iter()
		.map(|mut x| {
			x.file = x.file.map(|f| shown_name(place, &f));
			x
		})
		.collect()
}

#[derive(Deserialize)]
pub struct CheckRequest {
	name: String,
	text: String,
}

/// `POST /api/v1/admin/file/check`.
pub async fn check(State(hub): State<Arc<Hub>>, Json(req): Json<CheckRequest>) -> Response {
	let file = match resolve(&hub.place, &req.name) {
		Ok(f) => f,
		Err(r) => return r,
	};
	let place = hub.place.clone();
	let answer = tokio::task::spawn_blocking(move || {
		let before = text_of(&file.path).ok().flatten().unwrap_or_default();
		let diff = unified(&before, &req.text);
		let (sound, mistakes, warnings) = match sources_with_change(&place, None, Some((&file.path, &req.text))).and_then(|s| settings::read_sources(&s)) {
			Ok(c) => (true, Vec::new(), settings::warnings(&c)),
			Err(m) => (false, shown_mistakes(&place, m), Vec::new()),
		};
		json!({ "sound": sound, "mistakes": mistakes_json(&mistakes), "warnings": warnings, "diff": diff })
	})
	.await;
	match answer {
		Ok(v) => Json(v).into_response(),
		Err(e) => error_with(ErrorCode::Failed, e.to_string()),
	}
}

// ---- writing ------------------------------------------------------------------------------

/// What a write does: `put` a text in a file's place, or take a file out, or both.
pub struct Write<'a> {
	pub file: &'a Named,
	/// The hash of the file as its client read it; none for a new one.
	pub base: Option<&'a str>,
	pub put: Option<(&'a Path, &'a str)>,
	pub remove: Option<&'a Path>,
}

/// What a write did: the new text's hash and the backup's id.
pub struct Written {
	pub hash: Option<String>,
	pub backup: Option<String>,
}

/// Why a write wrote nothing.
pub enum Refused {
	ReadOnly(String),
	Changed(Option<String>),
	Mistakes(Vec<Mistake>),
	Io(String),
}

impl Refused {
	pub fn into_response(self) -> Response {
		match self {
			Refused::ReadOnly(why) => error_with(ErrorCode::ReadOnly, why),
			Refused::Changed(now) => {
				let mut extra = Map::new();
				extra.insert("hash".into(), json!(now));
				error_body(ErrorCode::ChangedOnDisk, extra)
			}
			Refused::Mistakes(m) => bad_settings(&m),
			Refused::Io(e) => error_with(ErrorCode::Storage, e),
		}
	}

	pub fn code(&self) -> &'static str {
		match self {
			Refused::ReadOnly(_) => ErrorCode::ReadOnly.as_str(),
			Refused::Changed(_) => ErrorCode::ChangedOnDisk.as_str(),
			Refused::Mistakes(_) => ErrorCode::BadSettings.as_str(),
			Refused::Io(_) => ErrorCode::Storage.as_str(),
		}
	}
}

/// Does `w` under the lock of writes the caller holds: the file as its client read it, the
/// settings sound with the change, a backup, then the change in one step.
pub fn apply(place: &Place, w: &Write, keep: u64) -> Result<Written, Refused> {
	for p in w.put.map(|(p, _)| p).into_iter().chain(w.remove) {
		if let Some(why) = read_only(place, p) {
			// A new drop-in isn't there to be read-only; its folder is the settings' own.
			if fs::symlink_metadata(p).is_ok() || !why.starts_with("a link") {
				return Err(Refused::ReadOnly(why));
			}
		}
	}
	let now = text_of(&w.file.path).map_err(|e| Refused::Io(e.to_string()))?;
	let now_hash = now.as_deref().map(|t| hash(t.as_bytes()));
	if now_hash.as_deref() != w.base {
		return Err(Refused::Changed(now_hash));
	}
	// A rename's new name mustn't be a file there already.
	if let Some((to, _)) = w.put
		&& to != w.file.path
		&& fs::symlink_metadata(to).is_ok()
	{
		return Err(Refused::Changed(fs::read(to).ok().map(|t| hash(&t))));
	}
	let sources = sources_with_change(place, w.remove, w.put).map_err(|m| Refused::Mistakes(shown_mistakes(place, m)))?;
	settings::read_sources(&sources).map_err(|m| Refused::Mistakes(shown_mistakes(place, m)))?;
	let mode = store::mode_of(&w.file.path, 0o600);
	let backup = match &now {
		Some(t) => back_up(place, &w.file.name, t, mode, keep).map_err(|e| Refused::Io(format!("can't keep a backup: {e}")))?,
		None => None,
	};
	let mut hash_now = None;
	if let Some((to, text)) = w.put {
		if let Some(dir) = to.parent() {
			fs::create_dir_all(dir).map_err(|e| Refused::Io(e.to_string()))?;
		}
		store::replace_as(to, text.as_bytes(), mode).map_err(|e| Refused::Io(e.to_string()))?;
		hash_now = Some(hash(text.as_bytes()));
	}
	if let Some(gone) = w.remove
		&& w.put.is_none_or(|(to, _)| to != gone)
	{
		fs::remove_file(gone).map_err(|e| Refused::Io(e.to_string()))?;
	}
	Ok(Written { hash: hash_now, backup })
}

/// Runs a write for `who` with the lock of writes held, takes the settings up at once, says it in
/// the journal and the activity log, and answers as `file/save` does.
pub async fn write(hub: &Arc<Hub>, who: &Caller, what: String, file: Named, base: Option<String>, put: Option<(PathBuf, String)>, remove: Option<PathBuf>) -> Response {
	if let Some(r) = writes_refused(hub) {
		record(hub, who, what, ErrorCode::WritesOff.as_str(), None);
		return r;
	}
	let _held = hub.writes.lock().await;
	let (place, keep) = (hub.place.clone(), hub.config().web.backups);
	let file2 = file.clone();
	let done = tokio::task::spawn_blocking(move || {
		let w = Write { file: &file2, base: base.as_deref(), put: put.as_ref().map(|(p, t)| (p.as_path(), t.as_str())), remove: remove.as_deref() };
		apply(&place, &w, keep)
	})
	.await
	.unwrap_or_else(|e| Err(Refused::Io(e.to_string())));
	match done {
		Ok(written) => {
			let generation = hub.reload_now();
			eprintln!("settings: \"{}\" ({}) wrote {} from the admin API; taken up", who.name, who.id, file.name);
			record(hub, who, what, "ok", written.backup.as_ref().map(|b| format!("backup {b}")));
			Json(json!({ "ok": true, "hash": written.hash, "backup": written.backup, "generation": generation })).into_response()
		}
		Err(r) => {
			record(hub, who, what, r.code(), None);
			r.into_response()
		}
	}
}

#[derive(Deserialize)]
pub struct SaveRequest {
	name: String,
	text: String,
	base: Option<String>,
}

/// `POST /api/v1/admin/file/save`.
pub async fn save(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Json(req): Json<SaveRequest>) -> Response {
	let file = match resolve(&hub.place, &req.name) {
		Ok(f) => f,
		Err(r) => return r,
	};
	let what = format!("settings: saved {} ({} lines)", file.name, req.text.lines().count());
	let path = file.path.clone();
	write(&hub, &who, what, file, req.base, Some((path, req.text)), None).await
}

#[derive(Deserialize)]
pub struct RestoreRequest {
	name: String,
	backup: String,
	base: Option<String>,
}

/// `POST /api/v1/admin/file/restore`: a backup written back, as a save of its text.
pub async fn restore(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Json(req): Json<RestoreRequest>) -> Response {
	let file = match resolve(&hub.place, &req.name) {
		Ok(f) => f,
		Err(r) => return r,
	};
	if req.backup.is_empty() || !req.backup.bytes().all(|b| b.is_ascii_digit()) {
		return error_with(ErrorCode::BadParam, "backup");
	}
	let (dir, prefix) = backups_of(&hub.place, &file.name);
	let text = match text_of(&dir.join(format!("{prefix}{}", req.backup))) {
		Ok(Some(t)) => t,
		Ok(None) => return error(ErrorCode::NotFound),
		Err(e) => return error_with(ErrorCode::Storage, e.to_string()),
	};
	let what = format!("settings: restored {} from backup {}", file.name, req.backup);
	let path = file.path.clone();
	write(&hub, &who, what, file, req.base, Some((path, text)), None).await
}

#[derive(Deserialize)]
pub struct RenameRequest {
	name: String,
	to: String,
	base: Option<String>,
}

/// `POST /api/v1/admin/file/rename`: drop-ins only.
pub async fn rename(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Json(req): Json<RenameRequest>) -> Response {
	let (file, to) = match (resolve(&hub.place, &req.name), resolve(&hub.place, &req.to)) {
		(Ok(f), Ok(t)) if f.kind == "dropin" && t.kind == "dropin" && f.path != t.path => (f, t),
		(Err(r), _) | (_, Err(r)) => return r,
		_ => return error(ErrorCode::BadName),
	};
	let text = match text_of(&file.path) {
		Ok(Some(t)) => t,
		Ok(None) => return error(ErrorCode::NotFound),
		Err(e) => return error_with(ErrorCode::Storage, e.to_string()),
	};
	let what = format!("settings: renamed {} to {}", file.name, to.name);
	let from = file.path.clone();
	write(&hub, &who, what, file, req.base, Some((to.path, text)), Some(from)).await
}

#[derive(Deserialize)]
pub struct DeleteRequest {
	name: String,
	base: Option<String>,
}

/// `POST /api/v1/admin/file/delete`: drop-ins only.
pub async fn delete(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Json(req): Json<DeleteRequest>) -> Response {
	let file = match resolve(&hub.place, &req.name) {
		Ok(f) if f.kind == "dropin" => f,
		Ok(_) => return error(ErrorCode::BadName),
		Err(r) => return r,
	};
	let what = format!("settings: deleted {}", file.name);
	let gone = file.path.clone();
	write(&hub, &who, what, file, req.base, None, Some(gone)).await
}

// ---- a difference, line by line ------------------------------------------------------------

/// Lines of context around a change.
const CONTEXT: usize = 3;
/// Past this many lines changed between the same start and end, they are shown taken out and put
/// back whole rather than matched up.
const MATCH_MAX: usize = 4000;

/// The lines of a unified diff of `a` and `b`, hunks with their `@@` headers.
pub fn unified(a: &str, b: &str) -> Vec<String> {
	let (a, b): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
	let ops = line_ops(&a, &b);
	// Each op: (' ' | '-' | '+', index in a, index in b).
	let mut out = Vec::new();
	let changed: Vec<usize> = ops.iter().enumerate().filter(|(_, o)| o.0 != ' ').map(|(i, _)| i).collect();
	let mut i = 0;
	while i < changed.len() {
		let start = changed[i].saturating_sub(CONTEXT);
		let mut end = changed[i];
		while i + 1 < changed.len() && changed[i + 1] <= end + 2 * CONTEXT + 1 {
			i += 1;
			end = changed[i];
		}
		let end = (end + CONTEXT + 1).min(ops.len());
		let hunk = &ops[start..end];
		let a_start = hunk.iter().find(|o| o.0 != '+').map_or(hunk[0].1, |o| o.1);
		let b_start = hunk.iter().find(|o| o.0 != '-').map_or(hunk[0].2, |o| o.2);
		let a_len = hunk.iter().filter(|o| o.0 != '+').count();
		let b_len = hunk.iter().filter(|o| o.0 != '-').count();
		out.push(format!("@@ -{},{a_len} +{},{b_len} @@", a_start + usize::from(a_len > 0), b_start + usize::from(b_len > 0)));
		for (op, ai, bi) in hunk {
			let line = if *op == '+' { b[*bi] } else { a[*ai] };
			out.push(format!("{op}{line}"));
		}
		i += 1;
	}
	out
}

/// The lines of `a` and `b` matched up: common ones kept, the rest taken out or put in.
fn line_ops(a: &[&str], b: &[&str]) -> Vec<(char, usize, usize)> {
	let pre = a.iter().zip(b).take_while(|(x, y)| x == y).count();
	let suf = a[pre..].iter().rev().zip(b[pre..].iter().rev()).take_while(|(x, y)| x == y).count();
	let (am, bm) = (&a[pre..a.len() - suf], &b[pre..b.len() - suf]);
	let mut ops: Vec<(char, usize, usize)> = (0..pre).map(|i| (' ', i, i)).collect();
	if am.len() * bm.len() <= MATCH_MAX * MATCH_MAX / 4 {
		// The longest common lines between, by the table of their lengths.
		let (n, m) = (am.len(), bm.len());
		let mut t = vec![0u32; (n + 1) * (m + 1)];
		for i in (0..n).rev() {
			for j in (0..m).rev() {
				t[i * (m + 1) + j] = if am[i] == bm[j] { t[(i + 1) * (m + 1) + j + 1] + 1 } else { t[(i + 1) * (m + 1) + j].max(t[i * (m + 1) + j + 1]) };
			}
		}
		let (mut i, mut j) = (0, 0);
		while i < n || j < m {
			if i < n && j < m && am[i] == bm[j] {
				ops.push((' ', pre + i, pre + j));
				i += 1;
				j += 1;
			} else if i < n && (j == m || t[(i + 1) * (m + 1) + j] >= t[i * (m + 1) + j + 1]) {
				ops.push(('-', pre + i, pre + j));
				i += 1;
			} else {
				ops.push(('+', pre + i, pre + j));
				j += 1;
			}
		}
	} else {
		ops.extend((0..am.len()).map(|i| ('-', pre + i, pre)));
		ops.extend((0..bm.len()).map(|j| ('+', pre + am.len(), pre + j)));
	}
	ops.extend((0..suf).map(|k| (' ', a.len() - suf + k, b.len() - suf + k)));
	ops
}

#[cfg(test)]
mod tests {
	use super::*;

	fn scratch() -> Place {
		let dir = std::env::temp_dir().join(format!("elamond-files-{}", data_encoding::HEXLOWER.encode(&store::random_bytes::<6>())));
		fs::create_dir_all(dir.join(store::DROPIN_DIR)).unwrap();
		Place { dir, named: None }
	}

	#[test]
	fn a_difference_line_by_line() {
		assert_eq!(unified("a\nb\nc\n", "a\nb\nc\n"), Vec::<String>::new());
		assert_eq!(unified("a\nb\nc\n", "a\nx\nc\n"), ["@@ -1,3 +1,3 @@", " a", "-b", "+x", " c"]);
		assert_eq!(unified("", "a\n"), ["@@ -0,0 +1,1 @@", "+a"]);
		let long_a: String = (0..20).map(|i| format!("{i}\n")).collect();
		let long_b = long_a.replace("3\n", "three\n").replace("17\n", "seventeen\n");
		let d = unified(&long_a, &long_b);
		assert_eq!(d.iter().filter(|l| l.starts_with("@@")).count(), 2, "{d:?}");
		assert!(d.contains(&"-3".to_string()) && d.contains(&"+seventeen".to_string()));
	}

	#[test]
	fn names_the_api_takes() {
		let place = scratch();
		fs::write(place.main(), "schema = 3\n").unwrap();
		fs::write(place.dropins().join("mine.toml"), "").unwrap();
		assert_eq!(resolve(&place, "elamond.toml").unwrap().kind, "main");
		assert_eq!(resolve(&place, "elamond.d/mine.toml").unwrap().kind, "dropin");
		assert_eq!(resolve(&place, "elamond.d/new-one.toml").unwrap().path, place.dropins().join("new-one.toml"));
		for bad in ["../x.toml", "elamond.d/../x.toml", "elamond.d/.hidden.toml", "elamond.d/a/b.toml", "elamond.d/x.txt", "/etc/passwd", "elamond.d/", "x.toml", "elamond.d/..toml"] {
			assert!(resolve(&place, bad).is_err(), "{bad}");
		}
		fs::remove_dir_all(&place.dir).unwrap();
	}

	#[test]
	fn a_write_checks_keeps_a_backup_and_never_writes_what_is_broken() {
		let place = scratch();
		fs::write(place.main(), "schema = 3\n").unwrap();
		let mine = Named { name: "elamond.d/mine.toml".into(), path: place.dropins().join("mine.toml"), kind: "dropin" };
		let good = "[dashboards.mine]\ntitle = \"Mine\"\nwidgets = [{ type = \"text\", text = \"hi\" }]\n";
		// New: no base.
		let w = Write { file: &mine, base: None, put: Some((&mine.path, good)), remove: None };
		let done = apply(&place, &w, 2).unwrap_or_else(|_| panic!("refused"));
		assert_eq!(done.hash.as_deref(), Some(hash(good.as_bytes()).as_str()));
		assert_eq!(done.backup, None);
		assert_eq!(fs::read_to_string(&mine.path).unwrap(), good);
		// The wrong base: the file changed since.
		let w = Write { file: &mine, base: Some("0000000000000000"), put: Some((&mine.path, good)), remove: None };
		assert!(matches!(apply(&place, &w, 2), Err(Refused::Changed(Some(_)))));
		// Broken: nothing written.
		let h = hash(good.as_bytes());
		let broken = "[dashboards.mine]\ntitle = 3\n";
		let w = Write { file: &mine, base: Some(&h), put: Some((&mine.path, broken)), remove: None };
		match apply(&place, &w, 2) {
			Err(Refused::Mistakes(m)) => assert_eq!(m[0].file.as_deref(), Some("elamond.d/mine.toml"), "{m:?}"),
			_ => panic!("written"),
		}
		assert_eq!(fs::read_to_string(&mine.path).unwrap(), good);
		// A sound change keeps the old one as a backup, two at most.
		for n in 0..3 {
			let before = fs::read_to_string(&mine.path).unwrap();
			let text = good.replace("hi", &format!("hi {n}"));
			let base = hash(before.as_bytes());
			let w = Write { file: &mine, base: Some(&base), put: Some((&mine.path, &text)), remove: None };
			assert!(apply(&place, &w, 2).unwrap_or_else(|_| panic!("refused")).backup.is_some());
		}
		let kept = backups(&place, "elamond.d/mine.toml");
		assert_eq!(kept.len(), 2);
		// Backups aren't read as settings.
		assert!(place.files().iter().all(|f| !f.starts_with(place.dir.join(BACKUPS_DIR))));
		// Taking it out: sound, so done.
		let now = hash(fs::read(&mine.path).unwrap().as_slice());
		let w = Write { file: &mine, base: Some(&now), put: None, remove: Some(&mine.path) };
		assert!(apply(&place, &w, 2).is_ok());
		assert!(!mine.path.exists());
		fs::remove_dir_all(&place.dir).unwrap();
	}

	#[test]
	fn a_file_from_nix_or_a_link_out_is_only_read() {
		let place = scratch();
		let elsewhere = place.dir.with_extension("elsewhere.toml");
		fs::write(&elsewhere, "schema = 3\n").unwrap();
		std::os::unix::fs::symlink(&elsewhere, place.main()).unwrap();
		let why = read_only(&place, &place.main()).unwrap();
		assert!(why.starts_with("a link"), "{why}");
		let main = Named { name: "elamond.toml".into(), path: place.main(), kind: "main" };
		let base = hash(b"schema = 3\n");
		let w = Write { file: &main, base: Some(&base), put: Some((&main.path, "schema = 3\nport = 2\n")), remove: None };
		assert!(matches!(apply(&place, &w, 2), Err(Refused::ReadOnly(_))));
		assert_eq!(fs::read_to_string(&elsewhere).unwrap(), "schema = 3\n");
		fs::remove_dir_all(&place.dir).unwrap();
		fs::remove_file(&elsewhere).unwrap();
	}
}
