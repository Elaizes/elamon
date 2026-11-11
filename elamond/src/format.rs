//! How numbers are written, the service's way and the clients' (docs/config.md, Formats): the
//! named formats and `{:.N}` patterns, and strings that hold values (`"{mem.used:bytes} of
//! {mem.total:bytes}"`). elamond/fixtures/formats.json holds the cases both sides are tested
//! against.

/// The named formats.
pub const FORMATS: [&str; 15] =
	["number", "integer", "percent", "bytes", "rate", "temp", "mhz", "watts", "wh", "rpm", "load", "duration", "time", "bool", "text"];

/// Units a number is written against directly, without a space.
const TIGHT: [&str; 3] = ["%", "°C", "°F"];

/// What is written: a number, a text or a yes or no.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shown<'a> {
	Num(f64),
	Text(&'a str),
	Bool(bool),
}

/// Whether `f` is a format: a named one, or a pattern like `{:.2}`.
pub fn is_format(f: &str) -> bool {
	FORMATS.contains(&f) || pattern_decimals(f).is_some()
}

/// The decimals of a `{:.N}` pattern (N up to 6).
fn pattern_decimals(f: &str) -> Option<usize> {
	let n = f.strip_prefix("{:.")?.strip_suffix('}')?;
	(n.len() == 1).then(|| n.parse::<usize>().ok()).flatten().filter(|n| *n <= 6)
}

/// `x` with `decimals` decimals, halves rounded away from zero. The decimal digits rounded are
/// those of the shortest text that reads back as `x` (what people see it as: 2.675 rounds to
/// 2.68, though the float is a little under it), so every client rounds alike.
pub fn fixed(x: f64, decimals: usize) -> String {
	if !x.is_finite() {
		return "—".into();
	}
	let text = format!("{}", x.abs());
	let (whole, frac) = text.split_once('.').unwrap_or((&text, ""));
	let mut digits: Vec<u8> = whole.bytes().chain(frac.bytes().chain(std::iter::repeat(b'0')).take(decimals)).collect();
	if frac.as_bytes().get(decimals).is_some_and(|d| *d >= b'5') {
		let mut i = digits.len();
		loop {
			if i == 0 {
				digits.insert(0, b'1');
				break;
			}
			i -= 1;
			if digits[i] == b'9' {
				digits[i] = b'0';
			} else {
				digits[i] += 1;
				break;
			}
		}
	}
	let split = digits.len() - decimals;
	let mut out = String::from_utf8_lossy(&digits[..split]).into_owned();
	if decimals > 0 {
		out.push('.');
		out.push_str(&String::from_utf8_lossy(&digits[split..]));
	}
	if x < 0.0 && digits.iter().any(|d| *d != b'0') {
		out.insert(0, '-');
	}
	out
}

/// `x` rounded to `decimals`, as `fixed` rounds it.
pub fn round(x: f64, decimals: usize) -> f64 {
	if !x.is_finite() {
		return x;
	}
	fixed(x, decimals).parse().unwrap_or(x)
}

/// A number and its unit: after a space, or directly for `%`, `°C` and `°F`.
fn with_unit(number: String, unit: &str) -> String {
	match unit {
		"" => number,
		u if TIGHT.contains(&u) => number + u,
		u => format!("{number} {u}"),
	}
}

/// Two decimals under 10, one under 100, none above; the rounded figure decides.
fn scaled_decimals(v: f64) -> String {
	let two = fixed(v, 2);
	if two.parse::<f64>().unwrap_or(v) < 10.0 {
		return two;
	}
	let one = fixed(v, 1);
	if one.parse::<f64>().unwrap_or(v) < 100.0 { one } else { fixed(v, 0) }
}

/// `x` in the largest of `units` that keeps it at least 1, each `base` times the one before; the
/// first unit whole.
fn scaled(x: f64, base: f64, units: &[&str]) -> String {
	let negative = x < 0.0;
	let mut v = x.abs();
	let mut i = 0;
	while v >= base && i + 1 < units.len() {
		v /= base;
		i += 1;
	}
	loop {
		let text = if i == 0 { fixed(v, 0) } else { scaled_decimals(v) };
		if text.parse::<f64>().unwrap_or(v) >= base && i + 1 < units.len() {
			v /= base;
			i += 1;
			continue;
		}
		let sign = if negative && text.bytes().any(|b| b.is_ascii_digit() && b != b'0') { "-" } else { "" };
		return format!("{sign}{text} {}", units[i]);
	}
}

const BYTES: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
const BITS: [&str; 5] = ["bit", "kbit", "Mbit", "Gbit", "Tbit"];

