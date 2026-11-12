//! `elamond update`: replaces this elamond with the newest release on GitHub, when there is a newer
//! one, and starts the service again on it. Only when run: nothing is looked up in the background.
//!
//! What it downloads, and how:
//! - GitHub's list of releases (pre-releases too, drafts never), asked anonymously: User-Agent
//!   `elamond/VERSION` and the Accept GitHub asks for, nothing else. The release is the newest X.Y.Z
//!   one by version, newer than this binary, with elamond for its architecture.
//! - that release's archive and the archive's SHA-256, each into a folder of a random name only this
//!   user can enter, made afresh (mkdir never takes a folder or a link someone put there first).
//!
//! Everything goes through curl, or wget where there is no curl, which the computer keeps up to date
//! with its certificates and proxies: over HTTPS only (TLS 1.2 or newer; curl's redirects only to
//! HTTPS, which wget can't hold its redirects to, so curl is taken first; neither reads a config
//! file of its own, ~/.curlrc or ~/.wgetrc, which could turn checks off), within a time, and never
//! more than a set size of each, cut off past it. The archive and its checksum come only from the
//! project's own release downloads on GitHub.
//!
//! What is checked before anything is replaced: the checksum file holds one SHA-256, of a file of
//! this archive's name and no other; the archive matches it; only `elamond-V-ARCH-linux/elamond`
//! comes out of it, and that is a file of its own (not a link, not a hard link), tar not allowed to
//! write more than a set size; copied beside this binary under a new name (made, never opened
//! through a link), it says `elamond V`. Then it is renamed over this binary, which is atomic: the
//! old one or the new one is there, never half of either, and the service that runs the old one
//! keeps it until it starts again.
//!
//! What that stands against, and what not: the checksum comes from the same release, so it catches a
//! download damaged or cut short, not a release forged by someone who can publish on the repository
//! or a GitHub that lies. Against those, only HTTPS to GitHub and the account's security stand;
//! signing releases (minisign) is open for 1.0. Only a newer version is ever installed. A binary in
//! the Nix store, one in a folder this user can't write, or one that is no longer the file that
//! runs, is left alone, with what to do instead; so is root's.
//!
//! For tests, ELAMON_RELEASES_URL names another list in GitHub's shape; plain http is taken from it,
//! and from the files it names, only on this computer (127.0.0.1, ::1, localhost).

use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use data_encoding::HEXLOWER;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::setup::{self, InitSystem};
use crate::store;

/// GitHub's list of the repository's releases, newest made first; 100 reach far enough back.
const RELEASES: &str = "https://api.github.com/repos/Elaizes/elamon/releases?per_page=100";
/// Where the release files are: an archive or a checksum elsewhere in the list is not taken.
const DOWNLOADS: &str = "https://github.com/Elaizes/elamon/releases/download/";
const ACCEPT: &str = "application/vnd.github+json";

/// The most each download may be: the list of releases, an archive and its checksum; and what tar
/// may write unpacking the archive.
const LIST_MAX: u64 = 8 << 20;
const ARCHIVE_MAX: u64 = 128 << 20;
const SUM_MAX: u64 = 64 << 10;
const BINARY_MAX: u64 = 256 << 20;
/// How long a download may take, in seconds (curl; wget gives up on a read that waits 30 s).
const DOWNLOAD_SECS: u64 = 900;
/// How long the new binary gets to say its version.
const VERSION_WAIT: Duration = Duration::from_secs(10);

const USAGE: &str = "usage: elamond update [--check] [--yes]";
const HELP: &str = "  --check   only tell whether there is a newer release, and which
  --yes     install it without asking";

const NIX: &str = "this elamond is from the Nix store, and Nix updates it: with the NixOS or Home Manager module, \
	nix flake update elamon, then rebuild; from a Nix profile, nix profile upgrade --all (guide, section 8)";

pub fn run(args: &[String]) -> Result<(), String> {
	let (mut check, mut yes) = (false, false);
	for a in args {
		match a.as_str() {
			"--check" => check = true,
			"--yes" => yes = true,
			"-h" | "--help" => {
				println!("{USAGE}\n\n{HELP}");
				return Ok(());
			}
			other => return Err(format!("unknown option \"{other}\"\n{USAGE}")),
		}
	}
	// SAFETY: geteuid only reads the process's user id.
	if !check && unsafe { libc::geteuid() } == 0 {
		return Err("run elamond update as the user the service runs as, not root: it replaces that user's elamond \
			 and starts their service again"
			.into());
	}
	let exe = setup::this_exe()?;
	let target = PathBuf::from(&exe);
	let (list, loopback) = match std::env::var("ELAMON_RELEASES_URL") {
		Ok(url) if !url.is_empty() => {
			// For trying it out against releases of one's own: said, so it never goes unnoticed.
			eprintln!("elamond update: the releases are read from ELAMON_RELEASES_URL, {}, not GitHub", plain(&url));
			(url, true)
		}
		_ => (RELEASES.to_string(), false),
	};
	if !allowed(&list, loopback) {
		return Err(format!("won't read {list}: only https is taken (plain http only from this computer)"));
	}
	let fetcher = Fetcher::find().ok_or("elamond update downloads with curl or wget, and neither is installed")?;
	let releases = read_list(fetcher, &list)?;
	let this = this_version();
	let arch = std::env::consts::ARCH;
	let Some(found) = newest(&releases, this, arch, loopback) else {
		println!("elamond {} is up to date: there's no newer release for {arch}.", env!("CARGO_PKG_VERSION"));
		return Ok(());
	};
	println!("elamond {} is out; this is {}.", found.version, env!("CARGO_PKG_VERSION"));
	if let Some(page) = &found.page {
		println!("  Release notes: {page}");
	}
	if (found.version.major, found.version.minor) != (this.0.major, this.0.minor) {
		println!(
			"  ! {}.{} is a new minor version: update the app with it, as an app and a service of different minor \
			 versions may not understand each other.",
			found.version.major, found.version.minor
		);
	}
	let place = replaceable(&target);
	if check {
		match place {
			Ok(()) => println!("  elamond update installs it."),
			Err(why) => println!("  But {why}"),
		}
		return Ok(());
	}
	place?;
	still_running(&target)?;
	if !yes && !ask(&format!("Install it over {}?", setup::short(&target)))? {
		println!("Nothing changed.");
		return Ok(());
	}
	let scratch = Scratch::new()?;
	let new = download(fetcher, &found, loopback, scratch.path())?;
	put_in_place(&new, &target, found.version)?;
	drop(scratch);
	done(&format!("installed elamond {} at {}", found.version, setup::short(&target)));
	println!("{}", restart(&setup::Systemd, &exe));
	Ok(())
}

fn done(what: &str) {
	println!("  OK  {what}");
}

/// Asks, and takes y or yes for yes; anything else, the end of the input too, is no.
fn ask(question: &str) -> Result<bool, String> {
	print!("{question} [y/N] ");
	let _ = io::stdout().flush();
	let mut line = String::new();
	match io::stdin().read_line(&mut line) {
		Ok(0) => {
			println!("\n  (no answer; elamond update --yes installs without asking)");
			Ok(false)
		}
		Ok(_) => Ok(matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")),
		Err(e) => Err(format!("can't read the answer: {e}")),
	}
}

/// Text from elsewhere (a server, a program), as it may be shown: nothing a terminal would act on.
fn plain(text: &str) -> String {
	text.chars().filter(|c| !store::unseen(*c)).collect()
}

// ---- versions and the list of releases ----------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Version {
	major: u64,
	minor: u64,
	patch: u64,
}

