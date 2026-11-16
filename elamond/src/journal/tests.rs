//! The journal, read: lines as journalctl prints them, the states a stream makes of them, and a
//! journalctl of the tests' own (`fake`) followed, ending, hanging, flooding and killed.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use super::*;

/// A line as `journalctl -o json --output-fields=…` prints it: the time (Unix µs), the priority, the
/// system's unit, the user's, the program's name and the message.
pub fn json_line(us: u64, priority: u8, unit: Option<&str>, user_unit: Option<&str>, id: &str, text: &str) -> String {
	let mut o = serde_json::Map::new();
	o.insert("__CURSOR".into(), format!("s=0;i={us}").into());
	o.insert("__REALTIME_TIMESTAMP".into(), us.to_string().into());
	o.insert("PRIORITY".into(), priority.to_string().into());
	if let Some(u) = unit {
		o.insert("_SYSTEMD_UNIT".into(), u.into());
	}
	if let Some(u) = user_unit {
		o.insert("_SYSTEMD_USER_UNIT".into(), u.into());
	}
	o.insert("SYSLOG_IDENTIFIER".into(), id.into());
	o.insert("MESSAGE".into(), text.into());
	serde_json::to_string(&Value::Object(o)).unwrap() + "\n"
}

/// What example.service's journal has, as elamond/fixtures/0.8/ui-sources.toml's comments say.
pub fn example_lines() -> String {
	[
		(1_699_999_990_123_456, 6, "Listening on port 8080"),
		(1_699_999_992_000_000, 7, "Cache warmed with 120 entries"),
		(1_699_999_995_234_567, 4, "Slow request: GET /status took 2.1 s"),
		(1_699_999_999_345_678, 6, "Request served in 12 ms"),
	]
	.map(|(us, p, text)| json_line(us, p, Some("example.service"), None, "example", text))
	.concat()
}

/// What this user's journal has, as the fixture's comments say.
pub fn user_lines() -> String {
	[(1_699_999_970_000_000, "Synced 14 files"), (1_699_999_985_000_000, "Nothing to sync")]
		.map(|(us, text)| json_line(us, 6, Some("user@1000.service"), Some("example-sync.service"), "example-sync", text))
		.concat()
}

/// The 2000 lines of the system journal before 1699999995234567, the newest first, as `journalctl
/// -r` prints them: the fixture's three at warning or above among lines of `info`, the oldest at
/// 1699999612345678.
pub fn system_page() -> String {
	let mut lines: Vec<(u64, u8, Option<&str>, &str, &str)> = (0..1996)
		.map(|i| (1_699_999_995_000_000 - i * 190_000, 6, Some("example.service"), "example", "Request served"))
		.collect();
	lines.push((1_699_999_801_000_000, 4, Some("sshd.service"), "sshd", "Connection closed by 192.0.2.20 port 52100 [preauth]"));
	lines.push((1_699_999_870_500_000, 3, Some("example.service"), "example", "Can't write the cache: no space left on device"));
	lines.push((1_699_999_935_250_000, 2, None, "kernel", "Out of memory: Killed process 4242 (example)"));
	lines.push((1_699_999_612_345_678, 6, Some("example.service"), "example", "Request served"));
	lines.sort_by_key(|l| std::cmp::Reverse(l.0));
	lines.iter().map(|(us, p, unit, id, text)| json_line(*us, *p, *unit, None, id, text)).collect()
}

/// Where the fake is: a folder of the tests' process.
fn fake_dir() -> PathBuf {
	std::env::temp_dir().join(format!("elamond-journalctl-{}", std::process::id()))
}