/// A value as `format` writes it, with `unit` (the value's own, or the widget's).
pub fn format(v: Shown, format: &str, unit: &str) -> String {
	let x = match v {
		Shown::Text(t) => return t.to_string(),
		Shown::Bool(b) => return if b { "yes" } else { "no" }.into(),
		Shown::Num(x) => x,
	};
	if let Some(d) = pattern_decimals(format) {
		return with_unit(fixed(x, d), unit);
	}
	match format {
		"integer" => with_unit(fixed(x, 0), unit),
		"percent" => fixed(x, 0) + "%",
		"bytes" => scaled(x, 1024.0, &BYTES),
		"rate" if unit == "bit/s" => scaled(x, 1000.0, &BITS) + "/s",
		"rate" => scaled(x, 1024.0, &BYTES) + "/s",
		"temp" => fixed(x, 0) + if unit.is_empty() { "°C" } else { unit },
		"mhz" => {
			let mhz = fixed(x, 0);
			if mhz.parse::<f64>().unwrap_or(x).abs() < 1000.0 { mhz + " MHz" } else { scaled_decimals(x / 1000.0) + " GHz" }
		}
		"watts" => fixed(x, 1) + " W",
		"wh" => fixed(x, 1) + " Wh",
		"rpm" => fixed(x, 0) + " RPM",
		"load" => fixed(x, 2),
		"duration" => duration(x),
		"time" => time(if unit == "ms" { x / 1000.0 } else { x }),
		"bool" => if x != 0.0 { "yes" } else { "no" }.into(),
		"text" => with_unit(format!("{x}"), unit),
		// "number", and anything else: up to one decimal.
		_ => {
			let one = fixed(x, 1);
			with_unit(one.strip_suffix(".0").map(str::to_string).unwrap_or(one), unit)
		}
	}
}

/// "3 d 4 h", "4 h 12 min", "12 min": whole units, as the 0.5 app wrote them; a minute at the least
/// for anything above nothing.
fn duration(secs: f64) -> String {
	let s = if secs.is_finite() && secs > 0.0 { secs as u64 } else { 0 };
	let (d, h, m) = (s / 86_400, s % 86_400 / 3600, s % 3600 / 60);
	if d > 0 {
		format!("{d} d {h} h")
	} else if h > 0 {
		format!("{h} h {m} min")
	} else {
		format!("{} min", if s > 0 { m.max(1) } else { 0 })
	}
}

/// A moment in the computer's own time: "2026-11-12 14:05".
fn time(secs: f64) -> String {
	if !secs.is_finite() || secs < 0.0 {
		return "—".into();
	}
	let t = secs as libc::time_t;
	let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
	// SAFETY: localtime_r fills the tm it is given from a plain time value, or returns null.
	let ok = !unsafe { libc::localtime_r(&t, tm.as_mut_ptr()) }.is_null();
	if !ok {
		return "—".into();
	}
	// SAFETY: filled by localtime_r above.
	let tm = unsafe { tm.assume_init() };
	format!("{:04}-{:02}-{:02} {:02}:{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min)
}

// ---- strings that hold values -------------------------------------------------------------

/// A `{…}` in a string: where it is, and what is between the braces (braces inside included, as in
/// `{gpu.{item}.summary}` before `{item}` is filled in).
#[derive(Debug, PartialEq)]
pub struct Placeholder<'a> {
	pub start: usize,
	pub end: usize,
	pub inner: &'a str,
}

/// The `{…}` in `text`, outermost only. A `{` that never closes is text.
pub fn placeholders(text: &str) -> Vec<Placeholder<'_>> {
	let bytes = text.as_bytes();
	let mut out = Vec::new();
	let mut i = 0;
	while i < bytes.len() {
		if bytes[i] != b'{' {
			i += 1;
			continue;
		}
		let mut depth = 0;
		let mut end = None;
		for (j, b) in bytes.iter().enumerate().skip(i) {
			match b {
				b'{' => depth += 1,
				b'}' => {
					depth -= 1;
					if depth == 0 {
						end = Some(j);
						break;
					}
				}
				_ => {}
			}
		}
		match end {
			Some(j) => {
				out.push(Placeholder { start: i, end: j + 1, inner: &text[i + 1..j] });
				i = j + 1;
			}
			None => break,
		}
	}
	out
}

/// A value's place in a string: the address, and the format after a colon, if any.
pub fn split_placeholder(inner: &str) -> (&str, Option<&str>) {
	match inner.split_once(':') {
		Some((a, f)) => (a.trim(), Some(f.trim())),
		None => (inner.trim(), None),
	}
}

/// Whether `text` holds a value to fill in.
pub fn holds_values(text: &str) -> bool {
	!placeholders(text).is_empty()
}

/// The separator between the parts of a string, a part with a missing value being left out with it.
pub const PARTS: &str = " · ";