impl Version {
	/// X.Y.Z, with a v before it or not, and nothing else: a release candidate's `-rc.1` is no
	/// release to update to.
	fn parse(s: &str) -> Option<Version> {
		let s = s.strip_prefix('v').unwrap_or(s);
		let mut parts = s.split('.');
		let mut number = || parts.next().filter(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))?.parse().ok();
		let v = Version { major: number()?, minor: number()?, patch: number()? };
		parts.next().is_none().then_some(v)
	}
}

impl std::fmt::Display for Version {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
	}
}

/// This binary's version, and whether it is a pre-release (0.7.0-rc.1), which its release is newer
/// than.
fn this_version() -> (Version, bool) {
	version_of(env!("CARGO_PKG_VERSION"))
}

fn version_of(s: &str) -> (Version, bool) {
	let s = s.split('+').next().unwrap_or(s);
	let (base, pre) = match s.split_once('-') {
		Some((base, _)) => (base, true),
		None => (s, false),
	};
	(Version::parse(base).unwrap_or(Version { major: 0, minor: 0, patch: 0 }), pre)
}

/// A release as GitHub lists it, the fields read here only; anything missing or null reads as
/// nothing, a missing `draft` as a draft.
#[derive(Debug, Deserialize)]
struct Release {
	#[serde(default)]
	tag_name: Option<String>,
	#[serde(default = "yes")]
	draft: bool,
	#[serde(default)]
	html_url: Option<String>,
	#[serde(default)]
	assets: Vec<Asset>,
}

fn yes() -> bool {
	true
}

#[derive(Debug, Deserialize)]
struct Asset {
	#[serde(default)]
	name: Option<String>,
	#[serde(default)]
	browser_download_url: Option<String>,
}

/// The release to install: its version, its archive for this architecture with the archive's
/// checksum, and its page.
#[derive(Debug, PartialEq)]
struct Found {
	version: Version,
	/// `elamond-V-ARCH-linux.tar.gz`.
	archive: String,
	archive_url: String,
	sum_url: String,
	page: Option<String>,
}

impl Found {
	/// The folder in the archive: `elamond-V-ARCH-linux`.
	fn folder(&self) -> &str {
		self.archive.strip_suffix(".tar.gz").unwrap_or(&self.archive)
	}
}

fn read_list(fetcher: Fetcher, url: &str) -> Result<Vec<Release>, String> {
	let mut body = Vec::new();
	fetch(fetcher, url, Some(ACCEPT), LIST_MAX, &mut body).map_err(|e| {
		// GitHub answers 403 or 429 past 60 questions an hour from one address without signing in.
		let limited = [" 403", " 429"].iter().any(|code| e.contains(code));
		format!("can't read the list of releases: {e}{}", if limited { "\n  GitHub may be limiting questions from this address; try again in an hour" } else { "" })
	})?;
	parse_list(&body)
}

fn parse_list(body: &[u8]) -> Result<Vec<Release>, String> {
	serde_json::from_slice(body).map_err(|e| format!("the list of releases isn't what GitHub sends: {e}"))
}

/// The newest release newer than `this` with both elamond's archive for `arch` and its checksum: by
/// version, not by where the list puts it (a fix for an older line can come out after a newer one).
fn newest(releases: &[Release], this: (Version, bool), arch: &str, loopback: bool) -> Option<Found> {
	releases
		.iter()
		.filter(|r| !r.draft)
		.filter_map(|r| {
			let version = Version::parse(r.tag_name.as_deref()?)?;
			if !(version > this.0 || (version == this.0 && this.1)) {
				return None;
			}
			let archive = format!("elamond-{version}-{arch}-linux.tar.gz");
			// The asset of that name, whose address ends in that name too.
			let url = |name: &str| {
				r.assets
					.iter()
					.filter(|a| a.name.as_deref() == Some(name))
					.find_map(|a| a.browser_download_url.clone().filter(|u| u.strip_suffix(name).is_some_and(|rest| rest.ends_with('/'))))
					.filter(|u| loopback || u.starts_with(DOWNLOADS))
			};
			let archive_url = url(&archive)?;
			let sum_url = url(&format!("{archive}.sha256"))?;
			let page = r.html_url.clone().filter(|p| allowed(p, loopback));
			Some(Found { version, archive, archive_url, sum_url, page })
		})
		.max_by_key(|f| f.version)
}

/// Whether `url` may be downloaded from: https anywhere; plain http only from this computer, and only
/// when testing (`loopback`), with nothing before the host (a user name) and no port but digits.
fn allowed(url: &str, loopback: bool) -> bool {
	if url.chars().any(|c| c.is_whitespace() || store::unseen(c)) {
		return false;
	}
	if let Some(rest) = url.strip_prefix("https://") {
		return !rest.is_empty();
	}
	let Some(rest) = url.strip_prefix("http://").filter(|_| loopback) else { return false };
	let host = rest.split('/').next().unwrap_or_default();
	["127.0.0.1", "localhost", "[::1]"].iter().any(|name| match host.strip_prefix(name) {
		Some("") => true,
		Some(port) => port.strip_prefix(':').is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())),
		None => false,
	})
}

// ---- downloading ---------------------------------------------------------------------------

/// What downloads: curl, or wget where there is no curl.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Fetcher {
	Curl,
	Wget,
}

impl Fetcher {
	fn find() -> Option<Fetcher> {
		[Fetcher::Curl, Fetcher::Wget].into_iter().find(|f| on_path(f.program()))
	}

	fn program(self) -> &'static str {
		match self {
			Fetcher::Curl => "curl",
			Fetcher::Wget => "wget",
		}
	}

	/// The command that writes what `url` holds to its standard output: over https only (plain http
	/// only where `allowed` took it), at most `max` bytes as far as it can tell, sending only the user
	/// agent and what is accepted.
	fn command(self, url: &str, accept: Option<&str>, max: u64) -> Command {
		let https = url.starts_with("https://");
		let agent = format!("elamond/{}", env!("CARGO_PKG_VERSION"));
		let accept = format!("Accept: {}", accept.unwrap_or("*/*"));
		let mut c = Command::new(self.program());
		match self {
			Fetcher::Curl => {
				// Plain http only to a test server on this computer, which is followed nowhere else.
				let (proto, redirects) = if https { ("=https", "5") } else { ("=http", "0") };
				// -q first: no ~/.curlrc, which could turn checks off or add headers.
				c.args(["-q", "--fail", "--silent", "--show-error", "--location", "--globoff", "--proto", proto, "--proto-redir", proto])
					.args(["--tlsv1.2", "--max-redirs", redirects,"--connect-timeout", "20", "--max-time", &DOWNLOAD_SECS.to_string()])
					.args(["--max-filesize", &max.to_string(), "--user-agent", &agent, "--header", &accept, "--output", "-", url]);
			}
			Fetcher::Wget => {
				// --no-config first: no /etc/wgetrc or ~/.wgetrc, which could turn the certificate check off.
				let redirects = if https { "--max-redirect=5" } else { "--max-redirect=0" };
				c.args(["--no-config", "--quiet", "--output-document=-", "--timeout=30", "--tries=3", redirects]);
				if https {
					c.arg("--https-only");
				}
				c.arg(format!("--user-agent={agent}")).arg(format!("--header={accept}")).arg(url);
			}
		}
		c
	}
}

fn on_path(name: &str) -> bool {
	which(name).is_some()
}