/// The tests' journalctl, the same for all of them, which `ELAMON_JOURNALCTL` names: a script that
/// prints what the journal asked for has (by `-u`'s unit, else `user` or `system`), following it
/// with `-f`, else a page of it. Some units are ways it misbehaves; `args.service` prints its
/// arguments and its process id as lines. Its folder goes as the tests end.
pub fn fake() -> PathBuf {
	static PROGRAM: OnceLock<PathBuf> = OnceLock::new();
	PROGRAM
		.get_or_init(|| {
			let dir = fake_dir();
			std::fs::create_dir_all(&dir).unwrap();
			let mut page: Vec<String> = example_lines().lines().map(String::from).collect();
			page.reverse();
			let files = [("example.service.tail", example_lines()), ("user.tail", user_lines()), ("system.page", system_page()), ("example.service.page", page.join("\n") + "\n")];
			for (name, text) in files {
				std::fs::write(dir.join(name), text).unwrap();
			}
			let program = dir.join("journalctl");
			std::fs::write(&program, SCRIPT).unwrap();
			std::fs::set_permissions(&program, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
			// SAFETY: set once, to the one value every test reads, before the tests that run journalctl
			// make a `Journal`.
			unsafe { std::env::set_var("ELAMON_JOURNALCTL", &program) };
			// SAFETY: a function of no arguments, run as the tests' process exits.
			unsafe { libc::atexit(remove_fake) };
			program
		})
		.clone()
}

extern "C" fn remove_fake() {
	let _ = std::fs::remove_dir_all(fake_dir());
}

const SCRIPT: &str = r#"#!/bin/sh
here=$(dirname "$0")
journal=system
follow=
prev=
for a in "$@"; do
	if [ "$prev" = -u ]; then
		journal=$a
	elif [ "$a" = --user ]; then
		journal=user
	fi
	[ "$a" = -f ] && follow=1
	prev=$a
done
line() {
	printf '{"__REALTIME_TIMESTAMP":"%s","PRIORITY":"%s","SYSLOG_IDENTIFIER":"fake","MESSAGE":"%s"}\n' "$1" "$2" "$3"
}
if [ -n "$follow" ]; then
	case $journal in
	exits.service)
		echo "Hint: You are currently not seeing messages from other users and the system." >&2
		echo "Failed to follow the journal." >&2
		exit 1;;
	hang.service) exec sleep 600;;
	late.service)
		sleep 0.6
		cat "$here/example.service.tail";;
	args.service)
		line 1700000000000000 6 "$*"
		line 1700000000000001 6 "pid $$";;
	burst.service)
		line 1700000000000000 6 "before the burst"
		sleep 1
		i=1
		while [ $i -le 2000 ]; do
			line $((1700000000000000 + i)) 6 "line $i"
			i=$((i + 1))
		done;;
	*) cat "$here/$journal.tail" 2>/dev/null;;
	esac
	exec sleep 600
fi
case $journal in
slow.service) exec sleep 600;;
broken.service)
	echo "Hint: You are currently not seeing messages from other users and the system." >&2
	echo "Failed to open the journal." >&2
	exit 1;;
args.service) line 1700000000000000 0 "$*";;
*) cat "$here/$journal.page" 2>/dev/null || true;;
esac
"#;

/// Whether process `pid` has ended (a zombie its parent hasn't reaped yet counts).
fn gone(pid: i32) -> bool {
	match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
		Err(_) => true,
		Ok(s) => s.rsplit_once(')').and_then(|(_, r)| r.split_whitespace().next()) == Some("Z"),
	}
}

/// Whether process `pid` has ended, waiting up to 5 s, the runtime going on meanwhile.
pub async fn ended(pid: i32) -> bool {
	for _ in 0..50 {
		if gone(pid) {
			return true;
		}
		tokio::time::sleep(Duration::from_millis(100)).await;
	}
	false
}

/// Journals the fake follows, as if the system journal were readable.
fn journal() -> Journal {
	Journal { program: fake(), readable: || true, ..Journal::default() }
}

/// Waits up to 5 s for the buffer of `tail` to be as `done` says.
async fn until(tail: &Tail, done: impl Fn(&Buffer) -> bool) {
	for _ in 0..100 {
		if done(&tail.buffer()) {
			return;
		}
		tokio::time::sleep(Duration::from_millis(50)).await;
	}
	panic!("the tail never got there: {:?}", tail.buffer());
}

