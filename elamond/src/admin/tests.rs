//! The page, a browser's session and the admin API, as the router answers them.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::consts::protocol;
use crate::server::tests::{call, call_as, hub, hub_with, remove_test_dir};
use crate::server::{router, Hub};
use crate::store;

const ORIGIN: &str = "https://192.0.2.7:47823";
const HOST: &str = "192.0.2.7:47823";

/// A request as a browser's page makes it: its cookie (if any), `Host`, and `Origin` (if given).
fn browser(method: &str, path: &str, cookie: Option<&str>, origin: Option<&str>, body: &str) -> Request<Body> {
	let mut b = Request::builder()
		.method(Method::from_bytes(method.as_bytes()).unwrap())
		.uri(path)
		.header(header::HOST, HOST)
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::USER_AGENT, "Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0")
		.header(protocol::CLIENT_HEADER, "web/0.11.0")
		.extension(ConnectInfo(SocketAddr::from(([192, 0, 2, 30], 50000))));
	if let Some(c) = cookie {
		b = b.header(header::COOKIE, format!("{}={c}", protocol::COOKIE));
	}
	if let Some(o) = origin {
		b = b.header(header::ORIGIN, o);
	}
	b.body(Body::from(body.to_string())).unwrap()
}

async fn send(hub: &Arc<Hub>, req: Request<Body>) -> (StatusCode, HeaderMap, Value) {
	let resp = router(hub.clone()).oneshot(req).await.unwrap();
	let (status, headers) = (resp.status(), resp.headers().clone());
	let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
	(status, headers, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// The token a `Set-Cookie` holds, and its attributes.
fn cookie_of(headers: &HeaderMap) -> Option<(String, String)> {
	let v = headers.get(header::SET_COOKIE)?.to_str().ok()?.to_string();
	let (pair, attrs) = v.split_once(';')?;
	let (name, token) = pair.split_once('=')?;
	(name == protocol::COOKIE).then(|| (token.to_string(), attrs.trim().to_string()))
}

/// A browser paired from the page with a code made as `elamond pair --role <role>` does.
async fn pair_browser(hub: &Arc<Hub>, role: crate::access::Role) -> String {
	let p = store::new_pairing(&hub.dir, role, None, false).unwrap();
	let (status, headers, body) = send(hub, browser("POST", "/api/v1/pair", None, Some(ORIGIN), &format!(r#"{{"code":"{}","device":""}}"#, p.code))).await;
	assert_eq!(status, StatusCode::OK, "{body}");
	assert!(body.get("token").is_none(), "a browser's token is never in the body: {body}");
	let (token, attrs) = cookie_of(&headers).expect("a cookie");
	assert_eq!(attrs, "Max-Age=2592000; Path=/; Secure; HttpOnly; SameSite=Strict");
	token
}

#[tokio::test]
async fn the_page_is_served_with_its_headers() {
	let (hub, _tx) = hub(true);
	let (status, headers, _) = send(&hub, browser("GET", "/", None, None, "")).await;
	assert_eq!(status, StatusCode::OK);
	assert!(headers[header::CONTENT_TYPE].to_str().unwrap().starts_with("text/html"));
	assert!(headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap().contains("script-src 'self'"));
	assert!(headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap().contains("frame-ancestors 'none'"));
	assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
	assert_eq!(headers[header::REFERRER_POLICY], "no-referrer");
	let etag = headers[header::ETAG].to_str().unwrap().to_string();
	// Again with its tag: not modified.
	let mut again = browser("GET", "/", None, None, "");
	again.headers_mut().insert(header::IF_NONE_MATCH, etag.parse().unwrap());
	assert_eq!(send(&hub, again).await.0, StatusCode::NOT_MODIFIED);
	let (status, headers, _) = send(&hub, browser("GET", "/web/js/main.js", None, None, "")).await;
	assert_eq!(status, StatusCode::OK);
	assert!(headers[header::CONTENT_TYPE].to_str().unwrap().starts_with("text/javascript"));
	// Its tests and anything else aren't served; the API's answers aren't kept.
	for path in ["/web/test/mock.mjs", "/web/jsconfig.json", "/web/../Cargo.toml", "/nothing"] {
		assert_eq!(send(&hub, browser("GET", path, None, None, "")).await.0, StatusCode::NOT_FOUND, "{path}");
	}
	let (_, headers, _) = send(&hub, browser("GET", "/api/v1/hello", None, None, "")).await;
	assert_eq!(headers[header::CACHE_CONTROL], "no-store");
	remove_test_dir(&hub.dir);
}

#[tokio::test]
async fn without_the_page_there_is_none() {
	let (hub, _tx) = hub_with(true, |h| {
		let mut c = (*h.config()).clone();
		c.web.page = false;
		h.set_config(c);
	});
	assert_eq!(send(&hub, browser("GET", "/", None, None, "")).await.0, StatusCode::NOT_FOUND);
	assert_eq!(send(&hub, browser("GET", "/web/js/main.js", None, None, "")).await.0, StatusCode::NOT_FOUND);
	remove_test_dir(&hub.dir);
}

#[tokio::test]
async fn a_browser_pairs_with_a_cookie_its_page_never_sees() {
	let (hub, _tx) = hub(true);
	hub.set_fingerprint("ab".repeat(32).as_str());
	let (_, _, hello) = send(&hub, browser("GET", "/api/v1/hello", None, None, "")).await;
	assert_eq!(hello["fingerprint"], json!("ab".repeat(32)));
	// From another page: refused, and the code isn't spent.
	let p = store::new_pairing(&hub.dir, crate::access::Role::Admin, None, false).unwrap();
	let body = format!(r#"{{"code":"{}","device":""}}"#, p.code);
	let (status, _, v) = send(&hub, browser("POST", "/api/v1/pair", None, Some("https://evil.example"), &body)).await;
	assert_eq!((status, v["error"].as_str()), (StatusCode::FORBIDDEN, Some("bad_origin")));
	let (status, _, v) = send(&hub, browser("POST", "/api/v1/pair", None, None, &body)).await;
	assert_eq!((status, v["error"].as_str()), (StatusCode::FORBIDDEN, Some("bad_origin")));
	store::clear_pairing(&hub.dir);
	let token = pair_browser(&hub, crate::access::Role::Admin).await;
	let list = store::load_clients_in(&hub.dir).unwrap();
	let me = list.iter().find(|c| c.web).expect("the browser, paired as web");
	assert_eq!(me.name, "Firefox on Linux");
	// The cookie lets it in, kind web, and comes again with `ui`.
	let (status, headers, ui) = send(&hub, browser("GET", "/api/v1/ui", Some(&token), None, "")).await;
	assert_eq!(status, StatusCode::OK, "{ui}");
	assert_eq!(cookie_of(&headers).map(|c| c.0), Some(token.clone()));
	// The token as a phone's Bearer: no. A phone's token as a cookie: no, and cleared.
	assert_eq!(call_as(&hub, "GET", "/api/v1/ui", &token, "").await.0, StatusCode::UNAUTHORIZED);
	let (status, headers, _) = send(&hub, browser("GET", "/api/v1/ui", Some("t-admin"), None, "")).await;
	assert_eq!(status, StatusCode::UNAUTHORIZED);
	assert!(cookie_of(&headers).is_some_and(|(t, a)| t.is_empty() && a.starts_with("Max-Age=0")));
	// A change by the cookie only from the page itself.
	let (status, _, v) = send(&hub, browser("POST", "/api/v1/actions/nothing", Some(&token), None, "{}")).await;
	assert_eq!((status, v["error"].as_str()), (StatusCode::FORBIDDEN, Some("bad_origin")));
	let mut cross = browser("POST", "/api/v1/actions/nothing", Some(&token), None, "{}");
	cross.headers_mut().insert("sec-fetch-site", "cross-site".parse().unwrap());
	assert_eq!(send(&hub, cross).await.0, StatusCode::FORBIDDEN);
	let (status, _, _) = send(&hub, browser("POST", "/api/v1/actions/nothing", Some(&token), Some(ORIGIN), "{}")).await;
	assert_eq!(status, StatusCode::NOT_FOUND);
	// No key of its own for a page: its token would be in a body.
	let (status, _, v) = send(&hub, browser("POST", "/api/v1/rekey", Some(&token), Some(ORIGIN), r#"{"device":"x"}"#)).await;
	assert_eq!((status, v["error"].as_str()), (StatusCode::FORBIDDEN, Some("rekey_refused")));
	// Forget this browser: unpaired, cookie cleared.
	let (status, headers, _) = send(&hub, browser("POST", "/api/v1/unpair", Some(&token), Some(ORIGIN), "")).await;
	assert_eq!(status, StatusCode::OK);
	assert!(cookie_of(&headers).is_some_and(|(t, _)| t.is_empty()));
	assert_eq!(send(&hub, browser("GET", "/api/v1/ui", Some(&token), None, "")).await.0, StatusCode::UNAUTHORIZED);
	remove_test_dir(&hub.dir);
}

#[tokio::test]
async fn the_admin_api_is_the_owner_s() {
	let (hub, _tx) = hub(true);
	for phone in ["glance", "monitor", "operate", "admin", "old"] {
		let (status, v) = call(&hub, "GET", "/api/v1/admin/state", phone, "").await;
		assert_eq!((status, v["error"].as_str()), (StatusCode::FORBIDDEN, Some("not_allowed")), "{phone}");
	}
	assert!(hub.activity.newest(100).iter().any(|e| e.what.starts_with("admin: GET /api/v1/admin/state") && e.result == "not_allowed"));
	let (status, state) = call(&hub, "GET", "/api/v1/admin/state", "owner", "").await;
	assert_eq!(status, StatusCode::OK, "{state}");
	assert_eq!(state["you"]["role"], "owner");
	assert!(state["files"].as_array().unwrap().iter().any(|f| f["name"] == "elamond.toml"));
	// features: admin and editor for the owner only; the `ui` of an owner's browser is the contract's.
	let owner_web = router(hub.clone()).oneshot(crate::server::tests::request_from("GET", "/api/v1/ui", "t-owner", "web/0.11.0", "")).await.unwrap();
	let owner_web: Value = serde_json::from_slice(&axum::body::to_bytes(owner_web.into_body(), usize::MAX).await.unwrap()).unwrap();
	crate::fixtures::check_text("0.11/ui-owner.json", &(serde_json::to_string_pretty(&owner_web).unwrap() + "\n"));
	let (_, ui) = call(&hub, "GET", "/api/v1/ui", "owner", "").await;
	let features: Vec<&str> = ui["service"]["features"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
	assert!(features.contains(&"admin") && features.contains(&"editor"), "{features:?}");
	let (_, ui) = call(&hub, "GET", "/api/v1/ui", "admin", "").await;
	assert!(!ui["service"]["features"].as_array().unwrap().iter().any(|f| f == "admin" || f == "editor"));
	// The shapes of the contract: every field the fixtures name is there.
	same_shape("admin-state", &state);
	let (_, clients) = call(&hub, "GET", "/api/v1/admin/clients", "owner", "").await;
	same_shape("admin-clients", &clients);
	let (_, settings) = call(&hub, "GET", "/api/v1/admin/settings", "owner", "").await;
	same_shape("admin-settings", &settings);
	remove_test_dir(&hub.dir);
	// Off: for every role.
	let (hub, _tx) = hub_with(true, |h| {
		let mut c = (*h.config()).clone();
		c.web.admin = false;
		h.set_config(c);
	});
	let (status, v) = call(&hub, "GET", "/api/v1/admin/state", "owner", "").await;
	assert_eq!((status, v["error"].as_str()), (StatusCode::FORBIDDEN, Some("admin_off")));
	remove_test_dir(&hub.dir);
}

/// Every field of the fixture `0.11/<name>.json` is in `real`, at every depth (a list's first item
/// for all of them): the service answers in the shape the contract says.
fn same_shape(name: &str, real: &Value) {
	let fixture: Value = serde_json::from_str(&std::fs::read_to_string(format!("{}/fixtures/0.11/{name}.json", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap();
	fn walk(f: &Value, r: &Value, path: &str, name: &str) {
		match (f, r) {
			(Value::Object(fo), Value::Object(ro)) => {
				for (k, fv) in fo {
					// Fields of tables keyed by data (settings, whence) are the data's own.
					if ["settings", "whence"].contains(&path) {
						return;
					}
					let rv = ro.get(k).unwrap_or_else(|| panic!("{name}: {path}.{k} is missing from {r}"));
					walk(fv, rv, format!("{path}.{k}").trim_start_matches('.'), name);
				}
			}
			(Value::Array(fa), Value::Array(ra)) => {
				if let (Some(f0), Some(r0)) = (fa.first(), ra.first()) {
					walk(f0, r0, path, name);
				}
			}
			_ => {}
		}
	}
	walk(&fixture, real, "", name);
}

#[tokio::test]
async fn clients_roles_and_control_from_the_page() {
	let (hub, _tx) = hub(true);
	// An owner gives roles up to its own; the change is in the activity log.
	let (status, v) = call(&hub, "POST", "/api/v1/admin/clients/id-glance/role", "owner", r#"{"role":"owner"}"#).await;
	assert_eq!(status, StatusCode::OK, "{v}");
	assert_eq!(v["changed"], json!(["id-glance"]));
	let list = store::load_clients_in(&hub.dir).unwrap();
	assert_eq!(list.iter().find(|c| c.id == "id-glance").unwrap().role, crate::access::Role::Owner);
	assert!(hub.activity.newest(10).iter().any(|e| e.what == "role of id-glance: owner" && e.result == "ok"));
	let (status, _) = call(&hub, "POST", "/api/v1/admin/clients/id-nobody/role", "owner", r#"{"role":"admin"}"#).await;
	assert_eq!(status, StatusCode::NOT_FOUND);
	let (status, _) = call(&hub, "POST", "/api/v1/admin/clients/id-admin/profile", "owner", r#"{"profile":"nothing"}"#).await;
	assert_eq!(status, StatusCode::BAD_REQUEST);
	let (status, _) = call(&hub, "POST", "/api/v1/admin/clients/id-admin/profile", "owner", r#"{"profile":"owner"}"#).await;
	assert_eq!(status, StatusCode::OK);
	// With `[web] role = "admin"`, an admin is in, but may give no more than admin.
	let (hub2, _tx2) = hub_with(true, |h| {
		let mut c = (*h.config()).clone();
		c.web.role = crate::access::Role::Admin;
		h.set_config(c);
	});
	let (status, v) = call(&hub2, "POST", "/api/v1/admin/clients/id-glance/role", "admin", r#"{"role":"owner"}"#).await;
	assert_eq!((status, v["error"].as_str()), (StatusCode::FORBIDDEN, Some("not_allowed")));
	let (status, v) = call(&hub2, "POST", "/api/v1/admin/pairing", "admin", r#"{"role":"owner"}"#).await;
	assert_eq!((status, v["error"].as_str()), (StatusCode::FORBIDDEN, Some("not_allowed")));
	remove_test_dir(&hub2.dir);
	// Pairing from the page: a code with its QR codes, answered by the page.
	hub.set_endpoints(vec!["192.0.2.7".into(), "desk.example.net".into()]);
	let (status, p) = call(&hub, "POST", "/api/v1/admin/pairing", "owner", r#"{"role":"monitor"}"#).await;
	assert_eq!(status, StatusCode::OK, "{p}");
	assert_eq!(p["from_page"], true);
	assert_eq!(p["role"], "monitor");
	same_shape_of_pending(&p);
	// Control off from the page; on only at the computer; then nothing changes clients or settings.
	let (status, v) = call(&hub, "POST", "/api/v1/admin/control", "owner", r#"{"on":true}"#).await;
	assert_eq!((status, v["error"].as_str()), (StatusCode::FORBIDDEN, Some("at_computer")));
	let (status, v) = call(&hub, "POST", "/api/v1/admin/control", "owner", r#"{"on":false}"#).await;
	assert_eq!((status, v["off_reason"].as_str()), (StatusCode::OK, Some("held_off")));
	assert!(store::control_held_off(&hub.dir));
	let (status, v) = call(&hub, "POST", "/api/v1/admin/clients/id-monitor/role", "owner", r#"{"role":"admin"}"#).await;
	assert_eq!((status, v["error"].as_str()), (StatusCode::FORBIDDEN, Some("writes_off")));
	let (status, v) = call(&hub, "POST", "/api/v1/admin/file/save", "owner", r#"{"name":"elamond.d/x.toml","text":"","base":null}"#).await;
	assert_eq!((status, v["error"].as_str()), (StatusCode::FORBIDDEN, Some("writes_off")));
	// Revoking a client.
	store::set_control_off(&hub.dir, false).unwrap();
	let (status, v) = call(&hub, "POST", "/api/v1/admin/clients/id-monitor/revoke", "owner", "{}").await;
	assert_eq!((status, &v["changed"]), (StatusCode::OK, &json!(["id-monitor"])));
	assert_eq!(call(&hub, "GET", "/api/v1/ui", "monitor", "").await.0, StatusCode::UNAUTHORIZED);
	remove_test_dir(&hub.dir);
}

fn same_shape_of_pending(p: &Value) {
	let fixture: Value =
		serde_json::from_str(&std::fs::read_to_string(format!("{}/fixtures/0.11/admin-pairing.json", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap();
	for k in fixture["pending"].as_object().unwrap().keys() {
		assert!(p.get(k).is_some(), "admin/pairing has no {k}: {p}");
	}
	let qr = &p["web"]["qr"];
	let size = qr["size"].as_u64().unwrap() as usize;
	assert_eq!(qr["modules"].as_str().unwrap().len(), size * size);
	assert!(p["web"]["link"].as_str().unwrap().contains("/#pair="));
}

#[tokio::test]
async fn a_drop_in_written_from_the_page() {
	let (hub, _tx) = hub(true);
	let save = |text: &str, base: Value| json!({ "name": "elamond.d/mine.toml", "text": text, "base": base }).to_string();
	let good = "[dashboards.mine]\ntitle = \"Mine\"\nwidgets = [{ type = \"text\", text = \"Hello\" }]\n[profiles.default]\ndashboards = [\"overview\", \"mine\"]\n";
	// Broken: 422 with the file and line, nothing written.
	let broken = "[dashboards.mine]\ntitle = \"Mine\"\nwidgets = [{ type = \"gauge\", value = \"cpu.usage\", span = 9 }]\n";
	let broken_req = json!({ "name": "elamond.d/mine.toml", "text": broken });
	let (status, v) = call(&hub, "POST", "/api/v1/admin/file/check", "owner", &broken_req.to_string()).await;
	assert_eq!((status, v["sound"].as_bool()), (StatusCode::OK, Some(false)), "{v}");
	assert_eq!(v["mistakes"][0]["file"], "elamond.d/mine.toml");
	assert_eq!(v["mistakes"][0]["line"], 3);
	let sound_req = json!({ "name": "elamond.d/mine.toml", "text": good });
	let (_, sound) = call(&hub, "POST", "/api/v1/admin/file/check", "owner", &sound_req.to_string()).await;
	assert_eq!(sound["sound"], true, "{sound}");
	let fixture = json!({ "broken": { "request": broken_req, "response": v }, "sound": { "request": sound_req, "response": sound } });
	crate::fixtures::check_text("0.11/admin-check.json", &(serde_json::to_string_pretty(&fixture).unwrap() + "\n"));
	let (status, v) = call(&hub, "POST", "/api/v1/admin/file/save", "owner", &save(broken, Value::Null)).await;
	assert_eq!((status, v["error"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("bad_settings")), "{v}");
	assert!(!hub.place.dropins().join("mine.toml").exists());
	// Sound: written, taken up at once, the owner's ui has the dashboard.
	let generation = hub.config_gen();
	let (status, v) = call(&hub, "POST", "/api/v1/admin/file/save", "owner", &save(good, Value::Null)).await;
	assert_eq!(status, StatusCode::OK, "{v}");
	assert!(v["generation"].as_u64().unwrap() > generation);
	let (_, ui) = call(&hub, "GET", "/api/v1/ui", "owner", "").await;
	assert!(ui["tabs"].as_array().unwrap().iter().any(|t| t == "mine"), "{}", ui["tabs"]);
	let hash = v["hash"].as_str().unwrap().to_string();
	// Saved again with a stale base: 409 and the hash now.
	let (status, v) = call(&hub, "POST", "/api/v1/admin/file/save", "owner", &save(good, json!("0000000000000000"))).await;
	assert_eq!((status, v["error"].as_str(), v["hash"].as_str()), (StatusCode::CONFLICT, Some("changed_on_disk"), Some(hash.as_str())));
	// A new action from the page runs.
	let with_action = format!("{good}[actions.hello]\nname = \"Hello\"\ncommand = [\"echo\", \"hello from the page\"]\nconfirm = false\n");
	let (status, v) = call(&hub, "POST", "/api/v1/admin/file/save", "owner", &save(&with_action, json!(hash))).await;
	assert_eq!(status, StatusCode::OK, "{v}");
	let backup = v["backup"].as_str().unwrap().to_string();
	let (status, v) = call(&hub, "POST", "/api/v1/actions/hello", "owner", "{}").await;
	assert_eq!(status, StatusCode::OK, "{v}");
	assert!(v["output"].as_str().unwrap().contains("hello from the page"), "{v}");
	// Back to the backup.
	let (_, file) = call(&hub, "GET", "/api/v1/admin/file?name=elamond.d/mine.toml", "owner", "").await;
	same_shape("admin-file", &file);
	let (status, v) = call(&hub, "POST", "/api/v1/admin/file/restore", "owner", &json!({ "name": "elamond.d/mine.toml", "backup": backup, "base": file["hash"] }).to_string()).await;
	assert_eq!(status, StatusCode::OK, "{v}");
	assert_eq!(std::fs::read_to_string(hub.place.dropins().join("mine.toml")).unwrap(), good);
	// A name out of the folder: refused.
	for bad in ["../x.toml", "elamond.d/../../x.toml", "/etc/passwd", "elamond.d/x.sh"] {
		let (status, v) = call(&hub, "POST", "/api/v1/admin/file/save", "owner", &json!({ "name": bad, "text": "", "base": null }).to_string()).await;
		assert_eq!((status, v["error"].as_str()), (StatusCode::BAD_REQUEST, Some("bad_name")), "{bad}");
	}
	// The activity log says what, never what the file holds.
	let log = hub.activity.newest(50);
	assert!(log.iter().any(|e| e.what == "settings: saved elamond.d/mine.toml (5 lines)" && e.result == "ok"), "{log:?}");
	assert!(log.iter().all(|e| !e.what.contains("hello from the page") && !e.message.as_deref().unwrap_or("").contains("hello from the page")));
	remove_test_dir(&hub.dir);
}

#[tokio::test]
async fn a_main_file_from_nix_is_only_read() {
	let (hub, _tx) = hub(true);
	// The main file a link, as Home Manager makes it (to a file out of the folder here).
	let elsewhere = hub.dir.join("store-elamond.toml");
	std::fs::rename(hub.place.main(), &elsewhere).unwrap();
	std::os::unix::fs::symlink(&elsewhere, hub.place.main()).unwrap();
	let (_, file) = call(&hub, "GET", "/api/v1/admin/file?name=elamond.toml", "owner", "").await;
	assert_eq!(file["writable"], false, "{file}");
	let (status, v) = call(&hub, "POST", "/api/v1/admin/file/save", "owner", &json!({ "name": "elamond.toml", "text": "schema = 3\n", "base": file["hash"] }).to_string()).await;
	assert_eq!((status, v["error"].as_str()), (StatusCode::FORBIDDEN, Some("read_only")));
	remove_test_dir(&hub.dir);
}

#[tokio::test]
async fn the_dashboard_editor() {
	let (hub, _tx) = hub(true);
	let (status, d) = call(&hub, "GET", "/api/v1/admin/editor", "owner", "").await;
	assert_eq!(status, StatusCode::OK, "{d}");
	// The contract's fixture is this description, as the test settings make it.
	crate::fixtures::check_text("0.11/editor.json", &(serde_json::to_string_pretty(&d).unwrap() + "\n"));
	for t in d["types"].as_array().unwrap() {
		for k in t["keys"].as_array().unwrap().iter().chain(d["common"].as_array().unwrap()) {
			for field in ["key", "kind", "label", "hint"] {
				assert!(k[field].is_string(), "{}: {k}", t["type"]);
			}
		}
	}
	let types: Vec<&str> = d["types"].as_array().unwrap().iter().filter_map(|t| t["type"].as_str()).collect();
	assert_eq!(types, crate::settings::WIDGET_TYPES);
	assert_eq!(d["hash"], Value::Null);
	// An admin doesn't see it.
	assert_eq!(call(&hub, "GET", "/api/v1/admin/editor", "admin", "").await.0, StatusCode::FORBIDDEN);
	// A new dashboard of three widgets, written to the editor's file, and a tab for it.
	let body = json!({ "id": "mine", "dashboard": { "title": "Mine", "icon": "gauge", "widgets": [
		{ "type": "gauge", "label": "CPU", "value": "cpu.usage", "max": 100, "span": 2 },
		{ "type": "button", "label": "Echo", "action": "echo", "params": { "word": "hi" } },
		{ "type": "graph", "label": "Memory", "values": ["mem.percent"] },
	] }, "base": null });
	let (status, v) = call(&hub, "POST", "/api/v1/admin/editor/dashboard", "owner", &body.to_string()).await;
	assert_eq!(status, StatusCode::OK, "{v}");
	let file = hub.place.dropins().join("zz-editor.toml");
	let text = std::fs::read_to_string(&file).unwrap();
	assert!(text.starts_with(super::editor::HEADER) && text.contains("[[dashboards.mine.widgets]]"), "{text}");
	let (status, v) = call(
		&hub,
		"POST",
		"/api/v1/admin/editor/profile",
		"owner",
		&json!({ "id": d["profile"], "profile": { "dashboards": ["overview", "mine"] }, "base": v["hash"] }).to_string(),
	)
	.await;
	assert_eq!(status, StatusCode::OK, "{v}");
	let (_, ui) = call(&hub, "GET", "/api/v1/ui", "owner", "").await;
	let mine = ui["dashboards"].as_array().unwrap().iter().find(|d| d["id"] == "mine").expect("the new dashboard in ui");
	assert_eq!(mine["widgets"].as_array().unwrap().len(), 3);
	// It writes no command.
	let (status, v) = call(
		&hub,
		"POST",
		"/api/v1/admin/editor/dashboard",
		"owner",
		&json!({ "id": "mine", "dashboard": { "title": "Mine", "command": ["sh", "-c", "true"] }, "base": v["hash"] }).to_string(),
	)
	.await;
	assert_eq!((status, v["error"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("bad_settings")), "{v}");
	assert!(!std::fs::read_to_string(&file).unwrap().contains("command"));
	// An existing dashboard overridden, and back to the files.
	let (_, d) = call(&hub, "GET", "/api/v1/admin/editor", "owner", "").await;
	let overview = d["dashboards"].as_array().unwrap().iter().find(|x| x["id"] == "overview").unwrap().clone();
	let mut def = overview["def"].clone();
	def["title"] = json!("My overview");
	let (status, v) = call(&hub, "POST", "/api/v1/admin/editor/dashboard", "owner", &json!({ "id": "overview", "dashboard": def, "base": d["hash"] }).to_string()).await;
	assert_eq!(status, StatusCode::OK, "{v}");
	let (_, ui) = call(&hub, "GET", "/api/v1/ui", "owner", "").await;
	assert_eq!(ui["dashboards"].as_array().unwrap().iter().find(|x| x["id"] == "overview").unwrap()["title"], "My overview");
	// A preview of a draft writes nothing.
	let before = std::fs::read_to_string(&file).unwrap();
	let (status, p) = call(
		&hub,
		"POST",
		"/api/v1/admin/preview",
		"owner",
		&json!({ "editor": { "dashboards": { "draft": { "title": "Draft", "widgets": [{ "type": "gauge", "value": "cpu.usage", "max": 100 }] } } }, "dashboard": "draft" }).to_string(),
	)
	.await;
	assert_eq!(status, StatusCode::OK, "{p}");
	assert!(p["ui"]["tabs"].as_array().unwrap().iter().any(|t| t == "draft"));
	let preview_fixture = json!({
		"request": { "editor": { "dashboards": { "draft": { "title": "Draft", "widgets": [{ "type": "gauge", "value": "cpu.usage", "max": 100 }] } } }, "dashboard": "draft" },
		"response": p,
	});
	crate::fixtures::check_text("0.11/admin-preview.json", &(serde_json::to_string_pretty(&preview_fixture).unwrap() + "\n"));
	assert_eq!(p["data"]["dashboard"], "draft");
	assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
	// A dashboard with `for_each`: its first item's.
	let (status, p) = call(&hub, "POST", "/api/v1/admin/preview", "owner", &json!({ "dashboard": "gpu" }).to_string()).await;
	assert_eq!(status, StatusCode::OK, "{p}");
	assert!(p["data"]["dashboard"].as_str().unwrap().starts_with("gpu."), "{}", p["data"]["dashboard"]);
	// What the files below the editor's set, by key.
	let (_, d2) = call(&hub, "GET", "/api/v1/admin/editor", "owner", "").await;
	let cpu = d2["dashboards"].as_array().unwrap().iter().find(|x| x["id"] == "cpu").unwrap();
	assert!(cpu["below"]["title"]["line"].is_u64(), "{cpu}");
	let (status, v) = call(&hub, "POST", "/api/v1/admin/editor/revert", "owner", &json!({ "dashboard": "overview", "base": v["hash"] }).to_string()).await;
	assert_eq!(status, StatusCode::OK, "{v}");
	let (_, ui) = call(&hub, "GET", "/api/v1/ui", "owner", "").await;
	assert_eq!(ui["dashboards"].as_array().unwrap().iter().find(|x| x["id"] == "overview").unwrap()["title"], "Overview");
	// Its own dashboard deleted, with its tab.
	let (status, v) = call(&hub, "POST", "/api/v1/admin/editor/delete", "owner", &json!({ "dashboard": "mine", "base": v["hash"] }).to_string()).await;
	assert_eq!(status, StatusCode::OK, "{v}");
	let (_, ui) = call(&hub, "GET", "/api/v1/ui", "owner", "").await;
	assert!(!ui["tabs"].as_array().unwrap().iter().any(|t| t == "mine"));
	remove_test_dir(&hub.dir);
}