/// Where `name` is on PATH, as a shell would find it.
fn which(name: &str) -> Option<PathBuf> {
	let path = std::env::var_os("PATH")?;
	std::env::split_paths(&path)
		.map(|dir| dir.join(name))
		.find(|p| fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
}

/// What `url` holds, through `fetcher`, written to `out`: all of it and at most `max` bytes, or an
/// error. Its size.
fn fetch(fetcher: Fetcher, url: &str, accept: Option<&str>, max: u64, out: &mut dyn Write) -> Result<u64, String> {
	let mut child = fetcher
		.command(url, accept, max)
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.map_err(|e| format!("{}: {e}", fetcher.program()))?;
	// Read beside, so a fetcher that says much can't stall on a full pipe; the start is kept.
	let mut stderr = child.stderr.take().expect("piped");
	let said = std::thread::spawn(move || {
		let (mut kept, mut buf) = (Vec::new(), [0u8; 4096]);
		while let Ok(n) = stderr.read(&mut buf) {
			if n == 0 {
				break;
			}
			if kept.len() < 4096 {
				kept.extend_from_slice(&buf[..n]);
			}
		}
		String::from_utf8_lossy(&kept).into_owned()
	});
	let mut stdout = child.stdout.take().expect("piped");
	let mut buf = vec![0u8; 64 * 1024];
	let mut total = 0u64;
	let read = loop {
		match stdout.read(&mut buf) {
			Ok(0) => break Ok(()),
			Ok(n) => {
				total += n as u64;
				if total > max {
					break Err(format!("{url} is larger than it can be (over {max} bytes)"));
				}
				if let Err(e) = out.write_all(&buf[..n]) {
					break Err(format!("can't keep {url}: {e}"));
				}
			}
			Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
			Err(e) => break Err(format!("{url}: {e}")),
		}
	};
	if read.is_err() {
		let _ = child.kill();
	}
	drop(stdout);
	let status = child.wait().map_err(|e| format!("{}: {e}", fetcher.program()))?;
	let said = said.join().unwrap_or_default();
	read?;
	// curl stops by itself at --max-filesize.
	if fetcher == Fetcher::Curl && status.code() == Some(63) {
		return Err(format!("{url} is larger than it can be (over {max} bytes)"));
	}
	if !status.success() {
		let why = said.lines().map(str::trim).rfind(|l| !l.is_empty()).map(plain).unwrap_or_else(|| format!("{} {status}", fetcher.program()));
		return Err(format!("{url}: {why}"));
	}
	Ok(total)
}

/// What is written, and its SHA-256 as it goes.
struct Hashed<W> {
	inner: W,
	hash: Sha256,
}

impl<W: Write> Write for Hashed<W> {
	fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
		let n = self.inner.write(buf)?;
		self.hash.update(&buf[..n]);
		Ok(n)
	}

	fn flush(&mut self) -> io::Result<()> {
		self.inner.flush()
	}
}

/// A folder of a random name only this user can enter, removed with all in it when dropped.
struct Scratch(PathBuf);

impl Scratch {
	fn new() -> Result<Scratch, String> {
		Scratch::under(&std::env::temp_dir())
	}

	fn under(dir: &Path) -> Result<Scratch, String> {
		let path = dir.join(format!("elamond-update-{}", HEXLOWER.encode(&store::random_bytes::<8>())));
		// mkdir makes it or fails: never a folder or a link someone put there first.
		fs::DirBuilder::new().mode(0o700).create(&path).map_err(|e| format!("can't make {}: {e}", path.display()))?;
		Ok(Scratch(path))
	}

	fn path(&self) -> &Path {
		&self.0
	}
}

impl Drop for Scratch {
	fn drop(&mut self) {
		let _ = fs::remove_dir_all(&self.0);
	}
}

/// The release's elamond, downloaded into `scratch`, checked against its checksum and unpacked; its
/// version is checked where it goes.
fn download(fetcher: Fetcher, found: &Found, loopback: bool, scratch: &Path) -> Result<PathBuf, String> {
	for url in [&found.archive_url, &found.sum_url] {
		if !allowed(url, loopback) {
			return Err(format!("won't download {}: only https is taken", plain(url)));
		}
	}
	let mut sum = Vec::new();
	fetch(fetcher, &found.sum_url, None, SUM_MAX, &mut sum).map_err(|e| format!("can't download {e}"))?;
	let want = expected_sum(&sum, &found.archive)?;
	let path = scratch.join(&found.archive);
	let file = fs::OpenOptions::new()
		.write(true)
		.create_new(true)
		.mode(0o600)
		.open(&path)
		.map_err(|e| format!("can't write {}: {e}", path.display()))?;
	let mut hashed = Hashed { inner: file, hash: Sha256::new() };
	let size = fetch(fetcher, &found.archive_url, None, ARCHIVE_MAX, &mut hashed).map_err(|e| format!("can't download {e}"))?;
	if HEXLOWER.encode(&hashed.hash.finalize()) != want {
		return Err(format!("{} doesn't match its SHA-256: damaged on the way? Nothing was changed; try again", found.archive));
	}
	done(&format!("downloaded {} ({:.1} MB); its SHA-256 matches", found.archive, size as f64 / 1e6));
	unpack(&path, &scratch.join("x"), &format!("{}/elamond", found.folder()))
}

/// The SHA-256 a checksum file gives for `name`: it must hold that one line, in sha256sum's form,
/// naming that file and no other (`sha256sum -c` would check whatever file it names).
fn expected_sum(text: &[u8], name: &str) -> Result<String, String> {
	let what = format!("{name}.sha256");
	let text = std::str::from_utf8(text).map_err(|_| format!("{what} isn't text"))?;
	let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
	let [line] = lines.as_slice() else {
		return Err(format!("{what} should hold one checksum, and holds {} lines", lines.len()));
	};
	let mut words = line.split_whitespace();
	let (Some(sum), Some(file), None) = (words.next(), words.next(), words.next()) else {
		return Err(format!("{what} isn't in sha256sum's form"));
	};
	let file = file.strip_prefix('*').unwrap_or(file);
	if file != name {
		return Err(format!("{what} is the checksum of another file ({:?})", plain(file)));
	}
	if sum.len() != 64 || !sum.bytes().all(|b| b.is_ascii_hexdigit()) {
		return Err(format!("{what} holds no SHA-256"));
	}
	Ok(sum.to_ascii_lowercase())
}

/// `member` of the archive, and nothing else, unpacked into the new folder `into`: a file of its own
/// (not a link, not a hard link), tar not allowed to write more than BINARY_MAX.
fn unpack(archive: &Path, into: &Path, member: &str) -> Result<PathBuf, String> {
	let name = archive.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
	fs::DirBuilder::new().mode(0o700).create(into).map_err(|e| format!("can't make {}: {e}", into.display()))?;
	let mut tar = Command::new("tar");
	tar.arg("-xzf").arg(archive).arg("-C").arg(into).arg(member).stdin(Stdio::null()).stdout(Stdio::null());
	// SAFETY: between fork and exec, only setrlimit, which is async-signal-safe.
	unsafe {
		tar.pre_exec(|| {
			let limit = libc::rlimit { rlim_cur: BINARY_MAX as libc::rlim_t, rlim_max: BINARY_MAX as libc::rlim_t };
			if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0 {
				return Err(io::Error::last_os_error());
			}
			Ok(())
		});
	}
	let out = tar.output().map_err(|e| format!("elamond update unpacks with tar: {e}"))?;
	if !out.status.success() {
		let said = String::from_utf8_lossy(&out.stderr);
		let why = said.lines().map(str::trim).rfind(|l| !l.is_empty()).map(plain).unwrap_or_else(|| format!("tar {}", out.status));
		return Err(format!("can't unpack {name}: {why}"));
	}
	let path = into.join(member);
	let folder = path.parent().map(fs::symlink_metadata);
	let file = fs::symlink_metadata(&path);
	match (folder, file) {
		(Some(Ok(d)), Ok(f)) if d.file_type().is_dir() && f.file_type().is_file() && f.nlink() == 1 && f.len() > 0 => Ok(path),
		_ => Err(format!("{name} holds no elamond of its own (a link, or not a file)")),
	}
}

// ---- replacing this binary ------------------------------------------------------------------

