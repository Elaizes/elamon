//! Events sent on to ntfy topics, the targets of the settings (`[notify.<id>]`): each a line of
//! its own, where an event waits its turn, is tried again when the server or the network fails,
//! and is given up once it is old. A topic's name is as secret as a token: it is read from its
//! file for each request and goes to curl on stdin alone, with the body it is part of.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Weak};
use std::time::Duration;

use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

use super::def::Target;
use super::rules::fill;
use super::{Events, Outgoing, Phase};
use crate::feeds::read::{first_line, run as run_program, Token};
use crate::settings::Config;

/// The answer's status code is read, nothing more of it.
const ANSWER_MAX: usize = 64;

/// A notification made of an event for a target, waiting to be sent.
struct Job {
	/// The event's `n`; none for the one that tells how many were held back.
	n: Option<u64>,
	title: String,
	message: String,
	priority: u8,
	tags: Vec<String>,
	born: Instant,
	tries: u32,
	next: Instant,
}

/// A target's line.
#[derive(Default)]
struct Line {
	queue: VecDeque<Job>,
	/// When each of the last notifications was taken in, within `per`.
	taken: VecDeque<Instant>,
	/// Events held back since, for being more than `burst`.
	held: u64,
}

/// What ntfy is asked to publish, as JSON.
pub fn body(topic: &str, title: &str, message: &str, priority: u8, tags: &[String]) -> String {
	let mut o = serde_json::Map::new();
	o.insert("topic".into(), topic.into());
	o.insert("title".into(), title.into());
	o.insert("message".into(), message.into());
	o.insert("priority".into(), priority.into());
	if !tags.is_empty() {
		o.insert("tags".into(), tags.to_vec().into());
	}
	serde_json::Value::Object(o).to_string()
}

