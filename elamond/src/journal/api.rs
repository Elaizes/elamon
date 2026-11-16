//! `GET /api/v1/log` (docs/protocol.md): older lines of a `log` in the client's `ui`, from a
//! `journalctl` run for them, looking back from where the client's lines begin.

use std::path::Path;
use std::sync::Arc;

use axum::extract::{Extension, Query, State};
use axum::response::Response;
use serde::Deserialize;
use serde_json::Value;

use super::{cant_start, parse, priority_of, spawn, why_failed, Filter, Key, Line, Splitter, FIELDS, INFO};
use crate::access::Caller;
use crate::consts::{limit, timeout};
use crate::control::{read_some, Group};
use crate::errors::{reason, ErrorCode};
use crate::server::{error, error_with, json_response, ClientApp, Hub};
use crate::ui::{Obj, Put};

#[derive(Deserialize, Default)]
pub struct LogQuery {
	widget: Option<String>,
	before: Option<String>,
	level: Option<String>,
	search: Option<String>,
}

/// Older lines of a `log` in the client's `ui`: the one it has now, for its role, profile and the
/// widget types it last said it draws.
pub async fn log(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Extension(app): Extension<ClientApp>, Query(q): Query<LogQuery>) -> Response {
	let Some(widget) = q.widget.filter(|w| !w.is_empty()) else { return error_with(ErrorCode::BadParam, "widget") };
	let before = match q.before.as_deref().map(str::parse::<u64>) {
		None => None,
		Some(Ok(b)) if b > 0 => Some(b),
		Some(_) => return error_with(ErrorCode::BadParam, "before"),
	};
	let level = match q.level.as_deref().map(priority_of) {
		None => None,
		Some(Some(p)) => Some(p),
		Some(None) => return error_with(ErrorCode::BadParam, "level"),
	};
	if q.search.as_ref().is_some_and(|s| s.chars().count() > limit::LOG_SEARCH_MAX) {
		return error_with(ErrorCode::BadParam, "search");
	}
	let key = hub.ui_key(&who, &app.kind, hub.draws_of(&who.id, &app.kind));
	let frame = hub.frame.borrow().clone();
	let ui = hub.ui(&key, &frame);
	let found = ui.dashboards.iter().flat_map(|d| d.all_widgets()).find(|w| w.id == widget && w.kind == "log");
	let Some((w, journal)) = found.and_then(|w| Some((w, Key::of(&w.def, &hub.config())?))) else { return error(ErrorCode::NoSuchWidget) };
	let own = w.def.level.as_deref().and_then(priority_of).unwrap_or(INFO);
	let filter = Filter::new(level.unwrap_or(own), q.search.as_deref());
	// A few at a time; the rest wait their turn.
	let Ok(_turn) = hub.journal.pages.acquire().await else { return error(ErrorCode::Failed) };
	match tokio::time::timeout(timeout::JOURNAL_PAGE, page(&hub.journal.program, &journal, before, &filter)).await {
		Ok(Ok((lines, before))) => json_response(serde_json::to_string(&Value::Object(answer(&widget, &lines, before))).unwrap_or_default()),
		Ok(Err(why)) => error_with(ErrorCode::Failed, why),
		Err(_) => error_with(ErrorCode::Failed, reason::TIMED_OUT),
	}
}

/// The answer for the log `widget`: `lines` oldest first, and where to go on from.
pub fn answer(widget: &str, lines: &[Line], before: Option<u64>) -> Obj {
	let mut o = Obj::new();
	o.put("widget", widget);
	o.put("lines", Value::Array(lines.iter().map(|l| Value::Object(l.json())).collect()));
	o.put("before", before.map_or(Value::Null, Value::from));
	o
}

/// journalctl's arguments for lines of `key`'s journal older than `before` (Unix µs), the newest
/// first, of a priority up to `priority`.
fn page_args(key: &Key, before: Option<u64>, priority: u8) -> Vec<String> {
	let mut args = key.args();
	args.extend(["-o", "json", FIELDS, "--all", "--no-pager", "-r", "-n", &limit::LOG_PAGE_SCAN.to_string(), "-p", &format!("0..{priority}")].map(String::from));
	if let Some(us) = before.map(|b| b - 1) {
		args.push(format!("--until=@{}.{:06}", us / 1_000_000, us % 1_000_000));
	}
	args
}

/// Lines of `key`'s journal before `before` (the newest without), back until `limit::LOG_PAGE` of
/// them `filter` lets through or `limit::LOG_PAGE_SCAN` have been looked at: those, oldest first,
/// and the `c` of the oldest looked at, none once the journal has no more.
pub async fn page(program: &Path, key: &Key, before: Option<u64>, filter: &Filter) -> Result<(Vec<Line>, Option<u64>), String> {
	let mut child = spawn(program, &page_args(key, before, filter.priority)).map_err(|e| cant_start(&e))?;
	// Killed as this ends, however it ends: enough lines, the timeout, the request gone.
	let _group = Group(child.id().map(|id| id as i32));
	let (mut out, mut err) = (child.stdout.take(), child.stderr.take());
	let (mut out_buf, mut err_buf) = (vec![0u8; 64 * 1024], [0u8; 4096]);
	let mut splitter = Splitter::default();
	let mut errors = Vec::new();
	let mut status = None;
	let (mut lines, mut looked, mut oldest) = (Vec::new(), 0, None);
	let full = |lines: &Vec<Line>| lines.len() >= limit::LOG_PAGE;
	while !full(&lines) && (status.is_none() || out.is_some() || err.is_some()) {
		tokio::select! {
			n = read_some(&mut out, &mut out_buf) => splitter.feed(&out_buf[..n], |l| {
				if full(&lines) {
					return;
				}
				looked += 1;
				let Some(line) = parse(l) else { return };
				oldest = Some(line.us);
				if filter.matches(&line) {
					lines.push(line);
				}
			}),
			n = read_some(&mut err, &mut err_buf) => {
				let room = limit::FEED_STDERR_MAX.saturating_sub(errors.len()).min(n);
				errors.extend_from_slice(&err_buf[..room]);
			}
			s = child.wait(), if status.is_none() => status = Some(s),
		}
	}
	let failed = status.as_ref().is_some_and(|s| !s.as_ref().is_ok_and(|s| s.success()));
	if !full(&lines) && failed {
		return Err(why_failed(&errors, status));
	}
	let more = full(&lines) || looked >= limit::LOG_PAGE_SCAN;
	lines.reverse();
	Ok((lines, oldest.filter(|_| more)))
}
