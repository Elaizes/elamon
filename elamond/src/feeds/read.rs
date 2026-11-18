//! One read of a source, within its `timeout`: a command run without a shell, a web service asked
//! through curl, or a file read. A command and curl run in a process group of their own, killed at
//! the timeout and once they end: a read leaves nothing running behind it.

use std::fs::File;
use std::io::{self, Read};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::task::JoinHandle;

use super::parse::{self, Parsed};
use super::Failure;
use crate::consts::limit;
use crate::control::{read_some, Group};
use crate::settings::{Feed, FeedKind};

/// What one read gave: its values by path, or why there are none; a command's exit code, when it
/// ended by itself; and whether values were left out.
#[derive(Debug)]
pub struct Outcome {
	pub result: Result<Vec<(String, crate::values::V)>, Failure>,
	pub exit: Option<i32>,
	pub cut: bool,
}

impl Outcome {
	fn failed(f: Failure) -> Outcome {
		Outcome { result: Err(f), exit: None, cut: false }
	}

	fn parsed(p: Parsed, exit: Option<i32>) -> Outcome {
		Outcome { result: Ok(p.values), exit, cut: p.cut }
	}
}

/// A file read in the background that hasn't ended (a file source's, or a token's), which the
/// source's next read waits for.
pub type Stuck = Option<JoinHandle<()>>;

/// Reads `feed` once.
pub async fn read(feed: &Feed, stuck: &mut Stuck) -> Outcome {
	let limit = Duration::from_secs(feed.timeout);
	match feed.kind {
		FeedKind::Command => command(feed, limit).await,
		FeedKind::Http => http(feed, limit, stuck).await,
		FeedKind::File => file(feed, stuck).await,
	}
}

/// What `job` gives, run on a thread of the blocking pool within the source's timeout: a file on a
/// mount that doesn't answer holds up no worker of the runtime. A job that doesn't end in time is
/// left there, and the source's next read waits for it.
pub(super) async fn blocking<T: Send + 'static>(feed: &Feed, stuck: &mut Stuck, job: impl FnOnce() -> T + Send + 'static) -> Result<T, Failure> {
	if let Some(before) = stuck.take() {
		let _ = before.await;
	}
	let (tx, rx) = tokio::sync::oneshot::channel();
	let running = tokio::task::spawn_blocking(move || {
		let _ = tx.send(job());
	});
	match tokio::time::timeout(Duration::from_secs(feed.timeout), rx).await {
		Ok(Ok(got)) => Ok(got),
		Ok(Err(_)) => Err(Failure::new("can't read")),
		Err(_) => {
			*stuck = Some(running);
			Err(timed_out(feed))
		}
	}
}

fn timed_out(feed: &Feed) -> Failure {
	Failure::new(format!("timed out after {} s", feed.timeout))
}

fn over() -> Failure {
	Failure::new(format!("output over {} KiB", limit::FEED_OUTPUT_MAX / 1024))
}

/// An error as the system says it, without its number.
pub(crate) fn said(e: &io::Error) -> String {
	let s = e.to_string();
	s.split(" (os error").next().unwrap_or(&s).to_string()
}

/// The first line of what was written to stderr, if any.
pub(crate) fn first_line(err: &[u8]) -> Option<String> {
	String::from_utf8_lossy(err).lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_string)
}

// ---- a process ------------------------------------------------------------------------------

/// What a process gave: its stdout, the start of its stderr, and how it ended: `None` when it was
/// killed at the timeout or as it wrote too much.
pub(crate) struct Ran {
	pub(crate) out: Vec<u8>,
	pub(crate) err: Vec<u8>,
	pub(crate) status: Option<ExitStatus>,
	pub(crate) over: bool,
}

/// Runs `program` with `args` in a process group of its own, `input` written to its stdin (then
/// closed), or nothing there. Its stdout is read up to `out_max` bytes, its stderr up to
/// `limit::FEED_STDERR_MAX`. At `limit`, or as its stdout goes over, the group is killed; once the
/// process ends, the group is killed too, what it wrote still read from the pipes.
pub(crate) async fn run(program: &str, args: &[String], input: Option<&[u8]>, limit: Duration, out_max: usize) -> io::Result<Ran> {
	let mut child = tokio::process::Command::new(program)
		.args(args)
		.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.process_group(0)
		.kill_on_drop(true)
		.spawn()?;
	// Killed as this ends, whatever ends it: the timeout, too much output, the source stopped.
	let mut group = Group(child.id().map(|id| id as i32));
	if let (Some(bytes), Some(mut stdin)) = (input, child.stdin.take()) {
		// A line, into an empty pipe: written at once, whatever the process does.
		let _ = stdin.write_all(bytes).await;
	}
	let mut out = child.stdout.take();
	let mut err = child.stderr.take();
	let (mut out_buf, mut err_buf) = ([0u8; 8192], [0u8; 4096]);
	let mut ran = Ran { out: Vec::new(), err: Vec::new(), status: None, over: false };
	let until = tokio::time::Instant::now() + limit;
	while ran.status.is_none() || out.is_some() || err.is_some() {
		tokio::select! {
			_ = tokio::time::sleep_until(until) => break,
			n = read_some(&mut out, &mut out_buf) => {
				ran.out.extend_from_slice(&out_buf[..n]);
				if ran.out.len() > out_max {
					ran.over = true;
					break;
				}
			}
			n = read_some(&mut err, &mut err_buf) => {
				let room = limit::FEED_STDERR_MAX.saturating_sub(ran.err.len()).min(n);
				ran.err.extend_from_slice(&err_buf[..room]);
			}
			s = child.wait(), if ran.status.is_none() => {
				ran.status = Some(s?);
				// What it started goes with it, once; what it wrote is in the pipes still.
				drop(std::mem::replace(&mut group, Group(None)));
			}
		}
	}
	Ok(ran)
}