/// `text` with its values filled in by `value` (an address and a format → the value written, or
/// `None` when it is missing). Of a string in parts joined by ` · `, a part with a missing value is
/// left out, its separator with it; so is a string of one part.
pub fn fill(text: &str, value: &dyn Fn(&str, Option<&str>) -> Option<String>) -> String {
	let mut kept = Vec::new();
	for part in text.split(PARTS) {
		let mut out = String::new();
		let mut at = 0;
		let mut missing = false;
		for p in placeholders(part) {
			out.push_str(&part[at..p.start]);
			let (address, format) = split_placeholder(p.inner);
			match value(address, format) {
				Some(v) => out.push_str(&v),
				None => missing = true,
			}
			at = p.end;
		}
		out.push_str(&part[at..]);
		if !missing {
			kept.push(out);
		}
	}
	kept.join(PARTS)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[derive(serde::Deserialize)]
	struct Case {
		format: String,
		unit: String,
		value: serde_json::Value,
		text: String,
	}

	/// Every case in elamond/fixtures/formats.json, which the app is tested against too.
	#[test]
	fn the_shared_cases() {
		let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/formats.json");
		let cases: Vec<Case> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
		assert!(cases.len() > 40);
		for c in cases {
			let shown = match &c.value {
				serde_json::Value::Number(n) => Shown::Num(n.as_f64().unwrap()),
				serde_json::Value::Bool(b) => Shown::Bool(*b),
				serde_json::Value::String(s) => Shown::Text(s),
				other => panic!("{other}"),
			};
			assert_eq!(format(shown, &c.format, &c.unit), c.text, "{} {} {}", c.format, c.unit, c.value);
		}
	}

	#[test]
	fn halves_round_away_from_zero_as_written() {
		assert_eq!(fixed(2.675, 2), "2.68");
		assert_eq!(fixed(2.665, 2), "2.67");
		assert_eq!(fixed(-2.5, 0), "-3");
		assert_eq!(fixed(9.995, 2), "10.00");
		assert_eq!(fixed(99.95, 1), "100.0");
		assert_eq!(fixed(-0.04, 1), "0.0");
		assert_eq!(fixed(0.0, 2), "0.00");
		assert_eq!(fixed(1e-7, 3), "0.000");
		assert_eq!(fixed(123456789.0, 0), "123456789");
		assert_eq!(round(12.25, 1), 12.3);
		assert_eq!(round(0.0246, 3), 0.025);
	}

	#[test]
	fn the_rest_of_the_formats() {
		assert_eq!(format(Shown::Num(9.996 * 1024.0), "bytes", "B"), "10.0 KiB");
		assert_eq!(format(Shown::Num(1023.6), "bytes", "B"), "1.00 KiB");
		assert_eq!(format(Shown::Num(5.0 * 1024f64.powi(5)), "bytes", "B"), "5.00 PiB");
		assert_eq!(format(Shown::Num(999.6), "mhz", "MHz"), "1.00 GHz");
		assert_eq!(format(Shown::Num(12345.0), "mhz", "MHz"), "12.3 GHz");
		assert_eq!(format(Shown::Num(90.0), "duration", "s"), "1 min");
		assert_eq!(format(Shown::Num(-5.0), "duration", "s"), "0 min");
		assert_eq!(format(Shown::Num(1.0), "bool", ""), "yes");
		assert_eq!(format(Shown::Num(8.0), "text", ""), "8");
		assert_eq!(format(Shown::Bool(true), "text", ""), "yes");
		assert_eq!(format(Shown::Num(41.0), "temp", ""), "41°C");
		assert_eq!(format(Shown::Num(3.0), "{:.1}", "%"), "3.0%");
		assert!(is_format("{:.3}") && is_format("duration") && !is_format("{:.12}") && !is_format("{:x}") && !is_format("guage"));
		let t = format(Shown::Num(1_700_000_000.0), "time", "s");
		assert_eq!((t.len(), &t[4..5], &t[13..14]), (16, "-", ":"), "{t}");
		assert_eq!(format(Shown::Num(1_700_000_000_000.0), "time", "ms"), t);
	}

	#[test]
	fn values_in_strings() {
		assert_eq!(
			placeholders("{a} and {gpu.{item}.summary}, {b:{:.1}} {"),
			[
				Placeholder { start: 0, end: 3, inner: "a" },
				Placeholder { start: 8, end: 28, inner: "gpu.{item}.summary" },
				Placeholder { start: 30, end: 39, inner: "b:{:.1}" }
			]
		);
		assert_eq!(split_placeholder("mem.used:bytes"), ("mem.used", Some("bytes")));
		let value = |a: &str, f: Option<&str>| match a {
			"cpu.temp" => Some(format!("56°C{}", f.map(|f| format!("({f})")).unwrap_or_default())),
			"cpu.freq_max" => Some("3.10 GHz".to_string()),
			_ => None,
		};
		assert_eq!(fill("{cpu.temp:temp} · {cpu.freq_max:mhz}", &value), "56°C(temp) · 3.10 GHz");
		// A part with a missing value goes, its separator with it.
		assert_eq!(fill("{gpu.temp:temp} · {cpu.freq_max:mhz}", &value), "3.10 GHz");
		assert_eq!(fill("{cpu.temp} · {gone} · Read", &value), "56°C · Read");
		// One part with a missing value: nothing left.
		assert_eq!(fill("{cpu.freq_max} / {gone}", &value), "");
		assert_eq!(fill("No values here", &value), "No values here");
		assert!(holds_values("Up for {system.uptime:duration}") && !holds_values("Plain {"));
	}
}
