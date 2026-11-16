//! What a source read, taken as its `format` says (docs/config.md, A source's values): JSON leaf by
//! leaf, one text, or `key = value` lines. Each value comes with its path after the source's id.

use serde_json::Value;

use super::Failure;
use crate::consts::limit;
use crate::settings::FeedFormat;
use crate::values::V;

/// The service's own addresses of a source, which a key at the top of what it read doesn't take.
const OWN: [&str; 3] = ["exit", "age", "error"];

/// The values one read gives, by path (`queue.length`), in the order read; and whether some were
/// left out, past `limit::FEED_VALUES_MAX` or deeper than `limit::FEED_DEPTH_MAX`.
#[derive(Debug, Default, PartialEq)]
pub struct Parsed {
	pub values: Vec<(String, V)>,
	pub cut: bool,
}

impl Parsed {
	/// `v` at `path`, unless the read has as many values as it may have; a path it has already
	/// takes the new value.
	fn put(&mut self, path: String, v: V) {
		if let Some(have) = self.values.iter_mut().find(|(p, _)| *p == path) {
			have.1 = v;
		} else if self.values.len() < limit::FEED_VALUES_MAX {
			self.values.push((path, v));
		} else {
			self.cut = true;
		}
	}
}

/// What `bytes` give as `format`; JSON that isn't JSON fails.
pub fn parse(format: FeedFormat, bytes: &[u8]) -> Result<Parsed, Failure> {
	let mut out = Parsed::default();
	match format {
		FeedFormat::Json => {
			let v: Value = serde_json::from_slice(bytes).map_err(|e| Failure::with("not JSON", e.to_string()))?;
			match &v {
				Value::Object(_) | Value::Array(_) => leaves(&v, &mut Vec::new(), &mut out),
				Value::Null => {}
				leaf => out.put("value".into(), scalar(leaf)),
			}
		}
		FeedFormat::Text => {
			let text = String::from_utf8_lossy(bytes);
			let kept: String = text.chars().filter(|c| *c == '\n' || !c.is_control()).collect();
			let t = kept.trim();
			if !t.is_empty() {
				out.put("text".into(), number(t).unwrap_or_else(|| V::Text(cut(t))));
			}
		}
		FeedFormat::Lines => {
			for line in String::from_utf8_lossy(bytes).lines() {
				let Some(at) = line.find(['=', ':']) else { continue };
				let key = segment(line[..at].trim());
				if OWN.contains(&key.as_str()) {
					continue;
				}
				let raw = line[at + 1..].trim();
				let v = match raw {
					"true" => V::Bool(true),
					"false" => V::Bool(false),
					_ => number(raw).unwrap_or_else(|| V::Text(cut(raw))),
				};
				out.put(key, v);
			}
		}
	}
	Ok(out)
}

/// The leaves of `v` under `path`, a part for each key of an object and place in an array.
fn leaves(v: &Value, path: &mut Vec<String>, out: &mut Parsed) {
	let parts: Vec<(String, &Value)> = match v {
		Value::Object(o) => o.iter().map(|(k, e)| (segment(k), e)).collect(),
		Value::Array(a) => a.iter().enumerate().map(|(i, e)| (i.to_string(), e)).collect(),
		Value::Null => return,
		leaf => {
			out.put(path.join("."), scalar(leaf));
			return;
		}
	};
	if path.len() >= limit::FEED_DEPTH_MAX {
		out.cut |= !parts.is_empty();
		return;
	}
	for (part, e) in parts {
		if path.is_empty() && OWN.contains(&part.as_str()) {
			continue;
		}
		path.push(part);
		leaves(e, path, out);
		path.pop();
	}
}

/// A JSON leaf as a value: a whole number stays whole.
fn scalar(v: &Value) -> V {
	match v {
		Value::Number(n) => match n.as_i64() {
			Some(i) => V::Int(i),
			None => V::Float(n.as_f64().unwrap_or(f64::NAN)),
		},
		Value::Bool(b) => V::Bool(*b),
		Value::String(s) => V::Text(cut(s)),
		other => V::Text(cut(&other.to_string())),
	}
}

/// `s` as a number when the whole of it is one: whole, or a finite fraction.
fn number(s: &str) -> Option<V> {
	s.parse::<i64>().ok().map(V::Int).or_else(|| s.parse::<f64>().ok().filter(|x| x.is_finite()).map(V::Float))
}

