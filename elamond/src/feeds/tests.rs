//! The sources of kind `command`, `http` and `file`, read: commands and scripts of the tests' own,
//! a web service on 127.0.0.1, files of a scratch folder; and how often they are read.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::*;
use crate::access::Role;
use crate::settings::{self, Source};
use crate::values::View;

/// A folder of its own for a test.
fn scratch(name: &str) -> PathBuf {
	let dir = std::env::temp_dir().join(format!("elamond-feeds-{name}-{}", data_encoding::HEXLOWER.encode(&crate::store::random_bytes::<6>())));
	std::fs::create_dir_all(&dir).unwrap();
	dir
}

/// The settings `text`, read.
fn config(text: &str) -> Config {
	settings::read_sources(&[Source::new("a.toml", text)]).unwrap_or_else(|m| panic!("{}", settings::join_mistakes(&m)))
}

/// The source `[sources.x]` of `keys`.
fn feed(keys: &str) -> Feed {
	config(&format!("[sources.x]\n{keys}\n")).feeds.remove("x").unwrap()
}

/// A command source running `argv`, with `more` keys.
fn command(argv: &[&str], more: &str) -> Feed {
	let argv: Vec<String> = argv.iter().map(|a| format!("{a:?}")).collect();
	feed(&format!("kind = \"command\"\ncommand = [{}]\n{more}", argv.join(", ")))
}

/// An executable script of `body` in `dir`.
fn script(dir: &Path, name: &str, body: &str) -> String {
	let path = dir.join(name);
	std::fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
	std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
	path.display().to_string()
}

async fn once(feed: &Feed) -> Outcome {
	read(feed, &mut None).await
}

fn error(o: &Outcome) -> String {
	o.result.as_ref().err().map(Failure::text).unwrap_or_default()
}

fn values(o: &Outcome) -> Vec<(String, V)> {
	o.result.clone().unwrap_or_else(|f| panic!("{}", f.text()))
}

/// Whether process `pid` has ended (a zombie its parent hasn't reaped yet counts), waiting up to 5 s.
fn ended(pid: i32) -> bool {
	for _ in 0..50 {
		match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
			Err(_) => return true,
			Ok(s) if s.rsplit_once(')').and_then(|(_, r)| r.split_whitespace().next()) == Some("Z") => return true,
			Ok(_) => std::thread::sleep(Duration::from_millis(100)),
		}
	}
	false
}

/// The process ids a script wrote to `file`, once it has written `n` of them (within 5 s).
fn pids(file: &Path, n: usize) -> Vec<i32> {
	for _ in 0..50 {
		let got: Vec<i32> = std::fs::read_to_string(file).unwrap_or_default().lines().filter_map(|l| l.trim().parse().ok()).collect();
		if got.len() >= n {
			return got;
		}
		std::thread::sleep(Duration::from_millis(100));
	}
	panic!("{} has fewer than {n} process ids", file.display());
}

// ---- command --------------------------------------------------------------------------------

#[tokio::test]
async fn a_command_prints_its_values() {
	let o = once(&command(&["printf", "state: ok\\nfiles: 1204\\n"], "format = \"lines\"")).await;
	assert_eq!(values(&o), [("state".to_string(), V::Text("ok".into())), ("files".to_string(), V::Int(1204))]);
	assert_eq!(o.exit, Some(0));
	let o = once(&command(&["printf", "{\"a\": {\"b\": [1, 2.5]}}"], "")).await;
	assert_eq!(values(&o), [("a.b.0".to_string(), V::Int(1)), ("a.b.1".to_string(), V::Float(2.5))]);
	// No shell: what would be one's is an argument like any other.
	let o = once(&command(&["printf", "%s", "$(echo no) ; `true`"], "format = \"text\"")).await;
	assert_eq!(values(&o), [("text".to_string(), V::Text("$(echo no) ; `true`".into()))]);
}

