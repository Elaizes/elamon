//! `elamond values <source>` for a `command`, `http` or `file` source: read there and then, within
//! its `timeout`, and its values listed as the service names them; or why it couldn't be read.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::SystemTime;

use super::{put, read, Keeper};
use crate::settings::{Config, Feed};
use crate::values::Values;

/// What `elamond values <id>` prints for the source `id` of `config`, read now.
pub fn listing(config: &Config, id: &str, feed: &Feed) -> Result<String, String> {
	let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
	let outcome = rt.block_on(read(feed, &mut None));
	let at = SystemTime::now();
	let mut keeper = Keeper::new(id);
	// What the journal would say is the service's to say.
	let _ = keeper.took(outcome, at);
	if let Some(e) = &keeper.snap.error {
		return Err(format!("source {id}: {e}"));
	}
	let mut values = Values { t: at.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64), ..Default::default() };
	let only = Config { feeds: BTreeMap::from([(id.to_string(), feed.clone())]), ..config.clone() };
	put(&mut values, &only, &BTreeMap::from([(id.to_string(), Arc::new(keeper.snap))]));
	let mut rows = vec![["address", "value", "unit", "format", "role"].map(String::from).to_vec()];
	for (address, s) in &values.scalars {
		let shown = crate::format::format(s.v.shown(), s.format, s.unit);
		let shown = if shown.chars().count() > 40 { format!("{}…", shown.chars().take(39).collect::<String>()) } else { shown };
		let role = values.def(address).map_or(feed.role, |d| d.role);
		rows.push(vec![address.clone(), shown, s.unit.to_string(), s.format.to_string(), role.as_str().to_string()]);
	}
	Ok(crate::nut::cli::table(&rows, ""))
}
