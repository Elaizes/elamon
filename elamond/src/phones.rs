//! Each paired phone's own profile: a table in elamond.d/phones.toml (docs/config.md#phones), made
//! when it pairs, kept for it when it pairs again, offered back to a phone of its name, and taken out
//! when it is unpaired as pairing wrote it. Written only under the paired phones' lock
//! (`store::lock_clients`), in one step, keeping everything else in the file as it was; never so
//! that the settings would no longer parse, never over a link, and never failing a pairing: a phone
//! the file can't take pairs without a table, as with 0.6.0, and the journal says why.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item, Table};

use crate::settings::{self, Config, Source};
use crate::store::{self, Client, Place, Profiles};

pub const FILE: &str = "phones.toml";

/// What a new phones.toml starts with.
const HEADER: &str = "# Profiles of the paired phones, one each: elamond pair adds them (docs/config.md#phones).\n\
	# Change how a phone looks here, key by key; `use` is the profile it starts from.\n";

/// The longest an id may be.
const ID_MAX: usize = 32;

pub fn path(place: &Place) -> PathBuf {
	place.dropins().join(FILE)
}

/// The id of a phone named `name`: its letters and digits of `a-z` and `0-9` (`Galaxy S25` →
/// `galaxy-s25`), anything else between them one `-`; `phone` when nothing is left. One `taken`
/// already gets `-2`, `-3`…
pub fn id_for(name: &str, taken: &BTreeSet<String>) -> String {
	let mut base = String::new();
	for c in name.chars().map(|c| c.to_ascii_lowercase()) {
		if c.is_ascii_lowercase() || c.is_ascii_digit() {
			base.push(c);
		} else if !base.ends_with('-') {
			base.push('-');
		}
	}
	let base: String = base.trim_matches('-').chars().take(ID_MAX).collect();
	let base = match base.trim_end_matches('-') {
		"" => "phone",
		b => b,
	};
	if !taken.contains(base) {
		return base.to_string();
	}
	(2..)
		.map(|n| {
			let suffix = format!("-{n}");
			let head: String = base.chars().take(ID_MAX - suffix.len()).collect();
			format!("{}{suffix}", head.trim_end_matches('-'))
		})
		.find(|id| !taken.contains(id))
		.unwrap_or_default()
}

/// phones.toml as it is: its text, none when there is none yet; why not, when it isn't the
/// service's to write (a link, something not a file) or can't be read.
fn read(place: &Place) -> Result<Option<String>, String> {
	let p = path(place);
	match fs::symlink_metadata(&p) {
		Ok(m) if m.file_type().is_symlink() => Err(format!("{} is a link, which the service leaves alone", store::shown(&p))),
		Ok(m) if !m.is_file() => Err(format!("{} isn't a file", store::shown(&p))),
		Ok(_) => fs::read_to_string(&p).map(Some).map_err(|e| format!("can't read {}: {e}", store::shown(&p))),
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
		Err(e) => Err(format!("can't look at {}: {e}", store::shown(&p))),
	}
}

fn parse(place: &Place, text: &str) -> Result<DocumentMut, String> {
	text.parse().map_err(|_| format!("{} doesn't parse (elamond check names the line)", store::shown(&path(place))))
}

/// The settings as they would be with `text` as phones.toml, or their mistakes.
fn settings_with(place: &Place, text: &str) -> Result<Config, String> {
	let files = place.files();
	let mut sources = settings::read_files(&files).map_err(|m| settings::join_mistakes(&m))?;
	let main = settings::main_in(&files, &sources);
	let name = path(place).display().to_string();
	match sources.iter_mut().find(|s| s.name == name) {
		Some(s) => s.text = text.to_string(),
		None => {
			// Among the drop-ins, by name, as it will be read.
			let from = usize::from(main);
			let at = sources.iter().skip(from).position(|s| s.name > name).map_or(sources.len(), |i| i + from);
			sources.insert(at, Source::new(name, text));
		}
	}
	settings::read_settings(&sources, main).map_err(|m| format!("the settings would have mistakes: {}", settings::join_mistakes(&m).replace('\n', "; ")))
}

/// Writes phones.toml as `text`, once the settings with it are sound: in one step, keeping its
/// permissions (only its owner's, for a new one).
fn write(place: &Place, text: &str) -> Result<(), String> {
	settings_with(place, text)?;
	let p = path(place);
	fs::create_dir_all(place.dropins()).map_err(|e| format!("can't make {}: {e}", store::shown(&place.dropins())))?;
	store::replace_as(&p, text.as_bytes(), store::mode_of(&p, 0o600)).map_err(|e| format!("can't write {}: {e}", store::shown(&p)))
}