#[tokio::test]
async fn a_command_that_exits_with_another_code() {
	let dir = scratch("exit");
	// With values: a good read, its code beside them.
	let s = script(&dir, "some", "echo '{\"done\": 3}'\necho 'something went wrong' >&2\nexit 3\n");
	let o = once(&command(&[&s], "")).await;
	assert_eq!((values(&o), o.exit), (vec![("done".to_string(), V::Int(3))], Some(3)));
	// Without: why, from the first line of stderr, whatever it printed.
	let s = script(&dir, "none", "echo 'not json'\necho\necho '  disk full  ' >&2\necho 'second line' >&2\nexit 7\n");
	let o = once(&command(&[&s], "")).await;
	assert_eq!((error(&o), o.exit), ("exited with 7: disk full".to_string(), Some(7)));
	assert_eq!(o.result.as_ref().unwrap_err().why, "exited with 7");
	let s = script(&dir, "quiet", "exit 2\n");
	assert_eq!(error(&once(&command(&[&s], "format = \"text\"")).await), "exited with 2");
	// Exiting with 0, it reads well when its output is of its format: empty text gives no value.
	let o = once(&command(&["true"], "format = \"text\"")).await;
	assert_eq!((values(&o), o.exit), (vec![], Some(0)));
	let o = once(&command(&["printf", "Status: ok"], "")).await;
	assert!(error(&o).starts_with("not JSON: "), "{}", error(&o));
	assert_eq!(o.exit, Some(0));
	// One that can't start, or is killed, has no code.
	let o = once(&command(&[&dir.join("missing").display().to_string()], "")).await;
	assert_eq!((error(&o), o.exit), ("can't start: No such file or directory".to_string(), None));
	let s = script(&dir, "killed", "kill -9 $$\n");
	assert_eq!(error(&once(&command(&[&s], "")).await), "ended by signal 9");
	std::fs::remove_dir_all(&dir).unwrap();
}