/// `s` of `limit::FEED_TEXT_MAX` characters at most, the last of them `…` when it was longer.
fn cut(s: &str) -> String {
	if s.chars().count() <= limit::FEED_TEXT_MAX {
		return s.to_string();
	}
	s.chars().take(limit::FEED_TEXT_MAX - 1).chain(['…']).collect()
}

/// A key as a part of an address: `A-Z`, `a-z`, `0-9`, `_` and `-`, anything else `_`; an empty one
/// `_`.
fn segment(key: &str) -> String {
	let s: String = key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
	if s.is_empty() { "_".into() } else { s }
}

#[cfg(test)]
mod tests {
	use super::*;

	fn values(format: FeedFormat, text: &str) -> Vec<(String, V)> {
		parse(format, text.as_bytes()).unwrap().values
	}

	fn get(format: FeedFormat, text: &str, path: &str) -> Option<V> {
		values(format, text).into_iter().find(|(p, _)| p == path).map(|(_, v)| v)
	}

	/// The fixture's web service, as ui-sources.toml's comments have it.
	#[test]
	fn json_is_read_leaf_by_leaf() {
		let body = r#"{"state": "ok", "version": "2.4.1", "queue": {"length": 3},
			"workers": [{"name": "main", "busy": true}, {"name": "spare", "busy": false}],
			"build info": {"commit": "0123abc"}, "note": null}"#;
		let v = values(FeedFormat::Json, body);
		let paths: Vec<&str> = v.iter().map(|(p, _)| p.as_str()).collect();
		assert_eq!(
			paths,
			["state", "version", "queue.length", "workers.0.name", "workers.0.busy", "workers.1.name", "workers.1.busy", "build_info.commit"]
		);
		assert_eq!(v[2].1, V::Int(3));
		assert_eq!(v[4].1, V::Bool(true));
		// Whole numbers stay whole; fractions, big ones and negative ones as they are.
		assert_eq!(get(FeedFormat::Json, r#"{"a": 1.5}"#, "a"), Some(V::Float(1.5)));
		assert_eq!(get(FeedFormat::Json, r#"{"a": 2.0}"#, "a"), Some(V::Float(2.0)));
		assert_eq!(get(FeedFormat::Json, r#"{"a": -7}"#, "a"), Some(V::Int(-7)));
		assert_eq!(get(FeedFormat::Json, r#"{"a": 18446744073709551615}"#, "a"), Some(V::Float(18446744073709551615.0)));
		// A root that is a value of its own; null, none at all.
		assert_eq!(values(FeedFormat::Json, "42"), [("value".to_string(), V::Int(42))]);
		assert_eq!(values(FeedFormat::Json, "\"up\""), [("value".to_string(), V::Text("up".into()))]);
		assert_eq!(values(FeedFormat::Json, "true"), [("value".to_string(), V::Bool(true))]);
		assert!(values(FeedFormat::Json, "null").is_empty() && values(FeedFormat::Json, "{}").is_empty() && values(FeedFormat::Json, "[]").is_empty());
		// A top-level array by places.
		assert_eq!(values(FeedFormat::Json, "[5, [6]]"), [("0".to_string(), V::Int(5)), ("1.0".to_string(), V::Int(6))]);
		// Keys of other characters, and empty ones; of two that come to the same, the last.
		let v = values(FeedFormat::Json, r#"{"a.b": 1, "": 2, "é": 3, "x-y_Z": 4}"#);
		assert_eq!(v, [("a_b".to_string(), V::Int(1)), ("_".to_string(), V::Int(3)), ("x-y_Z".to_string(), V::Int(4))]);
		// The service's own names, at the top only.
		let v = values(FeedFormat::Json, r#"{"exit": 1, "age": 2, "error": "x", "job": {"exit": 3, "error": "y"}}"#);
		assert_eq!(v, [("job.exit".to_string(), V::Int(3)), ("job.error".to_string(), V::Text("y".into()))]);
	}

	#[test]
	fn json_is_kept_within_its_limits() {
		// A long string, cut with an ellipsis.
		let long = "x".repeat(1500);
		let v = get(FeedFormat::Json, &format!("{{\"a\": \"{long}\"}}"), "a").unwrap();
		let V::Text(t) = v else { panic!() };
		assert_eq!((t.chars().count(), t.ends_with('…')), (1000, true));
		assert_eq!(cut(&"y".repeat(1000)).chars().count(), 1000);
		assert!(!cut(&"y".repeat(1000)).ends_with('…'));
		// 8 levels deep at most.
		let deep = "{\"a\":{\"b\":{\"c\":{\"d\":{\"e\":{\"f\":{\"g\":{\"h\":1,\"i\":{\"j\":2}}}}}}}}}";
		let p = parse(FeedFormat::Json, deep.as_bytes()).unwrap();
		assert_eq!((p.values, p.cut), (vec![("a.b.c.d.e.f.g.h".to_string(), V::Int(1))], true));
		// 256 leaves at most, the first.
		let many: Vec<String> = (0..300).map(|i| format!("\"k{i:03}\": {i}")).collect();
		let p = parse(FeedFormat::Json, format!("{{{}}}", many.join(",")).as_bytes()).unwrap();
		assert_eq!((p.values.len(), p.values[255].0.as_str(), p.cut), (256, "k255", true));
		assert!(!parse(FeedFormat::Json, b"{\"a\": 1}").unwrap().cut);
		// Not JSON, said why.
		let e = parse(FeedFormat::Json, b"Status: ok").unwrap_err();
		assert_eq!(e.why, "not JSON");
		assert!(e.text().starts_with("not JSON: expected value at line 1 column 1"), "{}", e.text());
		assert_eq!(parse(FeedFormat::Json, b"").unwrap_err().why, "not JSON");
	}

	#[test]
	fn text_is_one_value() {
		// A number from /sys.
		assert_eq!(values(FeedFormat::Text, "45000\n"), [("text".to_string(), V::Int(45000))]);
		assert_eq!(get(FeedFormat::Text, "  0.75 ", "text"), Some(V::Float(0.75)));
		assert_eq!(get(FeedFormat::Text, "-3", "text"), Some(V::Int(-3)));
		// Not all of it a number: text, trimmed, its control characters but line breaks taken out.
		assert_eq!(get(FeedFormat::Text, "up 3 days,\n\t2 users\x1b[0m\r\n", "text"), Some(V::Text("up 3 days,\n2 users[0m".into())));
		assert_eq!(get(FeedFormat::Text, "45 000", "text"), Some(V::Text("45 000".into())));
		assert_eq!(get(FeedFormat::Text, "inf", "text"), Some(V::Text("inf".into())));
		assert_eq!(get(FeedFormat::Text, "NaN", "text"), Some(V::Text("NaN".into())));
		// Nothing but spaces: no value.
		assert!(values(FeedFormat::Text, " \n\t ").is_empty() && values(FeedFormat::Text, "").is_empty());
		// Bytes that aren't UTF-8, as well as they go.
		assert_eq!(get(FeedFormat::Text, "a\u{FFFD}b", "text"), parse(FeedFormat::Text, b"a\xffb").unwrap().values.pop().map(|(_, v)| v));
		let V::Text(t) = get(FeedFormat::Text, &"z".repeat(5000), "text").unwrap() else { panic!() };
		assert_eq!(t.chars().count(), 1000);
	}

	/// The fixture's command, as ui-sources.toml's comments have it.
	#[test]
	fn lines_are_keys_and_values() {
		let out = "state: ok\nlast run = 1699996400\nfiles: 1204\nverified=true\nChecked 3 repositories.\n";
		assert_eq!(
			values(FeedFormat::Lines, out),
			[
				("state".to_string(), V::Text("ok".into())),
				("last_run".to_string(), V::Int(1699996400)),
				("files".to_string(), V::Int(1204)),
				("verified".to_string(), V::Bool(true)),
			]
		);
		// The first `=` or `:` splits; the last of a key counts; the service's own are left out.
		let v = values(FeedFormat::Lines, "url: http://localhost:8080\nratio = 0.5\nratio = 0.25\nexit=3\nflag: False\n= x\nnone\n");
		assert_eq!(
			v,
			[
				("url".to_string(), V::Text("http://localhost:8080".into())),
				("ratio".to_string(), V::Float(0.25)),
				("flag".to_string(), V::Text("False".into())),
				("_".to_string(), V::Text("x".into())),
			]
		);
		assert!(values(FeedFormat::Lines, "nothing here\n").is_empty());
	}
}