/// The tables of phones.toml's profiles: each id, with its `name`.
fn tables(doc: &DocumentMut) -> Vec<(String, Option<String>)> {
	let Some(profiles) = doc.get("profiles").and_then(Item::as_table_like) else { return Vec::new() };
	profiles
		.iter()
		.filter_map(|(id, item)| {
			let t = item.as_table_like()?;
			Some((id.to_string(), t.get("name").and_then(Item::as_str).map(str::to_string)))
		})
		.collect()
}

/// The table `id` in phones.toml as written with a header, if it is one.
fn table_mut<'a>(doc: &'a mut DocumentMut, id: &str) -> Option<&'a mut Table> {
	doc.get_mut("profiles")?.as_table_like_mut()?.get_mut(id)?.as_table_mut()
}

/// The last position of a table in `item`, for one added after them all.
fn last_position(item: &Item) -> isize {
	match item {
		Item::Table(t) => t.position().unwrap_or(0).max(t.iter().map(|(_, i)| last_position(i)).max().unwrap_or(0)),
		Item::ArrayOfTables(a) => a.iter().map(|t| last_position(&Item::Table(t.clone()))).max().unwrap_or(0),
		_ => 0,
	}
}

/// A table's comment, as what is before it (the file's header, a paragraph of the owner's), up to
/// its last blank line, and its own lines, directly above it.
pub fn split_own(prefix: &str) -> (&str, &str) {
	let mut at = 0;
	let mut pos = 0;
	for line in prefix.split_inclusive('\n') {
		pos += line.len();
		if line.trim().is_empty() && line.ends_with('\n') {
			at = pos;
		}
	}
	prefix.split_at(at)
}

fn prefix_of(t: &Table) -> String {
	t.decor().prefix().and_then(|p| p.as_str()).unwrap_or("").to_string()
}

/// The line above a phone's table that says which phone has it.
fn paired_line(name: &str, client_id: &str, at: u64) -> String {
	format!("# {name}, paired {} ({client_id})\n", store::format_date(at))
}

/// Whether `line` is one the service wrote above a phone's table: which phone has it, or that it
/// was unpaired.
fn ours(line: &str) -> bool {
	let line = line.trim_end();
	let paired = line.starts_with("# ")
		&& line.ends_with(')')
		&& line.rsplit_once(" (").is_some_and(|(before, id)| {
			let id = &id[..id.len() - 1];
			!id.is_empty() && id.bytes().all(|b| b.is_ascii_hexdigit()) && before.rsplit_once(", paired ").is_some_and(|(_, date)| date.len() == 10)
		});
	paired || (line.starts_with("# Unpaired ") && line.contains("; kept for a phone paired again"))
}

/// Adds a table to phones.toml for the phone `client_id` named `name`, paired at `at`, which uses
/// the profile `uses` (`default_profile` for none), unless the settings don't have it; its id, or
/// why nothing was written.
pub fn add(place: &Place, name: &str, uses: Option<&str>, client_id: &str, at: u64) -> Result<String, String> {
	// It goes into a comment line and a string: nothing in it that doesn't show.
	let name = &store::sanitize_name(name);
	let text = read(place)?;
	let mut doc = parse(place, text.as_deref().unwrap_or(""))?;
	let now = settings_with(place, text.as_deref().unwrap_or(""))?;
	let mut taken: BTreeSet<String> = now.profiles.keys().cloned().collect();
	taken.extend(tables(&doc).into_iter().map(|(id, _)| id));
	let id = id_for(name, &taken);
	let uses = uses.unwrap_or(&now.default_profile);
	let mut t = Table::new();
	t.insert("name", toml_edit::value(name));
	if now.profiles.contains_key(uses) {
		t.insert("use", toml_edit::value(uses));
	}
	// The first table carries the file's header, or what is left of it.
	let first = tables(&doc).is_empty();
	let lead = if first {
		let trailing = doc.trailing().as_str().unwrap_or("").to_string();
		doc.set_trailing("");
		if trailing.trim().is_empty() { format!("{HEADER}\n") } else { format!("{}\n\n", trailing.trim_end()) }
	} else {
		"\n".to_string()
	};
	t.decor_mut().set_prefix(lead + &paired_line(name, client_id, at));
	t.set_position(Some(last_position(doc.as_item()) + 1));
	let profiles = doc.as_table_mut().entry("profiles").or_insert_with(|| {
		let mut p = Table::new();
		p.set_implicit(true);
		Item::Table(p)
	});
	let Some(profiles) = profiles.as_table_mut() else {
		return Err(format!("{} has its profiles in a form the service doesn't add to", store::shown(&path(place))));
	};
	profiles.insert(&id, Item::Table(t));
	write(place, &doc.to_string())?;
	Ok(id)
}