// ---- command --------------------------------------------------------------------------------

/// A command's read: good when it exits with 0 and what it printed is of its format, or with
/// another code when what it printed gives a value.
async fn command(feed: &Feed, limit: Duration) -> Outcome {
	let Some((program, args)) = feed.command.split_first() else { return Outcome::failed(Failure::new("no command")) };
	let ran = match run(program, args, None, limit, limit::FEED_OUTPUT_MAX).await {
		Ok(r) => r,
		Err(e) => return Outcome::failed(Failure::with("can't start", said(&e))),
	};
	if ran.over {
		return Outcome::failed(over());
	}
	let Some(status) = ran.status else { return Outcome::failed(timed_out(feed)) };
	let Some(code) = status.code() else {
		use std::os::unix::process::ExitStatusExt;
		return Outcome::failed(Failure::new(format!("ended by signal {}", status.signal().unwrap_or(0))));
	};
	let parsed = parse::parse(feed.format, &ran.out);
	match parsed {
		Ok(p) if code == 0 || !p.values.is_empty() => Outcome::parsed(p, Some(code)),
		Err(f) if code == 0 => Outcome { result: Err(f), exit: Some(0), cut: false },
		_ => {
			let why = format!("exited with {code}");
			let f = match first_line(&ran.err) {
				Some(line) => Failure::with(why, line),
				None => Failure::new(why),
			};
			Outcome { result: Err(f), exit: Some(code), cut: false }
		}
	}
}

// ---- http -----------------------------------------------------------------------------------

/// A web service's token, read from its `token_file` at each request and dropped after it: it has
/// no `Debug`, `Clone` or `Serialize`, so it can't be printed, kept or sent anywhere by mistake. It
/// goes to curl on its stdin alone, in the header line.
pub struct Token(String);

impl Token {
	/// The first line of the file at `path`, trimmed: a regular file only its owner may read.
	pub fn read(path: &Path) -> Result<Token, Failure> {
		Token::read_as(path, "token_file")
	}

	/// The same of the file the key `key` names: a topic's is kept as a token is.
	pub fn read_as(path: &Path, key: &str) -> Result<Token, Failure> {
		let cant = |e: &io::Error| Failure::with(format!("can't read {key}"), said(e));
		let file = open_regular(path).map_err(|e| cant(&e))?.ok_or_else(|| Failure::new(format!("{key} isn't a regular file")))?;
		let mode = file.metadata().map_err(|e| cant(&e))?.permissions().mode();
		if mode & 0o077 != 0 {
			return Err(Failure::new(format!("{key} may be read by others: chmod 600 it")));
		}
		let mut text = String::new();
		file.take(limit::TOKEN_FILE_MAX).read_to_string(&mut text).map_err(|e| cant(&e))?;
		let first = text.lines().next().unwrap_or("").trim();
		if first.is_empty() {
			return Err(Failure::new(format!("{key} is empty")));
		}
		Ok(Token(first.to_string()))
	}

	/// The secret itself, for the one place it is written to: what curl reads on its stdin.
	pub(crate) fn secret(&self) -> &str {
		&self.0
	}

	/// The line curl reads its header from: `Authorization: Bearer <token>`, or `<header>: <token>`.
	fn header_line(&self, header: Option<&str>) -> String {
		match header {
			Some(h) => format!("{h}: {}\n", self.0),
			None => format!("Authorization: Bearer {}\n", self.0),
		}
	}

	/// Whether `text` holds the token.
	pub(crate) fn in_text(&self, text: &str) -> bool {
		text.contains(&self.0)
	}
}

