//! The web page elamond serves (docs/protocol.md, "The page"): its files, built in from web/ by
//! build.rs, with the headers that keep it to itself; a browser's session (the cookie it pairs with)
//! and where a request by that cookie must come from to change anything.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::consts::protocol;
use crate::errors::ErrorCode;
use crate::server::{error, Hub};

include!(concat!(env!("OUT_DIR"), "/web_files.rs"));

/// The page's Content-Security-Policy: its own files only, no inline script or style, no frames.
pub const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data: blob:; connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";

/// A file of the page: its bytes, its type and its tag (16 hex characters of its SHA-256).
struct Served {
	bytes: &'static [u8],
	kind: &'static str,
	etag: String,
}

/// The page's files by path, tagged once.
fn served() -> &'static HashMap<&'static str, Served> {
	static FILES_BY_PATH: OnceLock<HashMap<&'static str, Served>> = OnceLock::new();
	FILES_BY_PATH.get_or_init(|| {
		FILES
			.iter()
			.map(|(path, bytes)| (*path, Served { bytes, kind: kind_of(path), etag: format!("\"{}\"", &crate::store::sha256_hex(bytes)[..16]) }))
			.collect()
	})
}

/// A file's type by its extension.
fn kind_of(path: &str) -> &'static str {
	match path.rsplit('.').next().unwrap_or("") {
		"html" => "text/html; charset=utf-8",
		"js" | "mjs" => "text/javascript; charset=utf-8",
		"css" => "text/css; charset=utf-8",
		"svg" => "image/svg+xml",
		"png" => "image/png",
		"webmanifest" => "application/manifest+json",
		"json" => "application/json",
		"txt" => "text/plain; charset=utf-8",
		_ => "application/octet-stream",
	}
}

/// `GET /`: the page.
pub async fn index(State(hub): State<Arc<Hub>>, headers: HeaderMap) -> Response {
	file(&hub, "index.html", &headers)
}

/// `GET /web/{*path}`: its scripts, styles, icon and manifest.
pub async fn asset(State(hub): State<Arc<Hub>>, Path(path): Path<String>, headers: HeaderMap) -> Response {
	file(&hub, &path, &headers)
}

/// A file of the page with its headers; 304 to an `If-None-Match` with its tag; 404 for a file it
/// hasn't, or with `[web] page = false`.
fn file(hub: &Hub, path: &str, headers: &HeaderMap) -> Response {
	if !hub.config().web.page {
		return error(ErrorCode::NotFound);
	}
	let Some(f) = served().get(path) else { return error(ErrorCode::NotFound) };
	let fresh = headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()).is_some_and(|tags| tags.split(',').any(|t| t.trim() == f.etag));
	let mut res = if fresh { StatusCode::NOT_MODIFIED.into_response() } else { (StatusCode::OK, f.bytes).into_response() };
	let h = res.headers_mut();
	h.insert(header::CONTENT_TYPE, HeaderValue::from_static(f.kind));
	h.insert(header::ETAG, HeaderValue::from_str(&f.etag).unwrap_or(HeaderValue::from_static("\"\"")));
	h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
	h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(CSP));
	h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
	h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
	h.insert("cross-origin-opener-policy", HeaderValue::from_static("same-origin"));
	res
}

/// Every answer of `/api/`: never kept by a cache, never read as another type than it says.
pub async fn api_headers(req: Request, next: axum::middleware::Next) -> Response {
	let mut res = next.run(req).await;
	let h = res.headers_mut();
	h.entry(header::CACHE_CONTROL).or_insert(HeaderValue::from_static("no-store"));
	h.entry(header::X_CONTENT_TYPE_OPTIONS).or_insert(HeaderValue::from_static("nosniff"));
	res
}

// ---- a browser's session -----------------------------------------------------------------

/// The cookie of the service `id`: `__Host-elamon-` and the first 8 of its id, so services on other
/// ports of one host (a test one, another computer's forwarded) keep cookies of their own.
pub fn cookie_name(id: &str) -> String {
	format!("{}-{}", protocol::COOKIE, id.get(..8).unwrap_or(id))
}

/// The token in a request's cookie `name`, if it has one.
pub fn cookie_token(headers: &HeaderMap, name: &str) -> Option<String> {
	headers
		.get_all(header::COOKIE)
		.iter()
		.filter_map(|v| v.to_str().ok())
		.flat_map(|v| v.split(';'))
		.filter_map(|c| c.trim().split_once('='))
		.find(|(n, _)| *n == name)
		.map(|(_, value)| value.trim().trim_matches('"').to_string())
		.filter(|t| !t.is_empty())
}