/// Whether phones.toml has a table `id` that none of `clients` has as its own.
pub fn unowned(place: &Place, id: &str, clients: &[Client]) -> bool {
	let Ok(Some(text)) = read(place) else { return false };
	let Ok(doc) = text.parse::<DocumentMut>() else { return false };
	tables(&doc).iter().any(|(t, _)| t == id) && !clients.iter().any(|c| c.own_profile.as_deref() == Some(id))
}

/// A table of phones.toml none of `clients` has as its own, of a phone named `name` (whatever the
/// case): the one a phone of that name paired last had.
pub fn unowned_named(place: &Place, name: &str, clients: &[Client]) -> Option<String> {
	let text = read(place).ok()??;
	let doc = text.parse::<DocumentMut>().ok()?;
	let name = name.to_lowercase();
	tables(&doc)
		.into_iter()
		.filter(|(id, n)| n.as_deref().is_some_and(|n| n.to_lowercase() == name) && !clients.iter().any(|c| c.own_profile.as_deref() == Some(id)))
		.map(|(id, _)| id)
		.next_back()
}

/// Says above the table `id` that the phone `client_id` named `name` has it from `at` on: the
/// lines the service wrote there before (which phone had it, that it was unpaired) give way to that
/// one. Nothing else is changed, and nothing written when nothing would change.
pub fn note_taken(place: &Place, id: &str, name: &str, client_id: &str, at: u64) -> Result<(), String> {
	let Some(text) = read(place)? else { return Ok(()) };
	let mut doc = parse(place, &text)?;
	let Some(t) = table_mut(&mut doc, id) else { return Ok(()) };
	let prefix = prefix_of(t);
	let (before, own) = split_own(&prefix);
	let kept: String = own.split_inclusive('\n').filter(|l| !ours(l)).collect();
	let new = format!("{before}{kept}{}", paired_line(name, client_id, at));
	if new == prefix {
		return Ok(());
	}
	t.decor_mut().set_prefix(new);
	write(place, &doc.to_string())
}

/// What becomes of the table of the phone `gone`, unpaired, the phones `remaining` still paired:
/// taken out when it is as pairing wrote it (`name` and `use` alone) and nothing else has it (no
/// profile uses it, it isn't `default_profile`, no phone has it as its profile); otherwise kept,
/// as it is, with a note for a phone paired again to take it. What was done, for the person and
/// the journal; none for a phone without a table of its own.
pub fn release(place: &Place, gone: &Client, remaining: &[Client], at: u64) -> Option<String> {
	let own = gone.own_profile.as_deref()?;
	let file = store::shown(&path(place));
	let stays = |why: &str| Some(format!("its profile {own} stays in {file}: {why}"));
	if let Some(c) = remaining.iter().find(|c| c.own_profile.as_deref() == Some(own)) {
		return stays(&format!("\"{}\" ({}) has it as its own", c.name, c.id));
	}
	let text = match read(place) {
		Ok(Some(t)) => t,
		Ok(None) => return Some(format!("its profile {own} isn't in {file}; nothing to take out")),
		Err(why) => return stays(&why),
	};
	let mut doc = match parse(place, &text) {
		Ok(d) => d,
		Err(why) => return stays(&why),
	};
	let settings = match settings_with(place, &text) {
		Ok(s) => s,
		Err(_) => return stays("the settings have mistakes now (elamond check names them), so it isn't touched"),
	};
	let Some(table) = doc.get("profiles").and_then(Item::as_table_like).and_then(|p| p.get(own)).and_then(Item::as_table_like) else {
		return Some(format!("its profile {own} isn't in {file}; nothing to take out"));
	};
	let name = table.get("name").and_then(Item::as_str).map(str::to_string);
	let as_paired = name.as_deref() == Some(gone.name.as_str()) && table.iter().all(|(k, _)| k == "name" || k == "use");
	let mut why = Vec::new();
	if !as_paired {
		why.push("it was changed since pairing".to_string());
	}
	let users: Vec<&str> = settings.profiles.iter().filter(|(id, p)| id.as_str() != own && p.base.as_deref() == Some(own)).map(|(id, _)| id.as_str()).collect();
	if !users.is_empty() {
		why.push(format!("{} {} it", users.join(", "), if users.len() == 1 { "uses" } else { "use" }));
	}
	if settings.default_profile == own {
		why.push("it is default_profile".into());
	}
	for c in remaining.iter().filter(|c| c.profile.as_deref() == Some(own)) {
		why.push(format!("\"{}\" ({}) has it as its profile", c.name, c.id));
	}
	if why.is_empty() {
		return Some(match take_out(&mut doc, own).and_then(|()| write(place, &doc.to_string())) {
			Ok(()) => format!("its profile {own} is taken out of {file}, as pairing wrote it and nothing else has it"),
			Err(e) => format!("its profile {own} stays in {file}: {e}"),
		});
	}
	let shown = name.unwrap_or_else(|| gone.name.clone());
	let note = format!("# Unpaired {}; kept for a phone paired again (elamond pair offers it to one named \"{shown}\").\n", store::format_date(at));
	if let Some(t) = table_mut(&mut doc, own) {
		let prefix = prefix_of(t);
		let (before, mine) = split_own(&prefix);
		let kept: String = mine.split_inclusive('\n').filter(|l| !(l.starts_with("# Unpaired ") && ours(l))).collect();
		t.decor_mut().set_prefix(format!("{before}{kept}{note}"));
		if let Err(e) = write(place, &doc.to_string()) {
			return stays(&format!("{}; no note written: {e}", why.join("; ")));
		}
	}
	stays(&format!("{}; kept for a phone paired again", why.join("; ")))
}

