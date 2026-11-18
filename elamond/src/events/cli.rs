//! `elamond notify` and `elamond events`: an event a script makes, and the events as they are kept.

use super::def::Lvl;
use super::{Events, Notified, Phase};
use crate::store;

/// `elamond notify [--level LEVEL] [--title TITLE] TEXT`: leaves TEXT for the service to make an
/// event of, as `[events.notify]` of the settings takes it.
pub fn notify(args: &[String]) -> Result<(), String> {
	let usage = || format!("usage: elamond notify [--level {}] [--title TITLE] TEXT", super::def::LEVELS.join("|"));
	let level_arg = |s: &str| Lvl::parse(s).ok_or_else(|| format!("no level \"{s}\"; the levels are {}", super::def::LEVELS.join(", ")));
	let (mut level, mut title, mut words) = (None, None, Vec::new());
	let mut rest = args.iter().map(String::as_str);
	while let Some(a) = rest.next() {
		match a {
			"--level" => level = Some(level_arg(rest.next().ok_or_else(usage)?)?),
			"--title" => title = Some(rest.next().ok_or_else(usage)?.to_string()),
			"--" => words.extend(rest.by_ref()),
			_ if a.starts_with("--level=") => level = Some(level_arg(&a["--level=".len()..])?),
			_ if a.starts_with("--title=") => title = Some(a["--title=".len()..].to_string()),
			_ if a.starts_with("--") => return Err(usage()),
			_ => words.push(a),
		}
	}
	let config = store::load_config()?;
	let text = super::clean(&words.join(" "), usize::MAX);
	if text.is_empty() {
		return Err(usage());
	}
	let max = config.events.notify.length;
	if text.chars().count() > max {
		return Err(format!("the text is {} characters, more than the {max} [events.notify] takes (length)", text.chars().count()));
	}
	let dir = store::data_dir();
	if !store::data_dir_held(&dir) {
		return Err("the Elamon service isn't running, so nothing takes the event. Start it: systemctl --user start elamond".into());
	}
	let n = Notified { t: crate::activity::now_ms(), level, title: title.map(|t| super::clean(&t, crate::consts::limit::NAME_MAX)).filter(|t| !t.is_empty()), text };
	super::spool(&dir, &n).map_err(|e| format!("can't leave the event for the service in {}: {e}", dir.display()))
}

/// `elamond events [-n COUNT]`: the last events as the file has them, oldest first, each with
/// how sending it went.
pub fn events(args: &[String]) -> Result<(), String> {
	let usage = || "usage: elamond events [-n COUNT]".to_string();
	let count = match args {
		[] => None,
		[flag, n] if flag == "-n" => Some(n.parse::<usize>().map_err(|_| usage())?),
		_ => return Err(usage()),
	};
	let config = store::load_config()?;
	let kept = Events::load(&store::data_dir(), std::sync::Arc::new(config));
	let mut newest = kept.newest(count.unwrap_or(usize::MAX));
	if newest.is_empty() {
		println!("No events are kept.");
		return Ok(());
	}
	newest.reverse();
	for (e, sent) in newest {
		let phase = match e.phase {
			Phase::Start => "",
			Phase::End => " (end)",
			Phase::Once => "",
		};
		println!("{}  {:<8}  {}  {}{phase}: {}", crate::format::format(crate::format::Shown::Num((e.t / 1000) as f64), "time", ""), e.level.as_str(), e.rule, e.code, e.text);
		for d in sent {
			println!("    {}: {}", d.target, d.result);
		}
	}
	Ok(())
}