/// The cookie `name` that holds `token` for `days`.
pub fn set_cookie(name: &str, token: &str, days: u64) -> HeaderValue {
	let max_age = days.saturating_mul(86_400);
	HeaderValue::from_str(&format!("{name}={token}; Max-Age={max_age}; Path=/; Secure; HttpOnly; SameSite=Strict")).unwrap_or(HeaderValue::from_static(""))
}

/// The cookie `name` cleared: a browser forgets the token.
pub fn clear_cookie(name: &str) -> HeaderValue {
	HeaderValue::from_str(&format!("{name}=; Max-Age=0; Path=/; Secure; HttpOnly; SameSite=Strict")).unwrap_or(HeaderValue::from_static(""))
}

/// Whether a request comes from the page itself: its `Origin` is `https://` and the `Host` it was
/// sent to, or, without `Origin`, its `Sec-Fetch-Site` says `same-origin`. A browser sends one or
/// the other with every request that changes anything; another site's page can't make them say so.
pub fn same_origin(headers: &HeaderMap) -> bool {
	let host = headers.get(header::HOST).and_then(|v| v.to_str().ok()).map(str::trim).filter(|h| !h.is_empty());
	match headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
		Some(origin) => host.is_some_and(|h| origin.trim().eq_ignore_ascii_case(&format!("https://{h}"))),
		None => headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()).is_some_and(|s| s.trim() == "same-origin"),
	}
}