/// Takes the table `id` out of the phones' profiles, with its own comment lines; what was before
/// them (the file's header) stays, above the next table or at the end.
fn take_out(doc: &mut DocumentMut, id: &str) -> Result<(), String> {
	let Some(profiles) = doc.get_mut("profiles").and_then(Item::as_table_like_mut) else { return Ok(()) };
	let Some(item) = profiles.remove(id) else { return Ok(()) };
	let prefix = item.as_table().map(prefix_of).unwrap_or_default();
	let lead = split_own(&prefix).0.to_string();
	if !lead.contains('#') {
		return Ok(());
	}
	// The table that came next, in the file's order.
	let position = item.as_table().and_then(Table::position).unwrap_or(isize::MIN);
	let next = doc
		.get_mut("profiles")
		.and_then(Item::as_table_mut)
		.and_then(|p| p.iter_mut().filter_map(|(_, i)| i.as_table_mut()).filter(|t| t.position().is_some_and(|p| p > position)).min_by_key(|t| t.position()));
	match next {
		Some(t) => {
			let theirs = prefix_of(t);
			t.decor_mut().set_prefix(format!("{lead}{}", theirs.trim_start_matches('\n')));
		}
		None => {
			let trailing = doc.trailing().as_str().unwrap_or("").to_string();
			doc.set_trailing(format!("{lead}{trailing}"));
		}
	}
	Ok(())
}

/// What pairing gives a new phone named `name`, of id `id`, at `at`, with the phones `remaining`
/// paired: the profile of the entry it `replaced` (the same phone, paired again), or, with `asked`,
/// the profile `elamond pair --profile` names; a table `offered` by its name that the person at
/// `elamond pair` gave it; otherwise a table of its own. With what was done, for the journal.
#[allow(clippy::too_many_arguments)]
pub fn give(
	place: &Place,
	remaining: &[Client],
	replaced: Option<&Client>,
	offered: Option<&str>,
	asked: Option<&str>,
	name: &str,
	id: &str,
	at: u64,
) -> (Profiles, Vec<String>) {
	let mut notes = Vec::new();
	let profiles = 'given: {
		let take = |table: &str, notes: &mut Vec<String>| {
			if let Err(e) = note_taken(place, table, name, id, at) {
				notes.push(format!("its table {table} is its own, though the line above it isn't changed: {e}"));
			}
			Profiles { profile: Some(table.to_string()), own: Some(table.to_string()) }
		};
		if let (Some(old), None) = (replaced, asked) {
			let has = |p: &str| place.load().is_ok_and(|c| c.profiles.contains_key(p));
			if let Some(own) = old.own_profile.as_deref().filter(|own| has(own)) {
				let mut given = take(own, &mut notes);
				given.profile.clone_from(&old.profile);
				break 'given given;
			}
			// No table of its own to keep: one made, using the profile it had.
			let had = old.profile.as_deref().filter(|p| old.own_profile.as_deref() != Some(*p));
			break 'given new_table(place, name, had, id, at, &mut notes);
		}
		if let Some(x) = asked {
			if unowned(place, x, remaining) {
				break 'given take(x, &mut notes);
			}
			break 'given new_table(place, name, Some(x), id, at, &mut notes);
		}
		if let Some(t) = offered.filter(|t| unowned(place, t, remaining)) {
			break 'given take(t, &mut notes);
		}
		new_table(place, name, None, id, at, &mut notes)
	};
	// The table of the entry replaced, when the new one doesn't keep it, as revoking would leave it.
	if let Some(old) = replaced.filter(|o| o.own_profile.is_some() && o.own_profile != profiles.own)
		&& let Some(n) = release(place, old, remaining, at)
	{
		notes.push(format!("of \"{}\" ({}), unpaired: {n}", old.name, old.id));
	}
	(profiles, notes)
}