/// curl's arguments for `feed`: no config file of its own (`-q`, first), http and https only, no
/// redirect followed, no proxy, within the timeout and the size; the header, when there is one,
/// read from stdin; the status code after the body.
pub(super) fn curl_args(feed: &Feed, header: bool) -> Vec<String> {
	let mut a: Vec<String> = ["-q", "--silent", "--show-error", "--globoff", "--proto", "=http,https", "--max-redirs", "0", "--noproxy", "*"].map(String::from).to_vec();
	a.extend(["--max-time".into(), feed.timeout.to_string(), "--max-filesize".into(), limit::FEED_OUTPUT_MAX.to_string()]);
	if header {
		a.extend(["--header".into(), "@-".into()]);
	}
	a.extend(["--write-out".into(), "\\n%{http_code}".into(), "--url".into(), feed.url.clone().unwrap_or_default()]);
	a
}

/// A web service's read: good when it answers 2xx with a body of its format.
async fn http(feed: &Feed, limit: Duration, stuck: &mut Stuck) -> Outcome {
	let token = match feed.token_file.clone() {
		Some(path) => match blocking(feed, stuck, move || Token::read(&path)).await {
			Ok(Ok(t)) => Some(t),
			Ok(Err(f)) | Err(f) => return Outcome::failed(f),
		},
		None => None,
	};
	let line = token.as_ref().map(|t| t.header_line(feed.header.as_deref()));
	// The body, a line break and the code.
	let ran = run("curl", &curl_args(feed, line.is_some()), line.as_deref().map(str::as_bytes), limit, limit::FEED_OUTPUT_MAX + 4).await;
	drop(line);
	let ran = match ran {
		Ok(r) => r,
		Err(e) => return Outcome::failed(Failure::with("can't run curl", said(&e))),
	};
	if ran.over {
		return Outcome::failed(over());
	}
	let Some(status) = ran.status else { return Outcome::failed(timed_out(feed)) };
	match status.code() {
		Some(0) => {}
		Some(28) => return Outcome::failed(timed_out(feed)),
		Some(63) => return Outcome::failed(over()),
		_ => {
			// curl's own words, without its "curl: (7) "; never the token, which it doesn't print.
			let why = first_line(&ran.err).map(|l| l.split_once(") ").map_or(l.clone(), |(_, w)| w.to_string()));
			let why = why.filter(|w| !token.as_ref().is_some_and(|t| t.in_text(w)));
			return Outcome::failed(match why {
				Some(w) => Failure::with("couldn't connect", w),
				None => Failure::new("couldn't connect"),
			});
		}
	}
	drop(token);
	let (body, code) = match ran.out.iter().rposition(|b| *b == b'\n') {
		Some(at) => (&ran.out[..at], String::from_utf8_lossy(&ran.out[at + 1..]).trim().to_string()),
		None => (&ran.out[..0], String::new()),
	};
	if body.len() > limit::FEED_OUTPUT_MAX {
		return Outcome::failed(over());
	}
	match code.parse::<u16>() {
		Ok(200..=299) => match parse::parse(feed.format, body) {
			Ok(p) => Outcome::parsed(p, None),
			Err(f) => Outcome::failed(f),
		},
		Ok(c @ 300..=399) => Outcome::failed(Failure::new(format!("redirect to elsewhere ({c}), not followed"))),
		Ok(c) => Outcome::failed(Failure::new(format!("HTTP {c}"))),
		Err(_) => Outcome::failed(Failure::new("couldn't connect")),
	}
}

// ---- file -----------------------------------------------------------------------------------

/// The file at `path`, opened without waiting (a FIFO with no writer would wait for one); `None`
/// when it isn't a regular file.
fn open_regular(path: &Path) -> io::Result<Option<File>> {
	let file = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(path)?;
	Ok(file.metadata()?.file_type().is_file().then_some(file))
}

/// What the regular file at `path` holds, up to one byte more than `max`; `None` when it isn't one.
fn read_regular(path: &Path, max: usize) -> io::Result<Option<Vec<u8>>> {
	let Some(file) = open_regular(path)? else { return Ok(None) };
	let mut buf = Vec::new();
	file.take(max as u64 + 1).read_to_end(&mut buf)?;
	Ok(Some(buf))
}

/// A file's read, on a thread of the blocking pool.
async fn file(feed: &Feed, stuck: &mut Stuck) -> Outcome {
	let path = feed.path.clone().unwrap_or_default();
	let bytes = match blocking(feed, stuck, move || read_regular(&path, limit::FEED_OUTPUT_MAX)).await {
		Ok(Ok(Some(b))) => b,
		Ok(Ok(None)) => return Outcome::failed(Failure::new("not a regular file")),
		Ok(Err(e)) => return Outcome::failed(Failure::with("can't read", said(&e))),
		Err(f) => return Outcome::failed(f),
	};
	if bytes.len() > limit::FEED_OUTPUT_MAX {
		return Outcome::failed(over());
	}
	match parse::parse(feed.format, &bytes) {
		Ok(p) => Outcome::parsed(p, None),
		Err(f) => Outcome::failed(f),
	}
}