/// Whether a request by a browser's cookie needs to come from the page: anything but reading.
pub fn changes(method: &Method) -> bool {
	!matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

/// 403 `bad_origin`.
pub fn bad_origin() -> Response {
	error(ErrorCode::BadOrigin)
}

/// A browser named after itself and its system, from its `User-Agent`: "Firefox on Linux". Nothing
/// else of the header is kept.
pub fn browser_name(user_agent: Option<&str>) -> String {
	let ua = user_agent.unwrap_or("");
	let has = |s: &str| ua.contains(s);
	let browser = if has("Firefox/") || has("FxiOS/") {
		"Firefox"
	} else if has("Edg/") || has("EdgA/") || has("EdgiOS/") {
		"Edge"
	} else if has("OPR/") {
		"Opera"
	} else if has("SamsungBrowser/") {
		"Samsung Internet"
	} else if has("Chrome/") || has("CriOS/") || has("Chromium/") {
		"Chrome"
	} else if has("Safari/") {
		"Safari"
	} else {
		return "Browser".into();
	};
	let system = if has("Android") {
		Some("Android")
	} else if has("iPhone") || has("iPad") {
		Some("iOS")
	} else if has("CrOS") {
		Some("ChromeOS")
	} else if has("Windows") {
		Some("Windows")
	} else if has("Mac OS X") || has("Macintosh") {
		Some("macOS")
	} else if has("Linux") || has("X11") {
		Some("Linux")
	} else {
		None
	};
	match system {
		Some(s) => format!("{browser} on {s}"),
		None => browser.into(),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
		let mut h = HeaderMap::new();
		for (k, v) in pairs {
			h.append(*k, HeaderValue::from_str(v).unwrap());
		}
		h
	}

	#[test]
	fn the_page_is_built_in_without_its_tests() {
		let paths: Vec<&str> = FILES.iter().map(|(p, _)| *p).collect();
		for p in ["index.html", "style.css", "manifest.webmanifest", "js/main.js", "js/api.js", "js/dom.js", "js/version.js"] {
			assert!(paths.contains(&p), "{p} is missing from {paths:?}");
		}
		assert!(paths.iter().all(|p| !p.starts_with("test/") && *p != "jsconfig.json"), "{paths:?}");
		// Every file of web/ but those left out, and nothing else.
		let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web");
		let mut on_disk = Vec::new();
		fn walk(d: &std::path::Path, base: &std::path::Path, out: &mut Vec<String>) {
			for e in std::fs::read_dir(d).unwrap().flatten() {
				let p = e.path();
				let rel = p.strip_prefix(base).unwrap().to_string_lossy().into_owned();
				if rel.starts_with("test") || rel == "jsconfig.json" || rel.ends_with(".md") || rel.starts_with('.') {
					continue;
				}
				if p.is_dir() { walk(&p, base, out) } else { out.push(rel) }
			}
		}
		walk(&base, &base, &mut on_disk);
		on_disk.sort();
		let mut built: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
		built.sort();
		assert_eq!(built, on_disk);
	}

	#[test]
	fn the_page_says_the_version_of_its_service() {
		let v = served().get("js/version.js").unwrap();
		let text = std::str::from_utf8(v.bytes).unwrap();
		assert!(text.contains(&format!("'{}'", env!("CARGO_PKG_VERSION"))), "js/version.js says another version than Cargo.toml: {text}");
	}

	#[test]
	fn no_file_of_the_page_writes_markup_or_runs_text() {
		for (path, bytes) in FILES.iter().filter(|(p, _)| p.ends_with(".js") || p.ends_with(".html")) {
			let text = std::str::from_utf8(bytes).unwrap();
			for bad in ["innerHTML", "outerHTML", "insertAdjacentHTML", "document.write", "eval(", "new Function"] {
				assert!(!text.contains(bad), "{path} has {bad}");
			}
			if path.ends_with(".html") {
				assert!(!text.contains("<script>") && !text.contains(" style=") && !text.contains("<style"), "{path} has inline script or style");
			}
		}
	}

	#[test]
	fn the_cookie() {
		let name = cookie_name("0123456789abcdef");
		assert_eq!(name, "__Host-elamon-01234567");
		assert_eq!(cookie_token(&headers(&[("cookie", "a=1; __Host-elamon-01234567=tok; b=2")]), &name), Some("tok".into()));
		assert_eq!(cookie_token(&headers(&[("cookie", "a=1"), ("cookie", "__Host-elamon-01234567=t2")]), &name), Some("t2".into()));
		// Another service's on the same host isn't this one's.
		assert_eq!(cookie_token(&headers(&[("cookie", "__Host-elamon-89abcdef=tok; __Host-elamon-01234567=")]), &name), None);
		assert_eq!(cookie_token(&headers(&[]), &name), None);
		let set = set_cookie(&name, "tok", 30);
		assert_eq!(set.to_str().unwrap(), "__Host-elamon-01234567=tok; Max-Age=2592000; Path=/; Secure; HttpOnly; SameSite=Strict");
		assert!(clear_cookie(&name).to_str().unwrap().starts_with("__Host-elamon-01234567=; Max-Age=0;"));
	}

	#[test]
	fn a_change_comes_from_the_page_alone() {
		assert!(same_origin(&headers(&[("host", "192.0.2.7:47823"), ("origin", "https://192.0.2.7:47823")])));
		assert!(same_origin(&headers(&[("host", "Desk.example.net:47823"), ("origin", "https://desk.example.net:47823")])));
		assert!(same_origin(&headers(&[("host", "[2001:db8::1]:47823"), ("origin", "https://[2001:db8::1]:47823")])));
		assert!(!same_origin(&headers(&[("host", "192.0.2.7:47823"), ("origin", "https://evil.example:47823")])));
		assert!(!same_origin(&headers(&[("host", "192.0.2.7:47823"), ("origin", "http://192.0.2.7:47823")])));
		assert!(!same_origin(&headers(&[("host", "192.0.2.7:47823"), ("origin", "null")])));
		assert!(!same_origin(&headers(&[("origin", "https://192.0.2.7:47823")])));
		assert!(same_origin(&headers(&[("host", "192.0.2.7:47823"), ("sec-fetch-site", "same-origin")])));
		assert!(!same_origin(&headers(&[("host", "192.0.2.7:47823"), ("sec-fetch-site", "cross-site")])));
		assert!(!same_origin(&headers(&[("host", "192.0.2.7:47823")])));
		assert!(changes(&Method::POST) && changes(&Method::DELETE) && !changes(&Method::GET) && !changes(&Method::HEAD));
	}

	#[test]
	fn browsers_by_name() {
		let ff = "Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0";
		let chrome_android = "Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Mobile Safari/537.36";
		let samsung = "Mozilla/5.0 (Linux; Android 15) AppleWebKit/537.36 (KHTML, like Gecko) SamsungBrowser/28.0 Chrome/130.0.0.0 Mobile Safari/537.36";
		let edge = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36 Edg/140.0.0.0";
		let safari = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1";
		assert_eq!(browser_name(Some(ff)), "Firefox on Linux");
		assert_eq!(browser_name(Some(chrome_android)), "Chrome on Android");
		assert_eq!(browser_name(Some(samsung)), "Samsung Internet on Android");
		assert_eq!(browser_name(Some(edge)), "Edge on Windows");
		assert_eq!(browser_name(Some(safari)), "Safari on iOS");
		assert_eq!(browser_name(Some("curl/8.0")), "Browser");
		assert_eq!(browser_name(None), "Browser");
	}
}