/// What a phone named `name`, of id `id`, gets as a key of its own at `at` (`rekey`): a table of
/// its own using the profile of `asking`, the entry whose token asked.
pub fn give_rekey(place: &Place, asking: &Client, name: &str, id: &str, at: u64) -> (Profiles, Vec<String>) {
	let mut notes = Vec::new();
	let profiles = new_table(place, name, asking.profile.as_deref(), id, at, &mut notes);
	(profiles, notes)
}

/// A table of its own for a new phone, using `uses` (`default_profile` for none); when phones.toml
/// can't take it, the phone has that profile, as with 0.6.0, and `notes` say why.
fn new_table(place: &Place, name: &str, uses: Option<&str>, id: &str, at: u64, notes: &mut Vec<String>) -> Profiles {
	match add(place, name, uses, id, at) {
		Ok(table) => Profiles { profile: Some(table.clone()), own: Some(table) },
		Err(e) => {
			notes.push(format!("no profile of its own: {e}"));
			Profiles { profile: uses.map(str::to_string), own: None }
		}
	}
}

/// Gives each phone paired before 0.6.1, which has no table of its own, one in phones.toml, using
/// the profile it had (`default_profile` when it had none, or one gone); that table is its own and
/// its profile from then on, so this happens once. Under the paired phones' lock in the data
/// directory `data`; the journal names each. Whether any was given one.
pub fn adopt(place: &Place, data: &Path) -> bool {
	let _lock = match store::lock_clients(data) {
		Ok(l) => l,
		Err(e) => {
			eprintln!("phones: can't lock the paired phones ({e}); those paired before 0.6.1 get a profile of their own at a later start");
			return false;
		}
	};
	let mut clients = match store::load_clients_in(data) {
		Ok(c) => c,
		Err(e) => {
			eprintln!("phones: {e}; the phones paired before 0.6.1 get a profile of their own once it reads");
			return false;
		}
	};
	if clients.iter().all(|c| c.own_profile.is_some()) {
		return false;
	}
	let Ok(config) = place.load() else {
		eprintln!("phones: the settings have mistakes; the phones paired before 0.6.1 get a profile of their own once they are mended");
		return false;
	};
	let mut changed = false;
	for c in clients.iter_mut().filter(|c| c.own_profile.is_none()) {
		let had = c.profile.as_deref().filter(|p| config.profiles.contains_key(*p));
		match add(place, &c.name, had, &c.id, c.created) {
			Ok(table) => {
				let uses = had.unwrap_or(&config.default_profile);
				eprintln!("phones: \"{}\" ({}) has a profile of its own now, {table} in {}, using {uses}", c.name, c.id, store::shown(&path(place)));
				c.own_profile = Some(table.clone());
				c.profile = Some(table);
				changed = true;
			}
			Err(e) => {
				eprintln!("phones: the phones paired before 0.6.1 get no profile of their own for now: {e}");
				break;
			}
		}
	}
	if changed && let Err(e) = store::save_clients_in(data, &clients) {
		eprintln!("phones: can't write the paired phones ({e}); the tables made in phones.toml stay, for elamond pair --profile");
		return false;
	}
	changed
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::access::Role;

	fn scratch() -> Place {
		let dir = std::env::temp_dir().join(format!("elamond-phones-{}", data_encoding::HEXLOWER.encode(&store::random_bytes::<6>())));
		fs::create_dir_all(&dir).unwrap();
		Place { dir, named: None }
	}

	fn settings(place: &Place, text: &str) {
		fs::write(place.main(), format!("schema = 3\n{text}")).unwrap();
	}

	const OWNER: &str = "[profiles.default]\nname = \"Default\"\n[profiles.owner]\nuse = \"default\"\ntheme = \"dark\"\n";

	fn phones(place: &Place) -> String {
		fs::read_to_string(path(place)).unwrap_or_default()
	}

	fn client(id: &str, name: &str, own: Option<&str>, profile: Option<&str>) -> Client {
		Client {
			id: id.into(),
			name: name.into(),
			own_profile: own.map(Into::into),
			profile: profile.map(Into::into),
			role: Role::Admin,
			..Default::default()
		}
	}

	/// 2026-11-12, noon.
	const AT: u64 = 1_794_484_800;

	#[test]
	fn ids_from_names() {
		let none = BTreeSet::new();
		assert_eq!(id_for("Galaxy S25", &none), "galaxy-s25");
		assert_eq!(id_for("  Pixel 9 (work)  ", &none), "pixel-9-work");
		assert_eq!(id_for("Телефон", &none), "phone");
		assert_eq!(id_for("Mi Телефон 2", &none), "mi-2");
		assert_eq!(id_for("Phone", &none), "phone");
		let long = id_for(&"Ab".repeat(40), &none);
		assert_eq!(long.len(), 32);
		let taken: BTreeSet<String> = ["galaxy-s25".to_string(), "galaxy-s25-2".into(), long.clone()].into();
		assert_eq!(id_for("Galaxy S25", &taken), "galaxy-s25-3");
		let again = id_for(&"Ab".repeat(40), &taken);
		assert!(again.len() <= 32 && again.ends_with("-2") && settings::is_id(&again), "{again}");
	}

	#[test]
	fn pairing_adds_a_table_and_the_name_repeats() {
		let place = scratch();
		settings(&place, OWNER);
		let a = add(&place, "Galaxy S25", Some("owner"), "0123abcd", AT).unwrap();
		let b = add(&place, "Pixel 9", None, "4567cdef", AT).unwrap();
		let c = add(&place, "galaxy s25", Some("gone"), "89ab0123", AT).unwrap();
		assert_eq!((a.as_str(), b.as_str(), c.as_str()), ("galaxy-s25", "pixel-9", "galaxy-s25-2"));
		assert_eq!(
			phones(&place),
			"# Profiles of the paired phones, one each: elamond pair adds them (docs/config.md#phones).\n\
			 # Change how a phone looks here, key by key; `use` is the profile it starts from.\n\
			 \n\
			 # Galaxy S25, paired 2026-11-12 (0123abcd)\n\
			 [profiles.galaxy-s25]\nname = \"Galaxy S25\"\nuse = \"owner\"\n\
			 \n\
			 # Pixel 9, paired 2026-11-12 (4567cdef)\n\
			 [profiles.pixel-9]\nname = \"Pixel 9\"\nuse = \"default\"\n\
			 \n\
			 # galaxy s25, paired 2026-11-12 (89ab0123)\n\
			 [profiles.galaxy-s25-2]\nname = \"galaxy s25\"\n"
		);
		use std::os::unix::fs::PermissionsExt;
		assert_eq!(fs::metadata(path(&place)).unwrap().permissions().mode() & 0o777, 0o600);
		// The settings read it: the phone's table starts from owner.
		let config = place.load().unwrap();
		assert_eq!(config.profile("galaxy-s25").theme, "dark");
		assert_eq!(config.profile_sources["galaxy-s25"].file, "elamond.d/phones.toml");
		// An id the settings have elsewhere is taken too.
		assert_eq!(add(&place, "Owner", None, "cdef0123", AT).unwrap(), "owner-2");
		fs::remove_dir_all(&place.dir).unwrap();
	}

	#[test]
	fn a_link_or_settings_that_would_break_are_left_alone() {
		let place = scratch();
		settings(&place, OWNER);
		// A link there: never written through.
		fs::create_dir_all(place.dropins()).unwrap();
		let elsewhere = place.dir.join("elsewhere.toml");
		fs::write(&elsewhere, "# mine\n").unwrap();
		std::os::unix::fs::symlink(&elsewhere, path(&place)).unwrap();
		let e = add(&place, "Galaxy S25", None, "0123abcd", AT).unwrap_err();
		assert!(e.contains("is a link"), "{e}");
		assert_eq!(fs::read_to_string(&elsewhere).unwrap(), "# mine\n");
		fs::remove_file(path(&place)).unwrap();
		// Settings with a mistake: a table wouldn't make them parse.
		fs::write(place.dropins().join("broken.toml"), "[dashboards.x]\nicon = \"overview\"\n").unwrap();
		let e = add(&place, "Galaxy S25", None, "0123abcd", AT).unwrap_err();
		assert!(e.contains("mistakes"), "{e}");
		assert!(!path(&place).exists());
		// Pairing goes on without a table, with the profile it was to use.
		let (given, notes) = give(&place, &[], None, None, Some("owner"), "Galaxy S25", "0123abcd", AT);
		assert_eq!(given, Profiles { profile: Some("owner".into()), own: None });
		assert!(notes[0].starts_with("no profile of its own: the settings would have mistakes"), "{notes:?}");
		fs::remove_dir_all(&place.dir).unwrap();
	}

	#[test]
	fn the_same_phone_paired_again_keeps_its_table() {
		let place = scratch();
		settings(&place, OWNER);
		let own = add(&place, "Galaxy S25", Some("owner"), "0123abcd", AT).unwrap();
		let old = client("0123abcd", "Galaxy S25", Some(&own), Some("owner"));
		let (given, notes) = give(&place, &[], Some(&old), None, None, "Galaxy S25", "89ab0123", AT + 86_400);
		assert_eq!(given, Profiles { profile: Some("owner".into()), own: Some(own.clone()) });
		assert!(notes.is_empty(), "{notes:?}");
		// Its line says which phone has it now.
		let text = phones(&place);
		assert!(text.contains("# Galaxy S25, paired 2026-11-13 (89ab0123)\n[profiles.galaxy-s25]") && !text.contains("0123abcd"), "{text}");
		// With --profile: that profile decides, and the old table goes as revoking would take it.
		let again = client("89ab0123", "Galaxy S25", Some(&own), Some(&own));
		let (given, notes) = give(&place, &[], Some(&again), None, Some("default"), "Galaxy S25", "cdef4567", AT);
		assert_eq!(given, Profiles { profile: Some("galaxy-s25-2".into()), own: Some("galaxy-s25-2".into()) });
		assert!(notes[0].contains("galaxy-s25 is taken out"), "{notes:?}");
		let text = phones(&place);
		assert!(!text.contains("[profiles.galaxy-s25]\n") && text.contains("[profiles.galaxy-s25-2]\nname = \"Galaxy S25\"\nuse = \"default\"\n"), "{text}");
		// The file's header stays, above the table that is left.
		assert!(text.starts_with(HEADER), "{text}");
		fs::remove_dir_all(&place.dir).unwrap();
	}

	#[test]
	fn unpairing_takes_an_untouched_table_out_and_keeps_one_changed_or_used() {
		let place = scratch();
		settings(&place, OWNER);
		let a = add(&place, "Galaxy S25", Some("owner"), "0123abcd", AT).unwrap();
		let b = add(&place, "Pixel 9", None, "4567cdef", AT).unwrap();
		let c = add(&place, "Tab", None, "89ab0123", AT).unwrap();
		let d = add(&place, "Watch", None, "cdef4567", AT).unwrap();
		// b changed by hand, c used by another profile, d another phone's profile.
		let text = phones(&place).replace("name = \"Pixel 9\"\n", "name = \"Pixel 9\"\ntheme = \"light\"\n");
		fs::write(path(&place), text + "\n[profiles.kids]\nuse = \"tab\"\n").unwrap();
		let watch = client("cdef4567", "Watch", Some(&d), Some(&d));
		let other = client("01234567", "Other", None, Some(&d));
		let remaining = [other.clone()];
		let gone = |id: &str, name: &str, own: &str| client(id, name, Some(own), Some(own));

		let said = release(&place, &gone("0123abcd", "Galaxy S25", &a), &remaining, AT).unwrap();
		assert!(said.contains("galaxy-s25 is taken out"), "{said}");
		let said = release(&place, &gone("4567cdef", "Pixel 9", &b), &remaining, AT).unwrap();
		assert!(said.contains("stays") && said.contains("changed since pairing"), "{said}");
		let said = release(&place, &gone("89ab0123", "Tab", &c), &remaining, AT).unwrap();
		assert!(said.contains("kids uses it"), "{said}");
		let said = release(&place, &watch, &remaining, AT).unwrap();
		assert!(said.contains("\"Other\" (01234567) has it as its profile"), "{said}");

		let text = phones(&place);
		assert!(text.starts_with(HEADER) && !text.contains("galaxy-s25"), "{text}");
		assert!(
			text.contains(
				"# Pixel 9, paired 2026-11-12 (4567cdef)\n# Unpaired 2026-11-12; kept for a phone paired again (elamond pair offers it to one named \"Pixel 9\").\n[profiles.pixel-9]"
			),
			"{text}"
		);
		// Kept live: the settings still read them, and a phone of the name is offered one.
		let config = place.load().unwrap();
		assert!(config.profiles.contains_key("pixel-9") && config.profiles.contains_key("tab"));
		assert_eq!(unowned_named(&place, "PIXEL 9", &remaining).as_deref(), Some("pixel-9"));
		assert_eq!(unowned_named(&place, "Watch", &[watch]), None);
		// Given to it, the note gives way to the line of the phone that has it now.
		let (given, _) = give(&place, &remaining, None, Some("pixel-9"), None, "Pixel 9", "aaaa0000", AT);
		assert_eq!(given.own.as_deref(), Some("pixel-9"));
		let text = phones(&place);
		assert!(text.contains("\n# Pixel 9, paired 2026-11-12 (aaaa0000)\n[profiles.pixel-9]") && !text.contains("offers it to one named \"Pixel 9\""), "{text}");
		// A table offered that someone took meanwhile isn't given twice.
		let (given, _) = give(&place, &[client("aaaa0000", "Pixel 9", Some("pixel-9"), None)], None, Some("pixel-9"), None, "Pixel 9", "bbbb0000", AT);
		assert_eq!(given.own.as_deref(), Some("pixel-9-2"));
		fs::remove_dir_all(&place.dir).unwrap();
	}

	#[test]
	fn a_table_named_in_pair_profile_that_no_phone_has_is_taken_as_it_is() {
		let place = scratch();
		settings(&place, OWNER);
		let a = add(&place, "Galaxy S25", Some("owner"), "0123abcd", AT).unwrap();
		let (given, _) = give(&place, &[], None, None, Some(&a), "New phone", "4567cdef", AT);
		assert_eq!(given.own.as_deref(), Some("galaxy-s25"));
		// One a phone has: a new table using it.
		let has = [client("4567cdef", "New phone", Some(&a), Some(&a))];
		let (given, _) = give(&place, &has, None, None, Some(&a), "Third", "89ab0123", AT);
		assert_eq!(given.own.as_deref(), Some("third"));
		assert_eq!(place.load().unwrap().profiles["third"].base.as_deref(), Some("galaxy-s25"));
		fs::remove_dir_all(&place.dir).unwrap();
	}

	#[test]
	fn phones_paired_before_get_a_table_once() {
		let place = scratch();
		settings(&place, OWNER);
		let data = place.dir.join("data");
		fs::create_dir_all(&data).unwrap();
		let mut owner = client("0123abcd", "Galaxy S25", None, Some("owner"));
		owner.created = AT;
		let list = [owner, client("4567cdef", "Pixel 9", None, None), client("89ab0123", "Old tab", None, Some("gone"))];
		store::save_clients_in(&data, &list).unwrap();
		assert!(adopt(&place, &data));
		let list = store::load_clients_in(&data).unwrap();
		let got: Vec<(Option<&str>, Option<&str>)> = list.iter().map(|c| (c.own_profile.as_deref(), c.profile.as_deref())).collect();
		assert_eq!(got, [(Some("galaxy-s25"), Some("galaxy-s25")), (Some("pixel-9"), Some("pixel-9")), (Some("old-tab"), Some("old-tab"))]);
		let config = place.load().unwrap();
		let uses: Vec<Option<&str>> = ["galaxy-s25", "pixel-9", "old-tab"].iter().map(|p| config.profiles[*p].base.as_deref()).collect();
		assert_eq!(uses, [Some("owner"), Some("default"), Some("default")]);
		assert!(phones(&place).contains("# Galaxy S25, paired 2026-11-12 (0123abcd)\n"));
		// Once.
		let before = phones(&place);
		assert!(!adopt(&place, &data));
		assert_eq!(phones(&place), before);
		fs::remove_dir_all(&place.dir).unwrap();
	}
}