/// Why `target`, this elamond, can't be replaced where it is, if it can't.
fn replaceable(target: &Path) -> Result<(), String> {
	if target.starts_with("/nix/store") {
		return Err(NIX.into());
	}
	if !fs::symlink_metadata(target).is_ok_and(|m| m.file_type().is_file()) {
		return Err(format!("{} isn't a file of its own any more (a link?): run elamond update again", target.display()));
	}
	let dir = target.parent().unwrap_or(Path::new("/"));
	if !writable(dir) {
		return Err(format!(
			"this elamond is in {}, where you can't write: update it the way it was put there, or install it for \
			 yourself with install.sh (guide, section 1)",
			dir.display()
		));
	}
	Ok(())
}

fn writable(dir: &Path) -> bool {
	let Ok(c) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else { return false };
	// SAFETY: access only reads the path it is given.
	unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
}

/// Whether `target` is still the file this process runs: it may have been replaced since it started.
fn still_running(target: &Path) -> Result<(), String> {
	match (fs::metadata("/proc/self/exe"), fs::symlink_metadata(target)) {
		(Ok(running), Ok(there)) if there.file_type().is_file() && (running.dev(), running.ino()) == (there.dev(), there.ino()) => Ok(()),
		_ => Err(format!("{} isn't the elamond that runs any more (replaced since it started): run elamond update again", target.display())),
	}
}

/// `new` copied beside `target` under a name of its own, checked there to say `version` (it runs from
/// that folder, whatever the temporary one allows), and renamed over `target`.
fn put_in_place(new: &Path, target: &Path, version: Version) -> Result<(), String> {
	let dir = target.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
	let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
	let part = dir.join(format!(".{name}.update.{}.{}", std::process::id(), HEXLOWER.encode(&store::random_bytes::<4>())));
	let placed = (|| {
		{
			let mut from = fs::File::open(new).map_err(|e| format!("can't read {}: {e}", new.display()))?;
			// create_new: never through a link, nor over a file someone put there first.
			let mut to = fs::OpenOptions::new()
				.write(true)
				.create_new(true)
				.mode(0o700)
				.open(&part)
				.map_err(|e| format!("can't write in {}: {e}", dir.display()))?;
			let copied = io::copy(&mut from, &mut to)
				.and_then(|_| to.set_permissions(fs::Permissions::from_mode(0o755)))
				.and_then(|_| to.sync_all());
			copied.map_err(|e| format!("can't write {}: {e}", part.display()))?;
		}
		says_version(&part, version)?;
		fs::rename(&part, target).map_err(|e| format!("can't put elamond at {}: {e}", target.display()))?;
		// The rename on disk too, before the service starts again on it.
		if let Ok(d) = fs::File::open(dir) {
			let _ = d.sync_all();
		}
		Ok::<(), String>(())
	})();
	if placed.is_err() {
		let _ = fs::remove_file(&part);
	}
	placed
}

/// Whether `exe --version` says `elamond VERSION`, within VERSION_WAIT.
fn says_version(exe: &Path, version: Version) -> Result<(), String> {
	let mut tries = 0;
	let mut child = loop {
		let spawned = Command::new(exe).arg("--version").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
		match spawned {
			// Another thread forking while the file was open for writing holds it a moment.
			Err(e) if e.raw_os_error() == Some(libc::ETXTBSY) && tries < 20 => {
				tries += 1;
				std::thread::sleep(Duration::from_millis(50));
			}
			Err(e) => return Err(format!("the elamond downloaded doesn't run here: {e}")),
			Ok(child) => break child,
		}
	};
	let started = Instant::now();
	let status = loop {
		match child.try_wait() {
			Ok(Some(status)) => break status,
			Ok(None) if started.elapsed() < VERSION_WAIT => std::thread::sleep(Duration::from_millis(20)),
			_ => {
				let _ = child.kill();
				let _ = child.wait();
				return Err("the elamond downloaded doesn't answer --version".into());
			}
		}
	};
	let mut said = Vec::new();
	if let Some(out) = child.stdout.take() {
		let _ = out.take(4096).read_to_end(&mut said);
	}
	let said = plain(String::from_utf8_lossy(&said).trim());
	let want = format!("elamond {version}");
	if !status.success() || said != want {
		return Err(format!("the elamond downloaded says {said:?}, not {want}"));
	}
	Ok(())
}

