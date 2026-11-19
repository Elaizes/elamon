//! `POST /api/v1/admin/preview` (docs/protocol.md): what the settings would show with a draft — a
//! settings file's text, or the dashboard editor's changes — without writing it or running anything
//! of it. The `ui` is built for the asking client as the service builds any, with the dashboard
//! asked for among its tabs, and that dashboard's data from the reading there is now: a source or an
//! action only the draft has is never started, so its values are missing.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Extension, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use super::bad_settings;
use crate::access::Caller;
use crate::errors::ErrorCode;
use crate::server::{error, error_with, ClientApp, Hub};
use crate::settings::{self, Config};

#[derive(Deserialize)]
pub struct FileDraft {
	pub name: String,
	pub text: String,
}

#[derive(Deserialize)]
pub struct PreviewRequest {
	#[serde(default)]
	files: Vec<FileDraft>,
	#[serde(default)]
	editor: Option<super::editor::Draft>,
	#[serde(default)]
	dashboard: Option<String>,
	#[serde(default)]
	widgets: Option<String>,
}

/// The settings of the files with the drafts in place, as the service would read them.
fn settings_with(hub: &Hub, files: &[FileDraft], editor: Option<&super::editor::Draft>) -> Result<Config, Response> {
	let place = &hub.place;
	let mut sources = settings::read_files(&place.files()).map_err(|m| bad_settings(&m))?;
	for f in files {
		let Some(file) = super::files::named(place).into_iter().find(|n| n.name == f.name).or_else(|| {
			f.name
				.strip_prefix(&format!("{}/", crate::store::DROPIN_DIR))
				.filter(|n| settings::is_settings_file_name(n))
				.map(|n| super::files::Named { name: f.name.clone(), path: place.dropins().join(n), kind: "dropin" })
		}) else {
			return Err(error(ErrorCode::BadName));
		};
		let at = file.path.display().to_string();
		match sources.iter_mut().find(|s| s.name == at) {
			Some(s) => s.text = f.text.clone(),
			None => {
				sources = crate::commands::sources_with(place, Some(&file.path), Some(f.text.clone())).map_err(|m| bad_settings(&m))?;
			}
		}
	}
	if let Some(draft) = editor {
		sources = super::editor::sources_with_draft(hub, sources, draft).map_err(|m| bad_settings(&m))?;
	}
	settings::read_sources(&sources).map_err(|m| {
		let shown: Vec<settings::Mistake> = m
			.into_iter()
			.map(|mut x| {
				x.file = x.file.map(|f| super::files::shown_name(place, &f));
				x
			})
			.collect();
		bad_settings(&shown)
	})
}

pub async fn preview(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Extension(app): Extension<ClientApp>, Json(req): Json<PreviewRequest>) -> Response {
	let mut config = match settings_with(&hub, &req.files, req.editor.as_ref()) {
		Ok(c) => c,
		Err(r) => return r,
	};
	let draws = req.widgets.as_deref().map(crate::server::draws_from);
	// The client's profile, by the draft's settings, with the dashboard asked for among its tabs.
	let asked = who.profile.clone().unwrap_or_else(|| config.default_profile.clone());
	let profile = config.profile(&asked);
	if let Some(d) = &req.dashboard {
		if !config.dashboards.contains_key(d) {
			return error(ErrorCode::NoSuchDashboard);
		}
		if !profile.dashboards.contains(d) {
			let mut tabs = profile.dashboards.clone();
			tabs.push(d.clone());
			config.profiles.entry(profile.id.clone()).or_default().dashboards = Some(tabs);
		}
	}
	let frame = hub.frame.borrow().clone();
	let built = crate::ui::build(&crate::ui::Input {
		config: &config,
		values: &frame.values,
		caps: &frame.caps,
		service: hub.service_for(&frame, who.role),
		control: crate::ui::Control { enabled: hub.control_on(), off_reason: hub.off_reason() },
		role: who.role,
		profile: &profile.id,
		kind: &app.kind,
		draws: draws.as_ref(),
	});
	let Some(dashboard) = crate::server::dashboard_in(&built, req.dashboard.as_deref()) else {
		return error(ErrorCode::NoSuchDashboard);
	};
	let mut data = crate::server::data_of(&hub, &built, dashboard, &who, &frame, &HashMap::new(), true);
	data.remove("type");
	match serde_json::to_value(&data) {
		Ok(data) => Json(json!({ "ui": built.doc, "data": data })).into_response(),
		Err(e) => error_with(ErrorCode::Failed, e.to_string()),
	}
}
