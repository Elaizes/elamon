//! `elamond values <source>`: the source's values as the service names them, asked of upsd there and
//! then, with the station's commands and writable settings and what the source allows of them.

use std::time::SystemTime;

use upst_core::nut::{self, CmdInfo, VarInfo};
use upst_core::target::Server;

use super::{allows, Station};
use crate::consts::timeout;
use crate::settings::{Config, NutSource};
use crate::values;

/// What `elamond values <id>` prints.
pub fn values(config: &Config, id: &str) -> Result<String, String> {
	let Some(src) = config.sources.get(id) else {
		let there = if config.sources.is_empty() {
			"the settings have none".to_string()
		} else {
			format!("the settings have {}", config.sources.keys().map(String::as_str).collect::<Vec<_>>().join(", "))
		};
		return Err(format!("no source \"{id}\" ({there}); `elamond values` lists this computer's own"));
	};
	listing(id, src)
}

/// What the station has: its variables, its commands and its writable variables.
struct Asked {
	vars: Vec<(String, String)>,
	cmds: Vec<CmdInfo>,
	rw: Vec<(String, VarInfo)>,
}

/// Asks upsd, each answer within `timeout::NUT`.
fn ask(src: &NutSource) -> Result<Asked, String> {
	let server = Server { host: src.host.clone(), port: src.port };
	let mut c = nut::Client::connect(&src.host, src.port, timeout::NUT).map_err(|e| format!("can't reach upsd at {server}: {e}"))?;
	let vars = c.list_vars(&src.ups).map_err(|e| format!("upsd at {server} has nothing for {}: {e}", src.ups))?;
	// A station without commands or writable variables may refuse to list them.
	let names = c.list_commands(&src.ups).unwrap_or_default();
	let mut cmds = Vec::new();
	for name in names {
		let desc = c.command_description(&src.ups, &name).ok().flatten();
		cmds.push(CmdInfo { name, desc });
	}
	let mut rw = Vec::new();
	for (var, _) in c.list_rw(&src.ups).unwrap_or_default() {
		let info = c.var_info(&src.ups, &var).unwrap_or_default();
		rw.push((var, info));
	}
	c.logout();
	Ok(Asked { vars, cmds, rw })
}

/// Rows of cells, each column as wide as its widest, two spaces between.
fn table(rows: &[Vec<String>], indent: &str) -> String {
	let n = rows.iter().map(Vec::len).max().unwrap_or(0);
	let widths: Vec<usize> = (0..n).map(|i| rows.iter().filter_map(|r| r.get(i)).map(|c| c.chars().count()).max().unwrap_or(0)).collect();
	let mut out = String::new();
	for r in rows {
		let cells: Vec<String> = r.iter().enumerate().map(|(i, c)| if i + 1 == r.len() { c.clone() } else { format!("{c:w$}", w = widths[i]) }).collect();
		out.push_str(indent);
		out.push_str(cells.join("  ").trim_end());
		out.push('\n');
	}
	out
}

/// The listing of source `id`, asked of its upsd.
pub fn listing(id: &str, src: &NutSource) -> Result<String, String> {
	let asked = ask(src)?;
	let mut station = Station::new(id, src);
	let now = SystemTime::now();
	station.polled(asked.vars, now);
	let snapshot = station.snapshot(now);
	let mut rows = vec![["address", "value", "unit", "format", "role"].map(String::from).to_vec()];
	for (address, s) in &snapshot.scalars {
		let shown = crate::format::format(s.v.shown(), s.format, s.unit);
		let shown = if shown.chars().count() > 40 { format!("{}…", shown.chars().take(39).collect::<String>()) } else { shown };
		rows.push(vec![address.clone(), shown, s.unit.to_string(), s.format.to_string(), values::address_role(address).as_str().to_string()]);
	}
	let mut out = table(&rows, "");
	out.push('\n');
	let allowed = |yes: bool, key: &str| if yes { "allowed".to_string() } else { format!("not in {key}") };
	if asked.cmds.is_empty() {
		out.push_str("Commands on this station: none\n");
	} else {
		out.push_str("Commands on this station:\n");
		let rows: Vec<Vec<String>> =
			asked.cmds.iter().map(|c| vec![c.name.clone(), c.desc.clone().unwrap_or_default(), allowed(allows(&src.commands, &c.name), "commands")]).collect();
		out.push_str(&table(&rows, "  "));
	}
	if asked.rw.is_empty() {
		out.push_str("Writable settings: none\n");
	} else {
		out.push_str("Writable settings:\n");
		let rows: Vec<Vec<String>> = asked
			.rw
			.iter()
			.map(|(var, info)| {
				let takes = if !info.enum_values.is_empty() {
					format!("ENUM {}", info.enum_values.join(", "))
				} else if !info.ranges.is_empty() {
					let r: Vec<String> = info.ranges.iter().map(|(lo, hi)| format!("{lo}–{hi}")).collect();
					format!("RANGE {}", r.join(", "))
				} else {
					info.kind.join(" ")
				};
				vec![var.clone(), takes, allowed(allows(&src.settings, var), "settings")]
			})
			.collect();
		out.push_str(&table(&rows, "  "));
	}
	out.push_str("\nHow long it has been so, the estimates and the timeline come from the running service.\n");
	Ok(out)
}