/// A command that hangs, having started another that hangs: at the timeout both are killed.
#[tokio::test]
async fn a_command_that_hangs_is_killed_with_what_it_started() {
	let dir = scratch("hang");
	let file = dir.join("pids");
	let s = script(&dir, "hang", "sleep 300 &\necho $! > \"$1\"\necho $$ >> \"$1\"\nexec sleep 301\n");
	let started = Instant::now();
	let o = once(&command(&[&s, &file.display().to_string()], "timeout = 1")).await;
	assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
	assert_eq!((error(&o), o.exit), ("timed out after 1 s".to_string(), None));
	for pid in pids(&file, 2) {
		assert!(ended(pid), "process {pid} still runs");
	}
	// One that ends at once leaves nothing behind either, and isn't waited for.
	let s = script(&dir, "leave", "sleep 300 &\necho $!\n");
	let started = Instant::now();
	let o = once(&command(&[&s], "format = \"text\"\ntimeout = 10")).await;
	assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
	let Some((_, V::Int(pid))) = values(&o).pop() else { panic!("{o:?}") };
	assert!(ended(pid as i32), "process {pid} still runs");
	std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn a_command_that_says_too_much_is_stopped() {
	// It would go on for ever.
	let started = Instant::now();
	let o = once(&command(&["yes"], "format = \"text\"\ntimeout = 10")).await;
	assert_eq!((error(&o), o.exit), ("output over 64 KiB".to_string(), None));
	assert!(started.elapsed() < Duration::from_secs(5));
	// 64 KiB is still read.
	let dir = scratch("size");
	let s = script(&dir, "full", "head -c 65536 /dev/zero | tr '\\0' 'a'\n");
	let o = once(&command(&[&s], "format = \"text\"")).await;
	let Some((_, V::Text(t))) = values(&o).pop() else { panic!() };
	assert_eq!(t.chars().count(), 1000);
	std::fs::remove_dir_all(&dir).unwrap();
}

// ---- http -----------------------------------------------------------------------------------

/// A web service on a port of 127.0.0.1, answering by path; and the heads of the requests it got.
struct Server {
	url: String,
	seen: Arc<Mutex<Vec<String>>>,
	task: tokio::task::JoinHandle<()>,
}

impl Drop for Server {
	fn drop(&mut self) {
		self.task.abort();
	}
}

impl Server {
	async fn start() -> Server {
		let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
		let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
		let seen = Arc::new(Mutex::new(Vec::new()));
		let kept = seen.clone();
		let task = tokio::spawn(async move {
			while let Ok((conn, _)) = listener.accept().await {
				tokio::spawn(answer(conn, kept.clone()));
			}
		});
		Server { url, seen, task }
	}

	fn seen(&self) -> Vec<String> {
		self.seen.lock().unwrap().clone()
	}

	/// An `http` source of `path` here, with `more` keys.
	fn feed(&self, path: &str, more: &str) -> Feed {
		feed(&format!("kind = \"http\"\nurl = \"{}{path}\"\n{more}", self.url))
	}
}

async fn answer(mut conn: tokio::net::TcpStream, seen: Arc<Mutex<Vec<String>>>) {
	let mut head = Vec::new();
	let mut buf = [0u8; 1024];
	while !head.windows(4).any(|w| w == b"\r\n\r\n") {
		match conn.read(&mut buf).await {
			Ok(0) | Err(_) => return,
			Ok(n) => head.extend_from_slice(&buf[..n]),
		}
	}
	let head = String::from_utf8_lossy(&head).into_owned();
	let path = head.split_whitespace().nth(1).unwrap_or("").to_string();
	seen.lock().unwrap().push(head);
	// `/endless` says nothing of its length, and goes on.
	if path == "/endless" {
		let _ = conn.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n").await;
		while conn.write_all(&[b' '; 8192]).await.is_ok() {}
		return;
	}
	let (status, extra, body): (&str, &str, Vec<u8>) = match path.as_str() {
		"/status" => ("200 OK", "", br#"{"state": "ok", "queue": {"length": 3}, "note": null}"#.to_vec()),
		"/text" => ("200 OK", "", b"45000\n".to_vec()),
		"/missing" => ("404 Not Found", "", b"{\"error\": \"no such page\"}".to_vec()),
		"/moved" => ("301 Moved Permanently", "Location: /status\r\n", Vec::new()),
		"/slow" => {
			tokio::time::sleep(Duration::from_secs(10)).await;
			("200 OK", "", b"{}".to_vec())
		}
		"/big" => ("200 OK", "", vec![b' '; 70_000]),
		_ => ("200 OK", "", b"{}".to_vec()),
	};
	let reply = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n", body.len());
	let _ = conn.write_all(reply.as_bytes()).await;
	let _ = conn.write_all(&body).await;
}

/// A token file in `dir` whose first line is `token`, as private as `mode`.
fn token_file(dir: &Path, name: &str, token: &str, mode: u32) -> String {
	let path = dir.join(name);
	std::fs::write(&path, format!("{token}\nsecond line\n")).unwrap();
	std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
	path.display().to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_web_service_is_asked_through_curl() {
	let server = Server::start().await;
	let o = once(&server.feed("/status", "")).await;
	assert_eq!(values(&o), [("state".to_string(), V::Text("ok".into())), ("queue.length".to_string(), V::Int(3))]);
	assert_eq!(o.exit, None);
	assert_eq!(values(&once(&server.feed("/text", "format = \"text\"")).await), [("text".to_string(), V::Int(45000))]);
	// Without a token, no header for it.
	assert!(!server.seen()[0].to_ascii_lowercase().contains("authorization"), "{:?}", server.seen());
	// Another status: the code alone, not what came with it.
	assert_eq!(error(&once(&server.feed("/missing", "")).await), "HTTP 404");
	// A redirect isn't followed: one request.
	let before = server.seen().len();
	assert_eq!(error(&once(&server.feed("/moved", "")).await), "redirect to elsewhere (301), not followed");
	assert_eq!(server.seen().len(), before + 1);
	// Too slow, too much, or nobody there.
	let started = Instant::now();
	assert_eq!(error(&once(&server.feed("/slow", "timeout = 1")).await), "timed out after 1 s");
	assert!(started.elapsed() < Duration::from_secs(4), "{:?}", started.elapsed());
	assert_eq!(error(&once(&server.feed("/big", "")).await), "output over 64 KiB");
	assert_eq!(error(&once(&server.feed("/endless", "")).await), "output over 64 KiB");
	let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
	let url = format!("http://127.0.0.1:{}/status", closed.local_addr().unwrap().port());
	drop(closed);
	let f = once(&feed(&format!("kind = \"http\"\nurl = \"{url}\""))).await.result.unwrap_err();
	assert_eq!(f.why, "couldn't connect");
	assert!(f.detail.as_deref().is_some_and(|d| !d.starts_with("curl:")), "{f:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_token_goes_in_its_header_from_its_file() {
	let server = Server::start().await;
	let dir = scratch("token");
	let path = token_file(&dir, "app.token", "  token-of-the-test  ", 0o600);
	let with = |more: String| server.feed("/status", &more);
	once(&with(format!("token_file = \"{path}\""))).await.result.unwrap();
	once(&with(format!("token_file = \"{path}\"\nheader = \"X-Api-Key\""))).await.result.unwrap();
	let seen = server.seen();
	assert!(seen[0].contains("\r\nAuthorization: Bearer token-of-the-test\r\n"), "{seen:?}");
	assert!(seen[1].contains("\r\nX-Api-Key: token-of-the-test\r\n") && !seen[1].contains("Authorization"), "{seen:?}");
	assert!(seen.iter().all(|s| !s.contains("second line")));
	// A file others may read, an empty one, one that isn't there or isn't a file: no request.
	std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
	assert_eq!(error(&once(&with(format!("token_file = \"{path}\""))).await), "token_file may be read by others: chmod 600 it");
	let empty = token_file(&dir, "empty.token", "   ", 0o600);
	assert_eq!(error(&once(&with(format!("token_file = \"{empty}\""))).await), "token_file is empty");
	let gone = dir.join("gone").display().to_string();
	assert_eq!(error(&once(&with(format!("token_file = \"{gone}\""))).await), "can't read token_file: No such file or directory");
	assert_eq!(error(&once(&with(format!("token_file = \"{}\"", dir.display()))).await), "token_file isn't a regular file");
	assert_eq!(server.seen().len(), 2);
	std::fs::remove_dir_all(&dir).unwrap();
}

/// A token put in its file reaches the web service, and nothing else: not `ui`, a stream's line or
/// a snapshot, the values, an error, the journal, `elamond values` or `elamond check`, nor a backup.
#[test]
fn the_token_goes_to_the_web_service_alone() {
	const MARKER: &str = "marker-7f3a9c2e-token";
	let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
	let server = rt.block_on(Server::start());
	let dir = scratch("marker");
	let path = token_file(&dir, "app.token", MARKER, 0o600);
	let text = format!(
		r#"
default_profile = "owner"
[sources.app]
kind = "http"
url = "{url}/status"
token_file = "{path}"
role = "glance"

[sources.down]
kind = "http"
url = "{url}/missing"
token_file = "{path}"
header = "X-Api-Key"

[profiles.owner]
dashboards = ["d"]
header = ""

[dashboards.d]
title = "D"

[[dashboards.d.widgets]]
id = "state"
type = "status"
value = "app.state"
caption = "{{app.queue.length}} queued, read {{app.age:duration}} ago"

[[dashboards.d.widgets]]
id = "down"
type = "text"
text = "{{down.error}}"
when = "down.error"
"#,
		url = server.url
	);
	let config = config(&text);
	let mut said = Vec::new();
	let mut snapshots = BTreeMap::new();
	for id in ["app", "down"] {
		let mut keeper = Keeper::new(id);
		said.extend(keeper.took(rt.block_on(once(&config.feeds[id])), SystemTime::now()));
		snapshots.insert(id.to_string(), Arc::new(keeper.snap));
	}
	let seen = server.seen();
	assert_eq!(seen.iter().filter(|s| s.contains(MARKER)).count(), 2, "{seen:?}");
	// The values, `ui`, a stream's line.
	let mut values = crate::fixtures::values(&config);
	put(&mut values, &config, &snapshots);
	assert_eq!(values.scalars["down.error"].v, V::Text("HTTP 404".into()));
	let caps = crate::actions::Caps::all(&values.power_profiles);
	let ui = crate::ui::build(&crate::ui::tests::input(&config, &values, &caps, Role::Admin, "owner", None));
	let sorts = std::collections::HashMap::new();
	let s = crate::states::States { view: View::new(&values, Role::Admin, ui.units), control: false, sorts: &sorts, read: crate::ui::Sources::ALL };
	let line = serde_json::Value::Object(crate::states::data(values.t, ui.dashboard("d").unwrap(), None, &s, None));
	assert_eq!(line["widgets"]["d/down"]["text"], "HTTP 404");
	assert_eq!(line["widgets"]["d/state"]["v"], "ok");
	// Errors of the kinds there are with a token, and what the journal says of them.
	let fail = |more: &str| {
		let f = feed(&format!("kind = \"http\"\nurl = \"{}/slow\"\ntoken_file = \"{path}\"\ntimeout = 1\n{more}", server.url));
		let mut keeper = Keeper::new("x");
		let lines = keeper.took(rt.block_on(once(&f)), SystemTime::now());
		(keeper.snap.error.unwrap_or_default(), lines)
	};
	let mut errors = vec![fail(""), fail("header = \"X-Api-Key\"")];
	std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
	errors.push(fail(""));
	std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
	let shown: Vec<&str> = errors.iter().map(|e| e.0.as_str()).collect();
	assert_eq!(shown, ["timed out after 1 s", "timed out after 1 s", "token_file may be read by others: chmod 600 it"]);
	// `elamond values`, as it reads well and as it doesn't.
	let listed = cli::listing(&config, "app", &config.feeds["app"]).unwrap();
	assert!(listed.contains("app.queue.length") && listed.contains("glance"), "{listed}");
	let refused = cli::listing(&config, "down", &config.feeds["down"]).unwrap_err();
	assert_eq!(refused, "source down: HTTP 404");
	// `elamond check`, its warnings and its mistakes.
	let warned = settings::warnings(&config);
	let mistaken = settings::read_sources(&[Source::new("a.toml", text.replace("header = \"X-Api-Key\"", "header = \"X Api Key\""))]).unwrap_err();
	assert!(!mistaken.is_empty());
	// A backup of the service's state, the settings with it.
	let data = dir.join("data");
	std::fs::create_dir_all(&data).unwrap();
	for (name, bytes) in [(crate::identity::ID_FILE, "0123456789abcdef"), (crate::identity::KEY_FILE, "key"), (crate::identity::CERT_FILE, "cert")] {
		std::fs::write(data.join(name), bytes).unwrap();
	}
	let settings_file = dir.join("elamond.toml");
	std::fs::write(&settings_file, &text).unwrap();
	let backup = crate::backup::pack(&crate::backup::collect(&data, &[("elamond.toml".into(), settings_file)]).unwrap()).unwrap();
	let backup = String::from_utf8_lossy(&backup).into_owned();
	assert!(backup.contains("token_file"));
	let every = [
		ui.json(),
		line.to_string(),
		format!("{values:?}"),
		format!("{snapshots:?}"),
		format!("{config:?}"),
		format!("{errors:?}"),
		said.join("\n"),
		listed,
		refused,
		warned.join("\n"),
		settings::join_mistakes(&mistaken),
		backup,
		read::curl_args(&config.feeds["app"], true).join(" "),
	];
	for (i, text) in every.iter().enumerate() {
		assert!(!text.contains(MARKER), "the token is in #{i}: {text}");
	}
	drop(server);
	std::fs::remove_dir_all(&dir).unwrap();
}

// ---- file -----------------------------------------------------------------------------------

#[tokio::test]
async fn a_file_is_read_if_it_is_a_regular_one() {
	let dir = scratch("file");
	let file = |name: &str, bytes: &[u8]| {
		std::fs::write(dir.join(name), bytes).unwrap();
		dir.join(name).display().to_string()
	};
	let of = |path: &str, more: &str| feed(&format!("kind = \"file\"\npath = \"{path}\"\n{more}"));
	let temp = file("temp", b"45000\n");
	assert_eq!(values(&once(&of(&temp, "format = \"text\"")).await), [("text".to_string(), V::Int(45000))]);
	let status = file("status.json", br#"{"state": "ok"}"#);
	assert_eq!(values(&once(&of(&status, "")).await), [("state".to_string(), V::Text("ok".into()))]);
	// /proc's files are regular, of no size, and read.
	let o = once(&of("/proc/self/status", "format = \"lines\"")).await;
	assert!(values(&o).iter().any(|(k, _)| k == "Pid"), "{o:?}");
	// A FIFO with nobody writing: not waited for.
	let fifo = dir.join("fifo");
	let c = std::ffi::CString::new(fifo.display().to_string()).unwrap();
	// SAFETY: mkfifo(3) with a path that is a NUL-terminated string.
	assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
	let started = Instant::now();
	assert_eq!(error(&once(&of(&fifo.display().to_string(), "")).await), "not a regular file");
	assert!(started.elapsed() < Duration::from_secs(1));
	assert_eq!(error(&once(&of("/dev/null", "")).await), "not a regular file");
	assert_eq!(error(&once(&of(&dir.display().to_string(), "")).await), "not a regular file");
	let big = file("big", &vec![b'1'; 70_000]);
	assert_eq!(error(&once(&of(&big, "format = \"text\"")).await), "output over 64 KiB");
	let full = file("full", &vec![b'1'; 65_536]);
	assert!(once(&of(&full, "format = \"text\"")).await.result.is_ok());
	let gone = dir.join("gone").display().to_string();
	assert_eq!(error(&once(&of(&gone, "")).await), "can't read: No such file or directory");
	std::fs::remove_dir_all(&dir).unwrap();
}

// ---- what is kept of the reads ----------------------------------------------------------------

#[test]
fn reads_are_kept_and_said_once() {
	let at = |s: u64| UNIX_EPOCH + Duration::from_secs(s);
	let ok = |values: Vec<(String, V)>, exit: Option<i32>| Outcome { result: Ok(values), exit, cut: false };
	let bad = |why: &str, detail: &str, exit: Option<i32>| Outcome { result: Err(Failure::with(why, detail)), exit, cut: false };
	let mut k = Keeper::new("job");
	assert!(k.took(ok(vec![("files".into(), V::Int(3)), ("state".into(), V::Text("ok".into()))], Some(0)), at(100)).is_empty());
	assert_eq!(k.snap.kept, ["job.files"]);
	// Failing: once in the journal, without what the command wrote to stderr; its values go.
	let said = k.took(bad("exited with 7", "can't open /home/someone/x", Some(7)), at(110));
	assert_eq!(said, ["source job: exited with 7"]);
	assert_eq!(k.snap.error.as_deref(), Some("exited with 7: can't open /home/someone/x"));
	assert!(k.took(bad("timed out after 5 s", "", None), at(120)).is_empty());
	assert_eq!((k.snap.error.as_deref(), k.snap.exit, k.snap.scalars.len(), k.snap.ok_at), (Some("timed out after 5 s"), None, 0, Some(at(100))));
	// What it had is known still, as its own are.
	assert_eq!(
		k.snap.known.iter().map(|(a, f)| format!("{a} {f}")).collect::<Vec<_>>(),
		["job.age duration", "job.error text", "job.exit number", "job.files number", "job.state text"]
	);
	assert_eq!(k.took(ok(vec![("files".into(), V::Int(4))], Some(0)), at(130)), ["source job: reads again"]);
	assert_eq!(k.snap.error, None);
	// Values left out: said once.
	assert_eq!(k.took(Outcome { cut: true, ..ok(vec![], None) }, at(140)).len(), 1);
	assert!(k.took(Outcome { cut: true, ..ok(vec![], None) }, at(150)).is_empty());
	// 256 addresses known at most, beside its own.
	let mut k = Keeper::new("many");
	for round in 0..2 {
		let values = (0..200).map(|i| (format!("r{round}.{i}"), V::Int(i))).collect();
		k.took(ok(values, None), at(1));
	}
	assert_eq!(k.snap.known.len(), 256 + 1);
}

/// The values the collector makes of a source's snapshot at a reading's `t`.
#[test]
fn a_reading_has_the_sources_values_and_their_age() {
	let config = config(
		"[sources.job]\nkind = \"command\"\ncommand = [\"x\"]\nhistory = true\nrole = \"operate\"\n[sources.web]\nkind = \"http\"\nurl = \"http://localhost:8080\"\n",
	);
	let mut k = Keeper::new("job");
	let outcome = Outcome { result: Ok(vec![("files".into(), V::Int(3)), ("ok".into(), V::Bool(true))]), exit: Some(1), cut: false };
	k.took(outcome, UNIX_EPOCH + Duration::from_millis(1_700_000_000_000));
	let mut values = Values { t: 1_700_000_012_999, ..Default::default() };
	put(&mut values, &config, &BTreeMap::from([("job".to_string(), Arc::new(k.snap))]));
	let get = |a: &str| values.scalars.get(a).map(|s| (s.v.clone(), s.unit, s.format));
	assert_eq!(get("job.age"), Some((V::Int(12), "s", "duration")));
	assert_eq!(get("job.exit"), Some((V::Int(1), "", "number")));
	assert_eq!(get("job.files"), Some((V::Int(3), "", "number")));
	assert_eq!(get("job.ok"), Some((V::Bool(true), "", "bool")));
	assert_eq!(values.kept.iter().collect::<Vec<_>>(), ["job.files"]);
	// Every source has its role from the start, read or not.
	assert_eq!((values.feeds["job"], values.feeds["web"]), (Role::Operate, Role::Monitor));
	assert!(!values.scalars.keys().any(|a| a.starts_with("web.")));
}

// ---- running --------------------------------------------------------------------------------

fn reads(rt: &Runtime, id: &str) -> u64 {
	rt.snapshots().get(id).map_or(0, |s| s.reads)
}

async fn sleep_ms(ms: u64) {
	tokio::time::sleep(Duration::from_millis(ms)).await;
}

fn ids(list: &[&str]) -> BTreeSet<String> {
	list.iter().map(|s| s.to_string()).collect()
}

/// A source is read as it starts, then every `interval` while a client shows it or a graph tracks
/// it, and while its history is kept; otherwise not at all. Needed again, at once if its last read
/// is old enough.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_source_is_read_while_it_is_needed() {
	let config = config(
		r#"
[sources.a]
kind = "command"
command = ["printf", "1"]
interval = 1

[sources.kept]
kind = "command"
command = ["printf", "1"]
interval = 3600
history = true

[sources.slow]
kind = "command"
command = ["printf", "1"]
interval = 3600
"#,
	);
	let rt = Runtime::default();
	rt.start(tokio::runtime::Handle::current(), &config);
	let none = BTreeSet::new();
	rt.set_demand(&none, &none);
	sleep_ms(1500).await;
	// Each once, as it starts; then only what is needed.
	assert_eq!((reads(&rt, "a"), reads(&rt, "kept"), reads(&rt, "slow")), (1, 1, 1));
	// On screen: at once, its last read being older than a second, then every second.
	rt.set_demand(&ids(&["a"]), &none);
	sleep_ms(2600).await;
	assert!((3..=5).contains(&reads(&rt, "a")), "{}", reads(&rt, "a"));
	// Off: no more.
	rt.set_demand(&none, &none);
	sleep_ms(300).await;
	let n = reads(&rt, "a");
	sleep_ms(2000).await;
	assert_eq!(reads(&rt, "a"), n);
	// A graph's: at once, its last read being older than a second.
	rt.set_demand(&none, &ids(&["a.value"]));
	sleep_ms(500).await;
	assert_eq!(reads(&rt, "a"), n + 1);
	// Its last read not that old: not at once.
	rt.set_demand(&ids(&["slow"]), &none);
	sleep_ms(500).await;
	assert_eq!((reads(&rt, "slow"), reads(&rt, "kept")), (1, 1));
	rt.stop().await;
	assert!(rt.snapshots().is_empty());
}

/// A source taken out of the settings, or changed, stops, and its read in flight with it; the
/// service stopping stops them all.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_source_changed_or_taken_out_stops() {
	let dir = scratch("reload");
	let s = script(&dir, "hang", "sleep 300 &\necho $! >> \"$1\"\necho $$ >> \"$1\"\nexec sleep 301\n");
	let hang = |file: &str, timeout: u64| config(&format!("[sources.hang]\nkind = \"command\"\ncommand = [\"{s}\", \"{}\"]\ntimeout = {timeout}\n", dir.join(file).display()));
	let on = ids(&["hang"]);
	let none = BTreeSet::new();
	let rt = Runtime::default();
	rt.start(tokio::runtime::Handle::current(), &hang("first", 60));
	let first = pids(&dir.join("first"), 2);
	// Changed: the old read is killed, the new source reads anew.
	rt.apply(&hang("second", 59));
	for pid in &first {
		assert!(ended(*pid), "process {pid} still runs");
	}
	assert_eq!(reads(&rt, "hang"), 0);
	let second = pids(&dir.join("second"), 2);
	// Taken out.
	rt.apply(&config(""));
	for pid in &second {
		assert!(ended(*pid), "process {pid} still runs");
	}
	assert!(rt.snapshots().is_empty());
	// The service stopping.
	rt.apply(&hang("third", 60));
	rt.set_demand(&on, &none);
	let third = pids(&dir.join("third"), 2);
	rt.stop().await;
	for pid in &third {
		assert!(ended(*pid), "process {pid} still runs");
	}
	// Before it starts, nothing runs.
	let idle = Runtime::default();
	idle.apply(&hang("fourth", 60));
	sleep_ms(300).await;
	assert!(idle.snapshots().is_empty() && !dir.join("fourth").exists());
	std::fs::remove_dir_all(&dir).unwrap();
}