/// The texts of a state's lines.
pub fn texts(state: &Obj) -> Vec<String> {
	state["lines"].as_array().unwrap().iter().map(|l| l["text"].as_str().unwrap().to_string()).collect()
}

fn unit(name: &str) -> Key {
	Key::Unit { name: name.into(), user: false }
}

fn line(us: u64, priority: u8, text: &str) -> Line {
	parse(json_line(us, priority, None, None, "x", text).as_bytes()).unwrap()
}

#[test]
fn lines_read_as_journalctl_prints_them() {
	let l = parse(json_line(1_699_999_995_234_567, 4, Some("example.service"), None, "example", "Slow request").as_bytes()).unwrap();
	assert_eq!(
		serde_json::to_string(&l.json()).unwrap(),
		r#"{"c":1699999995234567,"t":1699999995234,"level":"warning","unit":"example.service","id":"example","text":"Slow request"}"#
	);
	// The user's unit before the system's, the command without an identifier, info without a
	// priority, a message that isn't UTF-8 as its bytes, a field given twice as its first.
	let l = parse(br#"{"__REALTIME_TIMESTAMP":"1","_SYSTEMD_UNIT":"user@1000.service","_SYSTEMD_USER_UNIT":"sync.service","_COMM":"sync","MESSAGE":[104,105,255]}"#).unwrap();
	assert_eq!((l.unit.as_deref(), l.id.as_deref(), l.priority, l.text.as_str()), (Some("sync.service"), Some("sync"), 6, "hi\u{fffd}"));
	let l = parse(br#"{"__REALTIME_TIMESTAMP":"1","PRIORITY":"9","MESSAGE":["first","second"],"SYSLOG_IDENTIFIER":""}"#).unwrap();
	assert_eq!((l.text.as_str(), l.priority, l.id), ("first", 6, None));
	// The kernel's: no unit.
	let l = parse(json_line(1, 2, None, None, "kernel", "Out of memory").as_bytes()).unwrap();
	assert_eq!((l.json().get("unit"), &l.json()["level"]), (None, &Value::from("critical")));
	// No time, or not JSON: no line.
	assert!(parse(br#"{"MESSAGE":"x"}"#).is_none() && parse(b"-- No entries --").is_none());
	let levels: Vec<&str> = (0..=7).map(level_of).collect();
	assert_eq!(levels, ["critical", "critical", "critical", "error", "warning", "notice", "info", "debug"]);
	assert_eq!(crate::settings::LOG_LEVELS.map(|l| priority_of(l).unwrap()), [7, 6, 5, 4, 3, 2]);
}

#[test]
fn a_message_loses_its_colours_and_control_characters_and_is_cut() {
	assert_eq!(clean("\u{1b}[1;31mred\u{1b}[0m\tand\u{7}\r\nnext\n"), "red and\nnext");
	let long = clean(&"é".repeat(5000));
	assert_eq!((long.chars().count(), long.ends_with('…')), (limit::LOG_TEXT_MAX, true));
	assert_eq!(clean(&"y".repeat(limit::LOG_TEXT_MAX)).chars().count(), limit::LOG_TEXT_MAX);
}

#[test]
fn lines_come_however_the_pipe_cuts_them_and_too_long_ones_are_skipped() {
	let mut s = Splitter::default();
	let mut got: Vec<String> = Vec::new();
	let long = "x".repeat(limit::JOURNAL_LINE_MAX + 1);
	for chunk in ["one\ntw", "o\n", &long[..10], &long[10..], "\nthree\n", "four"] {
		s.feed(chunk.as_bytes(), |l| got.push(String::from_utf8_lossy(l).into()));
	}
	assert_eq!(got, ["one", "two", "three"]);
	s.feed(b"\n", |l| got.push(String::from_utf8_lossy(l).into()));
	assert_eq!(got.last().map(String::as_str), Some("four"));
}

#[test]
fn a_filter_takes_a_level_and_a_search_whatever_the_case() {
	let l = parse(json_line(1, 4, Some("example.service"), None, "example", "Slow request").as_bytes()).unwrap();
	assert!(Filter::new(4, None).matches(&l) && !Filter::new(3, None).matches(&l));
	for search in ["SLOW", "Example.Service", "exam"] {
		assert!(Filter::new(7, Some(search)).matches(&l), "{search}");
	}
	assert!(!Filter::new(7, Some("fast")).matches(&l) && Filter::new(7, Some("")).matches(&l));
}

#[test]
fn a_stream_gets_the_newest_lines_then_what_comes_and_how_many_it_missed() {
	let info = Filter::new(6, None);
	let mut b = Buffer::default();
	let mut sent = None;
	// Nothing until the journal's last lines are in.
	b.start(None);
	b.push(line(1, 6, "first"));
	assert_eq!(b.state(&mut sent, &info, 200), None);
	assert_eq!(sent, None);
	b.ready();
	let first = b.state(&mut sent, &info, 200).unwrap();
	assert_eq!((texts(&first), &first["reset"]), (vec!["first".to_string()], &Value::Bool(true)));
	// Nothing new: no lines, no reset, no more.
	assert_eq!(Value::Object(b.state(&mut sent, &info, 200).unwrap()), serde_json::json!({ "lines": [] }));
	// A burst: the newest 50 come, the rest are counted, those gone from the buffer too.
	for i in 0..2000 {
		b.push(line(2 + i, 6, &format!("line {i}")));
	}
	assert_eq!(b.lines.len(), limit::JOURNAL_KEPT);
	let burst = b.state(&mut sent, &info, 200).unwrap();
	assert_eq!((texts(&burst).len(), texts(&burst).last().map(String::as_str)), (limit::LOG_NEW_LINES, Some("line 1999")));
	assert_eq!(burst["more"], 1000 + 950);
	assert!(burst.get("reset").is_none());
	// Filtered anew: the newest that match, as many as the client keeps.
	let state = b.state(&mut None, &info, 10).unwrap();
	assert_eq!((texts(&state).len(), &state["reset"]), (10, &Value::Bool(true)));
	assert_eq!(b.state(&mut None, &Filter::new(5, None), 200).unwrap()["lines"], serde_json::json!([]));
	// A journalctl started again: anew, without what the one before had; its note in every state.
	b.start(Some(ONLY_OWN.into()));
	assert_eq!(b.state(&mut sent, &info, 200), None);
	b.push(line(5000, 6, "again"));
	b.ready();
	let again = b.state(&mut sent, &info, 200).unwrap();
	assert_eq!((texts(&again), &again["reset"], &again["note"]), (vec!["again".to_string()], &Value::Bool(true), &Value::from(ONLY_OWN)));
	assert_eq!(b.state(&mut sent, &info, 200).unwrap()["note"], ONLY_OWN);
	b.ended("journalctl ended: gone".into());
	assert_eq!(Value::Object(b.state(&mut sent, &info, 200).unwrap()), serde_json::json!({ "lines": [], "note": "journalctl ended: gone" }));
}

#[test]
fn states_keep_within_their_bytes() {
	let mut b = Buffer::default();
	b.start(None);
	for i in 0..100 {
		b.push(line(i, 6, &"w".repeat(1500)));
	}
	b.ready();
	let mut sent = None;
	let reset = b.state(&mut sent, &Filter::new(7, None), 1000).unwrap();
	let size = serde_json::to_string(&reset["lines"]).unwrap().len();
	assert!(size <= limit::LOG_RESET_BYTES && size > limit::LOG_RESET_BYTES - 2000, "{size}");
	for i in 100..200 {
		b.push(line(i, 6, &"w".repeat(1500)));
	}
	let next = b.state(&mut sent, &Filter::new(7, None), 1000).unwrap();
	let n = texts(&next).len();
	assert!(serde_json::to_string(&next["lines"]).unwrap().len() <= limit::LOG_NEW_BYTES && n >= 9, "{n}");
	assert_eq!(next["more"], 100 - n as u64);
	// The newest that fit, and none older than one that doesn't, however small.
	let big = "€".repeat(limit::LOG_TEXT_MAX);
	for (i, text) in ["old", &big, &big, &big, "new"].into_iter().enumerate() {
		b.push(line(1000 + i as u64, 6, text));
	}
	let next = b.state(&mut sent, &Filter::new(7, None), 1000).unwrap();
	let sizes: Vec<usize> = texts(&next).iter().map(|t| t.chars().count()).collect();
	assert_eq!((sizes, &next["more"]), (vec![limit::LOG_TEXT_MAX, limit::LOG_TEXT_MAX, 3], &Value::from(2)));
}

#[test]
fn a_journal_is_chosen_by_its_scope_and_a_units_by_its_link() {
	let config = crate::settings::read_sources(&[crate::settings::Source::new(
		"a.toml",
		"[links.a]\nname = \"A\"\nurl = \"http://{host}:1\"\nunit = \"a.service\"\nscope = \"system\"\n\
		 [links.b]\nname = \"B\"\nurl = \"http://{host}:2\"\nunit = \"b.service\"\n",
	)])
	.unwrap();
	let w = |scope: &str, unit: Option<&str>| Widget { scope: Some(scope.into()), unit: unit.map(String::from), ..Default::default() };
	assert_eq!(Key::of(&Widget::default(), &config), Some(Key::System));
	assert_eq!(Key::of(&w("user", None), &config), Some(Key::User));
	assert_eq!(Key::of(&w("unit", Some("a.service")), &config), Some(unit("a.service")));
	assert_eq!(Key::of(&w("unit", Some("b.service")), &config), Some(Key::Unit { name: "b.service".into(), user: true }));
	assert_eq!(Key::of(&w("unit", Some("c.service")), &config), None);
	assert_eq!(Key::of(&w("unit", Some("")), &config), None);
	assert_eq!(Key::Unit { name: "b.service".into(), user: true }.args(), ["--user", "-u", "b.service"]);
	assert!(Key::System.of_system() && unit("a.service").of_system() && !Key::User.of_system());
}

/// The arguments the fake was run with, as it printed them, and its process id.
async fn args_and_pid(tail: &Tail) -> (String, i32) {
	until(tail, |b| b.lines.len() >= 2).await;
	let b = tail.buffer();
	let pid = b.lines[1].text.strip_prefix("pid ").unwrap().parse().unwrap();
	(b.lines[0].text.clone(), pid)
}

const FIELDS_ARG: &str = "--output-fields=MESSAGE,PRIORITY,_SYSTEMD_UNIT,_SYSTEMD_USER_UNIT,SYSLOG_IDENTIFIER,_COMM --all";

#[tokio::test]
async fn a_tail_follows_its_journal_while_held_and_kills_journalctl_as_it_is_let_go() {
	let journal = journal();
	for (key, args) in [
		(unit("args.service"), "-u args.service -f -o json -n 1000 --no-pager"),
		(Key::Unit { name: "args.service".into(), user: true }, "--user -u args.service -f -o json -n 1000 --no-pager"),
	] {
		let tail = journal.tail(&key);
		// The same journal, the same tail.
		assert!(Arc::ptr_eq(&tail, &journal.tail(&key)));
		let (got, pid) = args_and_pid(&tail).await;
		assert_eq!(got, format!("{args} {FIELDS_ARG}"));
		assert_eq!(journal.followed(), 1);
		drop(tail);
		assert!(ended(pid).await, "journalctl {pid} left running");
		assert_eq!(journal.followed(), 0);
	}
	// The unit's lines, ready once journalctl has printed them.
	let tail = journal.tail(&unit("example.service"));
	until(&tail, |b| b.ready).await;
	let state = tail.buffer().state(&mut None, &Filter::new(6, None), 200).unwrap();
	assert_eq!(texts(&state), ["Listening on port 8080", "Slow request: GET /status took 2.1 s", "Request served in 12 ms"]);
	assert!(state.get("note").is_none());
}

#[tokio::test]
async fn the_system_journal_unread_has_a_note_and_the_users_none() {
	let journal = Journal { readable: || false, ..journal() };
	let system = journal.tail(&unit("example.service"));
	let user = journal.tail(&Key::User);
	until(&system, |b| b.ready).await;
	until(&user, |b| b.ready).await;
	assert_eq!(system.buffer().state(&mut None, &Filter::new(7, None), 200).unwrap()["note"], ONLY_OWN);
	let mine = user.buffer().state(&mut None, &Filter::new(7, None), 200).unwrap();
	assert_eq!((texts(&mine), mine.get("note")), (vec!["Synced 14 files".to_string(), "Nothing to sync".to_string()], None));
	assert_eq!(user.buffer().lines[0].unit.as_deref(), Some("example-sync.service"));
}

/// A journalctl that says nothing: its logs have a state, empty, once it has run for a while.
#[tokio::test]
async fn a_journalctl_that_says_nothing_holds_up_nothing() {
	let journal = journal();
	let tail = journal.tail(&unit("hang.service"));
	let started = std::time::Instant::now();
	until(&tail, |b| b.ready).await;
	let took = started.elapsed();
	assert!(took >= timeout::JOURNAL_START - Duration::from_millis(100) && took < timeout::JOURNAL_START + Duration::from_secs(1), "{took:?}");
	assert_eq!(Value::Object(tail.buffer().state(&mut None, &Filter::new(6, None), 200).unwrap()), serde_json::json!({ "lines": [], "reset": true }));
}

/// A journalctl slow to start, as on a big journal not yet in memory: its logs' first state waits
/// for the lines it prints, rather than starting empty and having them all come as new.
#[tokio::test]
async fn a_journalctl_slow_to_start_is_waited_for() {
	let journal = journal();
	let tail = journal.tail(&unit("late.service"));
	until(&tail, |b| b.ready).await;
	let state = tail.buffer().state(&mut None, &Filter::new(6, None), 200).unwrap();
	assert_eq!(texts(&state), ["Listening on port 8080", "Slow request: GET /status took 2.1 s", "Request served in 12 ms"]);
	assert_eq!(state["reset"], true);
}

#[tokio::test]
async fn a_burst_is_kept_to_the_buffer_and_a_stream_told_how_much_it_missed() {
	let journal = journal();
	let tail = journal.tail(&unit("burst.service"));
	until(&tail, |b| b.ready).await;
	let mut sent = None;
	let first = tail.buffer().state(&mut sent, &Filter::new(6, None), 200).unwrap();
	assert_eq!(texts(&first), ["before the burst"]);
	until(&tail, |b| b.next == 2001).await;
	assert_eq!(tail.buffer().lines.len(), limit::JOURNAL_KEPT);
	let next = tail.buffer().state(&mut sent, &Filter::new(6, None), 200).unwrap();
	assert_eq!((texts(&next).len(), texts(&next).last().map(String::as_str)), (limit::LOG_NEW_LINES, Some("line 2000")));
	assert_eq!(next["more"], 2000 - limit::LOG_NEW_LINES as u64);
}

#[tokio::test]
async fn a_journalctl_that_ends_says_why_and_is_started_again_every_10_s() {
	let journal = journal();
	let tail = journal.tail(&unit("exits.service"));
	until(&tail, |b| b.ready && b.run == 1).await;
	let first = std::time::Instant::now();
	let state = tail.buffer().state(&mut None, &Filter::new(6, None), 200).unwrap();
	assert_eq!(Value::Object(state), serde_json::json!({ "lines": [], "reset": true, "note": "journalctl ended: Failed to follow the journal." }));
	for _ in 0..150 {
		if tail.buffer().run == 2 {
			break;
		}
		tokio::time::sleep(Duration::from_millis(100)).await;
	}
	let again = first.elapsed();
	assert_eq!(tail.buffer().run, 2);
	assert!(again >= cadence::JOURNAL_RESTART - Duration::from_millis(500) && again < cadence::JOURNAL_RESTART + Duration::from_secs(3), "{again:?}");
}

#[tokio::test]
async fn without_journalctl_a_log_says_so() {
	let journal = Journal { program: "/nonexistent/journalctl".into(), ..journal() };
	let tail = journal.tail(&Key::System);
	until(&tail, |b| b.ready).await;
	let state = tail.buffer().state(&mut None, &Filter::new(6, None), 200).unwrap();
	assert_eq!(Value::Object(state), serde_json::json!({ "lines": [], "reset": true, "note": NO_JOURNALCTL }));
}

#[test]
fn journalctl_goes_as_the_runtime_stops() {
	let journal = Arc::new(journal());
	let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
	let pid = rt.block_on(async {
		let tail = journal.tail(&unit("args.service"));
		let (_, pid) = args_and_pid(&tail).await;
		// Held by a task that never ends, as a stream's is.
		tokio::spawn(async move {
			let _held = tail;
			std::future::pending::<()>().await
		});
		pid
	});
	drop(rt);
	let ended = (0..50).any(|_| gone(pid) || {
		std::thread::sleep(Duration::from_millis(100));
		false
	});
	assert!(ended, "journalctl {pid} left running");
}

#[tokio::test]
async fn a_page_of_older_lines_goes_back_from_before() {
	let program = fake();
	// The fixture's: three of 2000 at warning or above, and where to go on from.
	let (lines, before) = api::page(&program, &Key::System, Some(1_699_999_995_234_567), &Filter::new(4, None)).await.unwrap();
	let times: Vec<u64> = lines.iter().map(|l| l.us).collect();
	assert_eq!((times, before), (vec![1_699_999_801_000_000, 1_699_999_870_500_000, 1_699_999_935_250_000], Some(1_699_999_612_345_678)));
	// 100 matches at most, `before` the oldest of them; a search.
	let (lines, before) = api::page(&program, &Key::System, None, &Filter::new(6, None)).await.unwrap();
	assert_eq!((lines.len(), before), (limit::LOG_PAGE, Some(1_699_999_995_000_000 - 99 * 190_000)));
	assert!(lines.windows(2).all(|w| w[0].us < w[1].us));
	let (lines, _) = api::page(&program, &Key::System, None, &Filter::new(7, Some("OUT OF MEMORY"))).await.unwrap();
	assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["Out of memory: Killed process 4242 (example)"]);
	// A journal that ends before 2000 lines: none to go on from.
	let (lines, before) = api::page(&program, &unit("example.service"), None, &Filter::new(6, None)).await.unwrap();
	assert_eq!((lines.len(), before), (3, None));
	// journalctl's arguments.
	let user = Key::Unit { name: "args.service".into(), user: true };
	let (lines, _) = api::page(&program, &user, Some(1_699_999_995_234_567), &Filter::new(4, None)).await.unwrap();
	assert_eq!(lines[0].text, format!("--user -u args.service -o json {FIELDS_ARG} --no-pager -r -n 2000 -p 0..4 --until=@1699999995.234566"));
	let (lines, _) = api::page(&program, &unit("args.service"), Some(1_000_001), &Filter::new(7, None)).await.unwrap();
	assert_eq!(lines[0].text, format!("-u args.service -o json {FIELDS_ARG} --no-pager -r -n 2000 -p 0..7 --until=@1.000000"));
	let (lines, _) = api::page(&program, &Key::User, None, &Filter::new(6, None)).await.unwrap();
	assert!(lines.is_empty());
	// One that fails says why, not its hint.
	assert_eq!(api::page(&program, &unit("broken.service"), None, &Filter::new(6, None)).await, Err("Failed to open the journal.".into()));
	assert_eq!(api::page(Path::new("/nonexistent/journalctl"), &Key::System, None, &Filter::new(6, None)).await, Err(NO_JOURNALCTL.into()));
}