/// A string as a curl config file takes it, in double quotes.
fn quoted(s: &str) -> String {
	format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The title, message, priority and tags of `o` for the target `t`, on the computer `computer`.
pub fn shaped(o: &Outgoing, t: &Target, computer: &str) -> (String, String, u8, Vec<String>) {
	let e = &o.event;
	let with = [
		("text", if t.names { e.text.as_str() } else { e.plain.as_str() }),
		("rule", e.rule.as_str()),
		("code", e.code.as_str()),
		("level", e.level.as_str()),
		("computer", if t.names { computer } else { "" }),
		("count", ""),
	];
	let title = match &e.title {
		Some(own) => own.clone(),
		None => fill(if t.names { &t.title_names } else { &t.title }, &with),
	};
	// A trouble's end is `recovered`'s, whatever its rule says of its start.
	let mood = if e.phase == Phase::End { "recovered" } else { e.level.as_str() };
	let own = (e.phase != Phase::End).then_some(());
	let priority = own.and(o.priority).or_else(|| t.priority.get(mood).copied()).unwrap_or_default();
	let tags = own.and(o.tags.clone()).or_else(|| t.tags.get(mood).cloned()).unwrap_or_default();
	(title, fill(&t.message, &with), priority, tags)
}

/// curl's arguments for the target `t`: no config file of its own (`-q`, first), the headers and
/// the body read from stdin, http and https only, no redirect followed (a topic is never asked of
/// another server), within the timeout; the answer's status code alone printed.
pub fn curl_args(t: &Target) -> Vec<String> {
	let mut args: Vec<String> = ["-q", "--config", "-", "--silent", "--show-error", "--globoff", "--proto", "=http,https", "--max-redirs", "0"].map(String::from).to_vec();
	args.extend(["--max-time".into(), t.timeout.to_string(), "--output".into(), "/dev/null".into(), "--write-out".into(), "%{http_code}".into(), "--url".into(), t.url.clone()]);
	args
}

/// How a request ended.
enum Sent {
	Ok,
	/// Not worth another try: the topic or token file, or the server's refusal.
	Failed(String),
	/// The network, the server busy or down.
	Later(String),
}

/// Sends `job` to `t`: the topic and the token read now, and dropped after.
async fn send(job: &Job, t: &Target, curl: &str) -> Sent {
	let secrets = {
		let (topic_file, token_file) = (t.topic_file.clone(), t.token_file.clone());
		tokio::task::spawn_blocking(move || {
			let topic = Token::read_as(&topic_file, "topic_file")?;
			let token = token_file.map(|p| Token::read(&p)).transpose()?;
			Ok::<_, crate::feeds::Failure>((topic, token))
		})
		.await
	};
	let (topic, token) = match secrets {
		Ok(Ok(s)) => s,
		Ok(Err(f)) => return Sent::Failed(f.text()),
		Err(_) => return Sent::Later("couldn't read the topic".into()),
	};
	let mut config = format!("header = {}\n", quoted("Content-Type: application/json"));
	if let Some(token) = &token {
		config.push_str(&format!("header = {}\n", quoted(&format!("Authorization: Bearer {}", token.secret()))));
	}
	config.push_str(&format!("data-binary = {}\n", quoted(&body(topic.secret(), &job.title, &job.message, job.priority, &job.tags))));
	let args = curl_args(t);
	let ran = run_program(curl, &args, Some(config.as_bytes()), Duration::from_secs(t.timeout + 1), ANSWER_MAX).await;
	drop(config);
	let hides = |text: &str| topic.in_text(text) || token.as_ref().is_some_and(|k| k.in_text(text));
	let ran = match ran {
		Ok(r) => r,
		Err(e) => return Sent::Later(format!("can't run curl: {}", crate::feeds::read::said(&e))),
	};
	let code = String::from_utf8_lossy(&ran.out).trim().parse::<u16>().unwrap_or(0);
	match (ran.status.and_then(|s| s.code()), code) {
		(Some(0), 200..=299) => Sent::Ok,
		(Some(0), 429 | 500..=599) => Sent::Later(format!("HTTP {code}")),
		(Some(0), 300..=399) => Sent::Failed(format!("redirect to elsewhere ({code}), not followed")),
		(Some(0), _) => Sent::Failed(format!("HTTP {code}")),
		(None | Some(28), _) => Sent::Later("timed out".into()),
		// curl's own words, without its "curl: (7) "; never the topic or the token.
		_ => {
			let why = first_line(&ran.err).map(|l| l.split_once(") ").map_or(l.clone(), |(_, w)| w.to_string())).filter(|w| !hides(w));
			Sent::Later(why.unwrap_or_else(|| "couldn't connect".into()))
		}
	}
}

/// Takes `o` into the lines of the targets of `config` that want it.
fn take(lines: &mut BTreeMap<String, Line>, o: &Outgoing, config: &Config, computer: &str, events: &Events, now: Instant) {
	for (id, t) in config.notify.iter().filter(|(_, t)| t.enabled) {
		let e = &o.event;
		if !t.rules.is_empty() && !t.rules.contains(&e.rule) {
			continue;
		}
		if e.of < t.level {
			events.delivered(e.n, id, &format!("not sent: below {}", t.level.as_str()));
			continue;
		}
		let line = lines.entry(id.clone()).or_default();
		let per = Duration::from_secs(t.per);
		while line.taken.front().is_some_and(|at| now.duration_since(*at) >= per) {
			line.taken.pop_front();
		}
		if line.taken.len() >= t.burst as usize {
			line.held += 1;
			events.delivered(e.n, id, &format!("held back: more than {} in {} s", t.burst, t.per));
			continue;
		}
		line.taken.push_back(now);
		let (title, message, priority, tags) = shaped(o, t, computer);
		queue(line, Job { n: Some(e.n), title, message, priority, tags, born: now, tries: 0, next: now }, t, id, events);
	}
}

/// `job` at the end of `line`; beyond the target's `queue`, the oldest goes.
fn queue(line: &mut Line, job: Job, t: &Target, id: &str, events: &Events) {
	line.queue.push_back(job);
	while line.queue.len() > t.queue {
		if let Some(n) = line.queue.pop_front().and_then(|j| j.n) {
			events.delivered(n, id, "dropped: too many waiting");
		}
	}
}

/// Once a line that held events back has room again, one notification says how many.
fn tell_held(lines: &mut BTreeMap<String, Line>, config: &Config, computer: &str, events: &Events, now: Instant) {
	for (id, line) in lines.iter_mut().filter(|(_, l)| l.held > 0) {
		let Some(t) = config.notify.get(id).filter(|t| t.enabled) else { continue };
		let per = Duration::from_secs(t.per);
		while line.taken.front().is_some_and(|at| now.duration_since(*at) >= per) {
			line.taken.pop_front();
		}
		if line.taken.len() >= t.burst as usize {
			continue;
		}
		let count = line.held.to_string();
		let with = [("count", count.as_str()), ("computer", if t.names { computer } else { "" }), ("text", ""), ("rule", ""), ("code", ""), ("level", "")];
		let title = fill(if t.names { &t.title_names } else { &t.title }, &with);
		let job = Job {
			n: None,
			title,
			message: fill(&t.more, &with),
			priority: t.priority.get("info").copied().unwrap_or_default(),
			tags: t.tags.get("info").cloned().unwrap_or_default(),
			born: now,
			tries: 0,
			next: now,
		};
		line.held = 0;
		line.taken.push_back(now);
		queue(line, job, t, id, events);
	}
}

/// When there is next something to do: a job due, or a line that held events back getting room.
fn wake(lines: &BTreeMap<String, Line>, config: &Config) -> Option<Instant> {
	let jobs = lines.values().filter_map(|l| l.queue.front().map(|j| j.next));
	let room = lines.iter().filter(|(_, l)| l.held > 0).filter_map(|(id, l)| Some(*l.taken.front()? + Duration::from_secs(config.notify.get(id)?.per)));
	jobs.chain(room).min()
}

/// Sends the events that come on `rx` to the targets of the settings, as `settings` has them, for
/// as long as the service's events (`kept`) are there; `computer` is what a title with names
/// says, `curl` the program run.
pub async fn run(mut rx: mpsc::UnboundedReceiver<Outgoing>, mut settings: watch::Receiver<Arc<Config>>, kept: Weak<Events>, computer: String, curl: String) {
	let mut lines: BTreeMap<String, Line> = BTreeMap::new();
	loop {
		let config = settings.borrow_and_update().clone();
		{
			let Some(events) = kept.upgrade() else { return };
			// A target taken out of the settings, or turned off, lets go of what waited for it.
			lines.retain(|id, _| config.notify.get(id).is_some_and(|t| t.enabled));
			tell_held(&mut lines, &config, &computer, &events, Instant::now());
			for (id, line) in lines.iter_mut() {
				let Some(t) = config.notify.get(id) else { continue };
				while line.queue.front().is_some_and(|j| j.next <= Instant::now()) {
					let Some(mut job) = line.queue.pop_front() else { break };
					let tell = |result: &str| {
						if let Some(n) = job.n {
							events.delivered(n, id, result);
						}
					};
					// One that waited too long says nothing true any more.
					if job.born.elapsed() >= Duration::from_secs(t.expire) {
						tell("not sent: too old by the time it could be");
						continue;
					}
					match send(&job, t, &curl).await {
						Sent::Ok => tell("sent"),
						Sent::Failed(why) => {
							eprintln!("notify {id}: not sent: {why}");
							tell(&format!("failed: {why}"));
						}
						Sent::Later(why) if job.tries >= t.retries => {
							eprintln!("notify {id}: not sent after {} tries: {why}", job.tries + 1);
							tell(&format!("failed: {why}"));
						}
						Sent::Later(why) => {
							// Twice as long each time, up to `retry_max`; those behind it wait with it.
							let wait = t.retry.saturating_mul(1u64 << job.tries.min(32)).min(t.retry_max);
							if job.tries == 0 {
								eprintln!("notify {id}: {why}; trying again in {wait} s");
							}
							job.tries += 1;
							job.next = Instant::now() + Duration::from_secs(wait);
							line.queue.push_front(job);
							break;
						}
					}
				}
			}
		}
		let until = wake(&lines, &config);
		tokio::select! {
			got = rx.recv() => match (got, kept.upgrade()) {
				// The settings as they are now, let go of before anything is done with them.
				(Some(o), Some(events)) => {
					let config = settings.borrow().clone();
					take(&mut lines, &o, &config, &computer, &events, Instant::now());
				}
				_ => return,
			},
			_ = async { match until { Some(at) => tokio::time::sleep_until(at).await, None => std::future::pending().await } } => {}
			changed = settings.changed() => if changed.is_err() { return },
		}
	}
}