/// The service started again on the new binary through the init system, if it runs and runs this
/// one; what happened, as a line to show.
fn restart(init: &dyn InitSystem, exe: &str) -> String {
	if !init.running() {
		return format!("  -   {} isn't running here: start `elamond serve` again to run the new version", init.name());
	}
	if let Err(e) = init.user_manager() {
		return format!(
			"  ?   {}'s user manager doesn't answer ({}): systemctl --user try-restart elamond, in a session of your own",
			init.name(),
			plain(&e)
		);
	}
	let status = init.status();
	if status.active.as_deref() != Some("active") {
		return if status.loaded == Some(true) {
			"  -   the service isn't running; it runs the new version when it starts: systemctl --user start elamond".into()
		} else {
			"  -   the service isn't set up yet: elamond setup".into()
		};
	}
	let runs = status.path.as_deref().and_then(|p| fs::read_to_string(p).ok()).and_then(|t| init.unit_exe(&t));
	let same = runs.as_deref().is_some_and(|r| r == exe || fs::canonicalize(r).is_ok_and(|r| fs::canonicalize(exe).is_ok_and(|e| e == r)));
	if !same {
		return format!("  !   the service runs {}, not this elamond: elamond setup points it here", runs.as_deref().unwrap_or("another elamond"));
	}
	for c in init.restart_if_running() {
		if let Err(e) = setup::run_command(&c) {
			return format!("  !   {e}");
		}
	}
	"  OK  started the service again on it".into()
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::collections::HashMap;
	use std::net::{TcpListener, TcpStream};
	use std::sync::{Arc, Mutex};

	use serde_json::json;

	use crate::setup::Status;

	const ARCH: &str = std::env::consts::ARCH;
	const GITHUB: &str = "https://github.com/Elaizes/elamon/releases/download/";

	/// A folder of the test's own, removed when the test ends well.
	fn folder(tag: &str) -> Scratch {
		let dir = std::env::temp_dir().join(format!("elamond-test-{tag}-{}", HEXLOWER.encode(&store::random_bytes::<6>())));
		fs::create_dir_all(&dir).unwrap();
		Scratch(dir)
	}

	/// A web server on 127.0.0.1 for the test, answering each path it was given; `/endless` sends
	/// without end, telling no length. What each request said is kept.
	struct Server {
		base: String,
		routes: Arc<Mutex<HashMap<String, Vec<u8>>>>,
		heads: Arc<Mutex<Vec<String>>>,
	}

	impl Server {
		fn start() -> Server {
			let listener = TcpListener::bind("127.0.0.1:0").unwrap();
			let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
			let routes = Arc::new(Mutex::new(HashMap::new()));
			let heads = Arc::new(Mutex::new(Vec::new()));
			let (r, h) = (routes.clone(), heads.clone());
			std::thread::spawn(move || {
				for conn in listener.incoming().flatten() {
					let (r, h) = (r.clone(), h.clone());
					std::thread::spawn(move || answer(conn, &r, &h));
				}
			});
			Server { base, routes, heads }
		}

		fn put(&self, path: &str, body: impl Into<Vec<u8>>) {
			self.routes.lock().unwrap().insert(path.into(), body.into());
		}

		fn url(&self, path: &str) -> String {
			format!("{}{path}", self.base)
		}
	}

	fn answer(mut conn: TcpStream, routes: &Mutex<HashMap<String, Vec<u8>>>, heads: &Mutex<Vec<String>>) {
		let _ = conn.set_read_timeout(Some(Duration::from_secs(5)));
		let (mut head, mut byte) = (Vec::new(), [0u8; 1]);
		while !head.ends_with(b"\r\n\r\n") && head.len() < 16 * 1024 {
			match conn.read(&mut byte) {
				Ok(1) => head.push(byte[0]),
				_ => return,
			}
		}
		let head = String::from_utf8_lossy(&head).into_owned();
		let path = head.split_whitespace().nth(1).unwrap_or("/").to_string();
		heads.lock().unwrap().push(head);
		if path == "/endless" {
			let _ = conn.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n");
			let chunk = vec![b' '; 64 * 1024];
			for _ in 0..1024 {
				if conn.write_all(&chunk).is_err() {
					return;
				}
			}
			return;
		}
		let reply = match routes.lock().unwrap().get(&path) {
			Some(body) => [format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes(), body.clone()].concat(),
			None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
		};
		let _ = conn.write_all(&reply);
	}

	/// A release archive as build-release.sh makes it, `folder/elamond` a script that says `says`.
	fn archive(work: &Path, folder: &str, says: &str) -> Vec<u8> {
		archive_with(work, folder, |bin| {
			fs::write(bin, format!("#!/bin/sh\necho '{says}'\n")).unwrap();
			fs::set_permissions(bin, fs::Permissions::from_mode(0o755)).unwrap();
		})
	}

	fn archive_with(work: &Path, folder: &str, make: impl FnOnce(&Path)) -> Vec<u8> {
		let stage = work.join(format!("stage-{}", HEXLOWER.encode(&store::random_bytes::<4>())));
		fs::create_dir_all(stage.join(folder)).unwrap();
		make(&stage.join(folder).join("elamond"));
		fs::write(stage.join(folder).join("README.md"), "# elamond\n").unwrap();
		let out = stage.join("archive.tar.gz");
		let ok = Command::new("tar").arg("-czf").arg(&out).arg("-C").arg(&stage).arg(folder).status().unwrap();
		assert!(ok.success());
		fs::read(out).unwrap()
	}

	fn sum_line(data: &[u8], name: &str) -> String {
		format!("{}  {name}\n", HEXLOWER.encode(&Sha256::digest(data)))
	}

	/// A release as GitHub lists it, with elamond's archive and checksum for each of `archs`.
	fn release(base: &str, version: &str, archs: &[&str], draft: bool) -> serde_json::Value {
		let assets: Vec<_> = archs
			.iter()
			.flat_map(|a| {
				let file = format!("elamond-{version}-{a}-linux.tar.gz");
				[format!("{file}.sha256"), file]
			})
			.map(|f| json!({ "name": f, "label": null, "size": 1, "browser_download_url": format!("{base}/download/v{version}/{f}") }))
			.collect();
		json!({ "tag_name": format!("v{version}"), "draft": draft, "prerelease": true, "html_url": format!("{base}/tag/v{version}"), "body": null, "assets": assets })
	}

	/// A server with a release of `version` for this architecture, whose elamond says `says`, and its
	/// checksum (`sum` instead, when given); and the release's address.
	fn serve_release(server: &Server, work: &Path, version: &str, says: &str, sum: Option<&str>) {
		let folder = format!("elamond-{version}-{ARCH}-linux");
		let data = archive(work, &folder, says);
		let line = sum.map(String::from).unwrap_or_else(|| sum_line(&data, &format!("{folder}.tar.gz")));
		server.put(&format!("/download/v{version}/{folder}.tar.gz"), data);
		server.put(&format!("/download/v{version}/{folder}.tar.gz.sha256"), line);
	}

	fn fetcher() -> Fetcher {
		Fetcher::find().expect("the tests download with curl or wget, and neither is installed")
	}

	fn list(server: &Server) -> Vec<Release> {
		read_list(fetcher(), &server.url("/releases")).unwrap()
	}

	fn this() -> (Version, bool) {
		this_version()
	}

	fn v(s: &str) -> Version {
		Version::parse(s).unwrap()
	}

	/// The file at `target`, with what it holds, and nothing else beside it.
	fn only(dir: &Path, target: &Path, holds: &str) {
		assert_eq!(fs::read_to_string(target).unwrap(), holds);
		let names: Vec<_> = fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name()).collect();
		assert_eq!(names, [target.file_name().unwrap()], "nothing left beside it");
	}

	#[test]
	fn versions_are_x_y_z() {
		assert_eq!(Version::parse("v0.6.1"), Some(Version { major: 0, minor: 6, patch: 1 }));
		assert_eq!(Version::parse("10.20.30"), Some(Version { major: 10, minor: 20, patch: 30 }));
		for not in ["0.6", "0.6.1.2", "0.8.0-rc.1", "v", "", "0..1", "0.6.x", "+1.2.3", "0.6.1 "] {
			assert_eq!(Version::parse(not), None, "{not:?}");
		}
		assert!(v("0.6.10") > v("0.6.9") && v("0.10.0") > v("0.9.9") && v("1.0.0") > v("0.99.99"));
		assert_eq!(version_of("0.6.0"), (v("0.6.0"), false));
		assert_eq!(version_of("0.7.0-rc.1"), (v("0.7.0"), true));
		assert_eq!(version_of("0.7.0+build.5"), (v("0.7.0"), false));
		assert_eq!(this().0.to_string(), env!("CARGO_PKG_VERSION").split(['-', '+']).next().unwrap());
	}

	#[test]
	fn github_s_list_as_it_sends_it() {
		let releases = parse_list(include_bytes!("update/github-releases.json")).unwrap();
		assert_eq!(releases.len(), 6);
		let above = |s: &str, arch: &str| newest(&releases, version_of(s), arch, false).map(|f| f.version.to_string());
		// The draft (0.8.0) never; a release candidate never; 0.6.2, out after 0.7.0, not newer than it.
		assert_eq!(above("0.6.0", "x86_64").as_deref(), Some("0.7.0"));
		assert_eq!(above("0.6.0", "aarch64").as_deref(), Some("0.7.1"), "0.7.1 has elamond for aarch64 only");
		assert_eq!(above("0.7.0", "x86_64"), None);
		assert_eq!(above("0.7.0-rc.2", "x86_64").as_deref(), Some("0.7.0"), "a release is newer than its candidates");
		assert_eq!(above("0.7.1", "aarch64"), None);
		assert_eq!(above("0.6.1", "riscv64"), None, "no elamond for it anywhere");
		assert_eq!(above("0.4.0", "x86_64").as_deref(), Some("0.7.0"), "0.5.0 has no checksum, and isn't the newest either");

		let found = newest(&releases, version_of("0.6.0"), "x86_64", false).unwrap();
		assert_eq!(
			found,
			Found {
				version: v("0.7.0"),
				archive: "elamond-0.7.0-x86_64-linux.tar.gz".into(),
				archive_url: format!("{GITHUB}v0.7.0/elamond-0.7.0-x86_64-linux.tar.gz"),
				sum_url: format!("{GITHUB}v0.7.0/elamond-0.7.0-x86_64-linux.tar.gz.sha256"),
				page: Some("https://github.com/Elaizes/elamon/releases/tag/v0.7.0".into()),
			}
		);
		assert_eq!(found.folder(), "elamond-0.7.0-x86_64-linux");

		// Minified, the same.
		let minified = serde_json::to_vec(&serde_json::from_slice::<serde_json::Value>(include_bytes!("update/github-releases.json")).unwrap()).unwrap();
		assert_eq!(newest(&parse_list(&minified).unwrap(), version_of("0.6.0"), "x86_64", false), Some(found));
		// What isn't a list of releases says so.
		assert!(parse_list(br#"{"message": "API rate limit exceeded"}"#).unwrap_err().contains("isn't what GitHub sends"));
	}

	#[test]
	fn an_asset_counts_only_by_its_name_and_address_both() {
		let base = "https://example.net";
		let mut r = release(base, "9.9.9", &[ARCH], false);
		// The address names another file than the asset's name.
		r["assets"][1]["browser_download_url"] = json!(format!("{base}/download/v9.9.9/other.tar.gz"));
		let releases: Vec<Release> = serde_json::from_value(json!([r])).unwrap();
		assert_eq!(newest(&releases, this(), ARCH, false), None);
		// Without a draft flag: a draft.
		let mut r = release(base, "9.9.9", &[ARCH], false);
		r.as_object_mut().unwrap().remove("draft");
		let releases: Vec<Release> = serde_json::from_value(json!([r])).unwrap();
		assert_eq!(newest(&releases, this(), ARCH, false), None);
	}

	#[test]
	fn only_https_or_this_computer_when_testing() {
		assert!(allowed("https://api.github.com/repos/Elaizes/elamon/releases?per_page=100", false));
		assert!(allowed("https://objects.example.net/x", false));
		for url in ["http://127.0.0.1:8080/releases", "http://localhost/x", "http://[::1]:9/x", "http://127.0.0.1", "http://localhost:80"] {
			assert!(allowed(url, true), "{url}");
			assert!(!allowed(url, false), "{url}: plain http only when testing");
		}
		for url in [
			"http://example.net/x",
			"http://127.0.0.1.example.net/x",
			"http://127.0.0.1:80@example.net/x",
			"http://localhost:/x",
			"http://localhost:8a/x",
			"http://[::1]x/",
			"http://127.0.0.1#@example.net/",
			"ftp://127.0.0.1/x",
			"file:///etc/passwd",
			"https://",
			"https://example.net/a b",
			"https://example.net/\u{1b}[2J",
			"-K/etc/passwd",
		] {
			assert!(!allowed(url, true), "{url}");
		}
	}

	#[test]
	fn the_checksum_file_names_the_archive_and_nothing_else() {
		let name = "elamond-9.9.9-x86_64-linux.tar.gz";
		let sum = "ab".repeat(32);
		assert_eq!(expected_sum(format!("{sum}  {name}\n").as_bytes(), name), Ok(sum.clone()));
		assert_eq!(expected_sum(format!("{}  *{name}", sum.to_uppercase()).as_bytes(), name), Ok(sum.clone()), "binary mode, upper case, no newline");
		let refused = |text: String| expected_sum(text.as_bytes(), name).unwrap_err();
		assert!(refused(format!("{sum}  ../../.bashrc\n")).contains("another file"));
		assert!(refused(format!("{sum}  elamond-9.9.9-aarch64-linux.tar.gz\n")).contains("another file"));
		assert!(refused(format!("{sum}  {name}\n{sum}  other\n")).contains("one checksum"));
		assert!(refused(String::new()).contains("one checksum"));
		assert!(refused(format!("{sum}\n")).contains("form"));
		assert!(refused(format!("{sum}  {name} x\n")).contains("form"));
		assert!(refused(format!("{}  {name}\n", &sum[2..])).contains("no SHA-256"));
		assert!(refused(format!("{}zz  {name}\n", &sum[2..])).contains("no SHA-256"));
		assert!(expected_sum(b"\xff\xfe", name).unwrap_err().contains("isn't text"));
	}

	#[test]
	fn a_newer_release_is_downloaded_checked_and_put_in_place() {
		let work = folder("newer");
		let server = Server::start();
		serve_release(&server, work.path(), "9.9.9", "elamond 9.9.9", None);
		server.put(
			"/releases",
			json!([
				release(&server.base, "99.0.0", &[ARCH], true),
				release(&server.base, "9.9.9", &[ARCH, "riscv64"], false),
				release(&server.base, env!("CARGO_PKG_VERSION"), &[ARCH], false),
			])
			.to_string(),
		);
		let found = newest(&list(&server), this(), ARCH, true).unwrap();
		assert_eq!(found.version, v("9.9.9"), "not the draft");
		assert_eq!(found.page.as_deref(), Some(server.url("/tag/v9.9.9").as_str()));
		// Not testing, the files are taken only from the project's own downloads on GitHub.
		assert!(newest(&list(&server), this(), ARCH, false).is_none(), "files from elsewhere only when testing");

		let scratch = Scratch::under(work.path()).unwrap();
		let new = download(fetcher(), &found, true, scratch.path()).unwrap();
		assert!(new.starts_with(scratch.path()));
		let bin = work.path().join("bin");
		fs::create_dir(&bin).unwrap();
		let target = bin.join("elamond");
		fs::write(&target, "the old one").unwrap();
		replaceable(&target).unwrap();
		put_in_place(&new, &target, found.version).unwrap();
		only(&bin, &target, "#!/bin/sh\necho 'elamond 9.9.9'\n");
		assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o7777, 0o755);
		let scratch_path = scratch.path().to_path_buf();
		drop(scratch);
		assert!(!scratch_path.exists(), "the downloads go with the scratch folder");

		// The list was asked for anonymously: the user agent, what is accepted, nothing else of note.
		let heads = server.heads.lock().unwrap().clone();
		let head = heads.iter().find(|h| h.starts_with("GET /releases ")).unwrap().to_ascii_lowercase();
		assert!(head.contains(&format!("\r\nuser-agent: elamond/{}\r\n", env!("CARGO_PKG_VERSION"))), "{head}");
		if fetcher() == Fetcher::Curl {
			assert!(head.contains("\r\naccept: application/vnd.github+json\r\n"), "{head}");
		}
		for h in &heads {
			let h = h.to_ascii_lowercase();
			assert!(!h.contains("\r\nauthorization:") && !h.contains("\r\ncookie:") && !h.contains("\r\nfrom:"), "{h}");
		}
	}

	#[test]
	fn the_same_version_is_up_to_date() {
		let server = Server::start();
		server.put(
			"/releases",
			json!([release(&server.base, env!("CARGO_PKG_VERSION"), &[ARCH, "aarch64", "x86_64"], false), release(&server.base, "0.1.0", &[ARCH], false)])
				.to_string(),
		);
		assert_eq!(newest(&list(&server), this(), ARCH, true), None);
	}

	#[test]
	fn no_elamond_for_this_architecture_is_no_update() {
		let server = Server::start();
		server.put("/releases", json!([release(&server.base, "9.9.9", &["riscv64", "s390x"], false)]).to_string());
		assert_eq!(newest(&list(&server), this(), ARCH, true), None);
	}

	#[test]
	fn a_corrupted_checksum_changes_nothing() {
		let work = folder("corrupt");
		let server = Server::start();
		let folder_name = format!("elamond-9.9.9-{ARCH}-linux");
		serve_release(&server, work.path(), "9.9.9", "elamond 9.9.9", Some(&format!("{}  {folder_name}.tar.gz\n", "0".repeat(64))));
		server.put("/releases", json!([release(&server.base, "9.9.9", &[ARCH], false)]).to_string());
		let found = newest(&list(&server), this(), ARCH, true).unwrap();
		let scratch = Scratch::under(work.path()).unwrap();
		let e = download(fetcher(), &found, true, scratch.path()).unwrap_err();
		assert!(e.contains("doesn't match its SHA-256"), "{e}");
		assert!(!scratch.path().join("x").exists(), "nothing unpacked");

		// Nor one cut short: the archive served is half of what was summed.
		let data = archive(work.path(), &folder_name, "elamond 9.9.9");
		server.put(&format!("/download/v9.9.9/{folder_name}.tar.gz.sha256"), sum_line(&data, &format!("{folder_name}.tar.gz")));
		server.put(&format!("/download/v9.9.9/{folder_name}.tar.gz"), data[..data.len() / 2].to_vec());
		let scratch = Scratch::under(work.path()).unwrap();
		assert!(download(fetcher(), &found, true, scratch.path()).unwrap_err().contains("doesn't match its SHA-256"));
	}

	#[test]
	fn plain_http_isnt_taken_unless_testing_nor_a_missing_file() {
		let work = folder("refused");
		let server = Server::start();
		server.put("/releases", json!([release(&server.base, "9.9.9", &[ARCH], false)]).to_string());
		let found = newest(&list(&server), this(), ARCH, true).unwrap();
		let scratch = Scratch::under(work.path()).unwrap();
		assert!(download(fetcher(), &found, false, scratch.path()).unwrap_err().contains("only https"));
		// Not on the server: curl and wget fail, and say so.
		let e = download(fetcher(), &found, true, scratch.path()).unwrap_err();
		assert!(e.starts_with("can't download http://127.0.0.1:"), "{e}");
	}

	#[test]
	fn a_response_too_large_is_cut_off() {
		let server = Server::start();
		server.put("/big", vec![b'x'; 300 * 1024]);
		let mut out = Vec::new();
		let e = fetch(fetcher(), &server.url("/big"), None, 100 * 1024, &mut out).unwrap_err();
		assert!(e.contains("is larger than it can be (over "), "{e}");
		// Without a length told: read until the limit, then stopped.
		let mut out = Vec::new();
		let e = fetch(fetcher(), &server.url("/endless"), None, 1 << 20, &mut out).unwrap_err();
		assert!(e.contains("is larger than it can be (over "), "{e}");
		assert!(out.len() <= 1 << 20);
	}

	#[test]
	fn wget_where_there_is_no_curl() {
		if !on_path("wget") {
			eprintln!("no wget here");
			return;
		}
		let server = Server::start();
		server.put("/releases", json!([release(&server.base, "9.9.9", &[ARCH], false)]).to_string());
		let found = newest(&read_list(Fetcher::Wget, &server.url("/releases")).unwrap(), this(), ARCH, true).unwrap();
		assert_eq!(found.version, v("9.9.9"));
		let head = server.heads.lock().unwrap()[0].to_ascii_lowercase();
		assert!(head.contains(&format!("\r\nuser-agent: elamond/{}\r\n", env!("CARGO_PKG_VERSION"))), "{head}");
		assert!(head.contains("\r\naccept: application/vnd.github+json\r\n"), "{head}");
		let e = fetch(Fetcher::Wget, &server.url("/endless"), None, 1 << 20, &mut Vec::new()).unwrap_err();
		assert!(e.contains("is larger than it can be (over "), "{e}");
		let e = fetch(Fetcher::Wget, &server.url("/missing"), None, 1 << 20, &mut Vec::new()).unwrap_err();
		assert!(e.starts_with(&server.url("/missing: ")), "{e}");
	}

	#[test]
	fn an_elamond_that_says_another_version_isnt_put_in_place() {
		let work = folder("version");
		let server = Server::start();
		serve_release(&server, work.path(), "9.9.9", "elamond 9.9.8", None);
		server.put("/releases", json!([release(&server.base, "9.9.9", &[ARCH], false)]).to_string());
		let found = newest(&list(&server), this(), ARCH, true).unwrap();
		let scratch = Scratch::under(work.path()).unwrap();
		let new = download(fetcher(), &found, true, scratch.path()).unwrap();
		let bin = work.path().join("bin");
		fs::create_dir(&bin).unwrap();
		let target = bin.join("elamond");
		fs::write(&target, "the old one").unwrap();
		let e = put_in_place(&new, &target, found.version).unwrap_err();
		assert!(e.contains("says \"elamond 9.9.8\", not elamond 9.9.9"), "{e}");
		only(&bin, &target, "the old one");
	}

	#[test]
	fn an_archive_whose_elamond_is_a_link_is_refused() {
		let work = folder("link");
		let folder_name = format!("elamond-9.9.9-{ARCH}-linux");
		let data = archive_with(work.path(), &folder_name, |bin| std::os::unix::fs::symlink("/bin/sh", bin).unwrap());
		let path = work.path().join(format!("{folder_name}.tar.gz"));
		fs::write(&path, data).unwrap();
		let e = unpack(&path, &work.path().join("x"), &format!("{folder_name}/elamond")).unwrap_err();
		assert!(e.contains("holds no elamond of its own"), "{e}");
		// One without it at all.
		let data = archive(work.path(), "elsewhere", "elamond 9.9.9");
		fs::write(&path, data).unwrap();
		let e = unpack(&path, &work.path().join("y"), &format!("{folder_name}/elamond")).unwrap_err();
		assert!(e.starts_with(&format!("can't unpack {folder_name}.tar.gz: ")), "{e}");
	}

	#[test]
	fn places_it_leaves_alone() {
		assert_eq!(replaceable(Path::new("/nix/store/0000-elamond-0.6.0/bin/.elamond-wrapped")), Err(NIX.into()));
		let work = folder("places");
		let real = work.path().join("real");
		fs::write(&real, "x").unwrap();
		let link = work.path().join("elamond");
		std::os::unix::fs::symlink(&real, &link).unwrap();
		assert!(replaceable(&link).unwrap_err().contains("isn't a file of its own"));
		assert!(replaceable(&work.path().join("gone")).unwrap_err().contains("isn't a file of its own"));
		replaceable(&real).unwrap();
		assert!(still_running(&real).is_err(), "not the file this test runs");
		still_running(&std::env::current_exe().unwrap()).unwrap();
		// SAFETY: geteuid only reads the process's user id.
		if unsafe { libc::geteuid() } != 0 {
			let locked = work.path().join("locked");
			fs::create_dir(&locked).unwrap();
			fs::write(locked.join("elamond"), "x").unwrap();
			fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
			let e = replaceable(&locked.join("elamond")).unwrap_err();
			fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
			assert!(e.contains(&format!("is in {}, where you can't write", locked.display())), "{e}");
		}
	}

	/// An init system that answers as told, and restarts with a command that does nothing.
	struct Fake {
		running: bool,
		manager: bool,
		status: fn(&Path) -> Status,
		unit: PathBuf,
	}

	impl InitSystem for Fake {
		fn name(&self) -> &'static str {
			"fake"
		}
		fn running(&self) -> bool {
			self.running
		}
		fn user_manager(&self) -> Result<(), String> {
			if self.manager { Ok(()) } else { Err("no bus".into()) }
		}
		fn unit_path(&self) -> PathBuf {
			self.unit.clone()
		}
		fn unit_text(&self, exe: &str) -> String {
			format!("ExecStart={exe} serve\n")
		}
		fn unit_exe(&self, text: &str) -> Option<String> {
			text.strip_prefix("ExecStart=")?.split_whitespace().next().map(String::from)
		}
		fn status(&self) -> Status {
			(self.status)(&self.unit)
		}
		fn activate(&self, _: setup::Start) -> Vec<Vec<String>> {
			Vec::new()
		}
		fn restart_if_running(&self) -> Vec<Vec<String>> {
			vec![vec!["true".into()]]
		}
		fn lingers(&self, _: &str) -> Option<bool> {
			None
		}
		fn linger_command(&self, _: &str) -> String {
			String::new()
		}
		fn system_active(&self, _: &str) -> Option<bool> {
			None
		}
	}

	fn active(unit: &Path) -> Status {
		Status { loaded: Some(true), path: Some(unit.into()), active: Some("active".into()), ..Status::default() }
	}

	fn stopped(unit: &Path) -> Status {
		Status { loaded: Some(true), path: Some(unit.into()), active: Some("inactive".into()), ..Status::default() }
	}

	fn unknown(_: &Path) -> Status {
		Status::default()
	}

	#[test]
	fn the_service_starts_again_only_when_it_runs_this_binary() {
		assert_eq!(setup::Systemd.restart_if_running(), [["systemctl", "--user", "try-restart", "elamond"]]);
		let work = folder("restart");
		let unit = work.path().join("elamond.service");
		fs::write(&unit, "ExecStart=/opt/elamond serve\n").unwrap();
		let fake = |running: bool, manager: bool, status: fn(&Path) -> Status| Fake { running, manager, status, unit: unit.clone() };

		assert_eq!(restart(&fake(true, true, active), "/opt/elamond"), "  OK  started the service again on it");
		assert!(restart(&fake(true, true, active), "/home/u/.local/bin/elamond").contains("the service runs /opt/elamond, not this elamond"));
		assert!(restart(&fake(false, true, active), "/opt/elamond").contains("fake isn't running here"));
		assert!(restart(&fake(true, false, active), "/opt/elamond").contains("user manager doesn't answer (no bus)"));
		assert!(restart(&fake(true, true, stopped), "/opt/elamond").contains("isn't running; it runs the new version when it starts"));
		assert!(restart(&fake(true, true, unknown), "/opt/elamond").contains("isn't set up yet"));
	}

	#[test]
	fn install_sh_installs_the_newest_release() {
		let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../install.sh");
		let glibc = Command::new("getconf").arg("GNU_LIBC_VERSION").output().is_ok_and(|o| o.status.success());
		if !script.exists() || !glibc || !matches!(ARCH, "x86_64" | "aarch64") {
			eprintln!("install.sh isn't tried here: no install.sh beside elamond, no glibc or another architecture");
			return;
		}
		let work = folder("install-sh");
		let server = Server::start();
		let fixture = include_str!("update/github-releases.json").replace(GITHUB, &format!("{}/download/", server.base));
		server.put("/releases", fixture.clone());
		let minified = serde_json::to_string(&serde_json::from_str::<serde_json::Value>(&fixture).unwrap()).unwrap();
		server.put("/minified", minified);
		// The newest for each architecture in the fixture, and 0.6.2 asked for by name.
		for (version, arch) in [("0.7.0", "x86_64"), ("0.7.1", "aarch64"), ("0.6.2", ARCH)] {
			let folder = format!("elamond-{version}-{arch}-linux");
			let data = archive(work.path(), &folder, &format!("elamond {version}"));
			server.put(&format!("/download/v{version}/{folder}.tar.gz.sha256"), sum_line(&data, &format!("{folder}.tar.gz")));
			server.put(&format!("/download/v{version}/{folder}.tar.gz"), data);
		}
		let home = work.path().join("home");
		fs::create_dir(&home).unwrap();
		let install_with = |list: &str, bin: &Path, args: &[&str], path: &std::ffi::OsStr| {
			let out = Command::new("sh")
				.arg(&script)
				.args(args)
				.env("PATH", path)
				.env("HOME", &home)
				.env("ELAMON_BIN_DIR", bin)
				.env("ELAMON_RELEASES_URL", list)
				.stdin(Stdio::null())
				.output()
				.unwrap();
			(out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr))
		};
		let path = std::env::var_os("PATH").unwrap_or_default();
		let install = |list: &str, bin: &Path, args: &[&str]| install_with(list, bin, args, &path);
		let bin = work.path().join("bin");
		// SAFETY: geteuid only reads the process's user id.
		if unsafe { libc::geteuid() } == 0 {
			let (ok, said) = install(&server.url("/releases"), &bin, &["--no-setup"]);
			assert!(!ok && said.contains("not root"), "{said}");
			return;
		}
		let newest = if ARCH == "x86_64" { "0.7.0" } else { "0.7.1" };
		for list in ["/releases", "/minified"] {
			let bin = work.path().join(format!("bin{}", list.replace('/', "-")));
			// --force: an elamond on this computer's PATH isn't this test's.
			let (ok, said) = install(&server.url(list), &bin, &["--no-setup", "--force"]);
			assert!(ok, "{said}");
			assert!(said.contains(&format!("Downloading elamond {newest} for {ARCH}")) && said.contains("OK  SHA-256 matches"), "{said}");
			assert!(said.contains(&format!("release notes: https://github.com/Elaizes/elamon/releases/tag/v{newest}")), "{said}");
			assert!(said.contains("isn't on your PATH"), "{said}");
			only(&bin, &bin.join("elamond"), &format!("#!/bin/sh\necho 'elamond {newest}'\n"));
			assert_eq!(fs::metadata(bin.join("elamond")).unwrap().permissions().mode() & 0o777, 0o755);
		}
		// Already there: it says so, and changes nothing.
		let (ok, said) = install(&server.url("/releases"), &bin.with_file_name("bin-releases"), &["--no-setup"]);
		assert!(ok && said.contains(&format!("elamond {newest} is already installed")) && said.contains("elamond update"), "{said}");
		// A version asked for by name.
		let (ok, said) = install(&server.url("/releases"), &bin, &["--no-setup", "--force", "--version", "v0.6.2"]);
		assert!(ok, "{said}");
		only(&bin, &bin.join("elamond"), "#!/bin/sh\necho 'elamond 0.6.2'\n");
		// Neither a draft nor one without elamond for this architecture.
		for (version, why) in [("0.8.0", "draft"), ("0.5.0", "no checksum"), ("9.9.9", "none")] {
			let (ok, said) = install(&server.url("/releases"), &bin, &["--no-setup", "--force", "--version", version]);
			assert!(!ok && said.contains(&format!("there's no release {version} with elamond for {ARCH}")), "{why}: {said}");
		}
		// A checksum that doesn't match: nothing installed.
		let folder = format!("elamond-0.6.2-{ARCH}-linux");
		server.put(&format!("/download/v0.6.2/{folder}.tar.gz.sha256"), format!("{}  {folder}.tar.gz\n", "0".repeat(64)));
		let fresh = work.path().join("bin-corrupt");
		let (ok, said) = install(&server.url("/releases"), &fresh, &["--no-setup", "--version", "0.6.2", "--force"]);
		assert!(!ok && said.contains("doesn't match its SHA-256") && !fresh.join("elamond").exists(), "{said}");
		// Nor one naming another file.
		server.put(&format!("/download/v0.6.2/{folder}.tar.gz.sha256"), format!("{}  ../{folder}.tar.gz\n", "0".repeat(64)));
		let (ok, said) = install(&server.url("/releases"), &fresh, &["--no-setup", "--version", "0.6.2", "--force"]);
		assert!(!ok && said.contains("checksum of another file"), "{said}");
		// Plain http from elsewhere, never.
		let (ok, said) = install("http://192.0.2.1/releases", &fresh, &["--no-setup", "--force"]);
		assert!(!ok && said.contains("only https is taken"), "{said}");
		// Options for elamond setup with --no-setup, and a word that is none.
		let (ok, said) = install(&server.url("/releases"), &fresh, &["--no-setup", "--no-start"]);
		assert!(!ok && said.contains("--no-start would be for elamond setup"), "{said}");
		let (ok, said) = install(&server.url("/releases"), &fresh, &["0.6.2"]);
		assert!(!ok && said.contains("unknown argument"), "{said}");

		// With wget, where there is no curl: a PATH of what install.sh runs, curl left out.
		let tools = [
			"sh", "id", "uname", "getconf", "mktemp", "rm", "awk", "sort", "tail", "wc", "grep", "tr", "sha256sum", "mkdir", "tar", "gzip", "cp", "chmod", "mv",
			"readlink", "cat", "wget",
		];
		let shim = work.path().join("path");
		fs::create_dir(&shim).unwrap();
		if tools.iter().all(|t| which(t).is_some_and(|p| std::os::unix::fs::symlink(p, shim.join(t)).is_ok())) {
			let bin = work.path().join("bin-wget");
			let (ok, said) = install_with(&server.url("/minified"), &bin, &["--no-setup"], shim.as_os_str());
			assert!(ok, "{said}");
			only(&bin, &bin.join("elamond"), &format!("#!/bin/sh\necho 'elamond {newest}'\n"));
		} else {
			eprintln!("install.sh with wget isn't tried here: no wget, or not every tool it runs");
		}
	}
}
