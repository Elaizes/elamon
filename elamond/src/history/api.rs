//! `GET /api/v1/history` (docs/protocol.md): a graph's long history over one of six ranges, each
//! bucket the mean of a tier's points in it.

use std::sync::Arc;

use axum::extract::{Extension, Query, State};
use axum::response::Response;
use serde::Deserialize;
use serde_json::Value;

use super::disk::{Point, Store, Tier, EARLIEST_SECS};
use crate::access::Caller;
use crate::errors::ErrorCode;
use crate::format;
use crate::server::{error, error_with, json_response, ClientApp, Hub};
use crate::ui::{Obj, Put, Ui, UiWidget};
use crate::values::{self, Units};

/// The ranges `/history` answers, as `ranges` offers them in `ui`.
pub const RANGES: [&str; 6] = ["1h", "24h", "7d", "30d", "1y", "all"];

/// `all` comes in no more buckets than these.
const ALL_BUCKETS: u64 = 800;

/// How `range` is answered: from the points of which tier, in buckets of how many seconds, and how
/// many of them; for `all`, hours and as many as there are from the oldest point.
pub fn plan(range: &str) -> Option<(Tier, u64, Option<u64>)> {
	Some(match range {
		"1h" => (Tier::S10, 10, Some(360)),
		"24h" => (Tier::S10, 120, Some(720)),
		"7d" => (Tier::M5, 900, Some(672)),
		"30d" => (Tier::M5, 3600, Some(720)),
		"1y" => (Tier::H1, 43_200, Some(730)),
		"all" => (Tier::H1, 3600, None),
		_ => return None,
	})
}

#[derive(Deserialize, Default)]
pub struct HistoryQuery {
	widget: Option<String>,
	range: Option<String>,
}

/// The long history of a graph in the client's `ui`: the one it has now, for its role, profile and
/// the widget types it last said it draws. A timeline's comes with timelines.
pub async fn history(State(hub): State<Arc<Hub>>, Extension(who): Extension<Caller>, Extension(app): Extension<ClientApp>, Query(q): Query<HistoryQuery>) -> Response {
	let Some(widget) = q.widget.filter(|w| !w.is_empty()) else { return error_with(ErrorCode::BadParam, "widget") };
	let Some(range) = q.range.filter(|r| plan(r).is_some()) else { return error_with(ErrorCode::BadParam, "range") };
	let key = hub.ui_key(&who, &app.kind, hub.draws_of(&who.id, &app.kind));
	let frame = hub.frame.borrow().clone();
	let ui = hub.ui(&key, &frame);
	let Some(w) = graph_in(&ui, &widget) else { return error(ErrorCode::NoSuchWidget) };
	let addresses: Vec<String> = w.def.values.iter().flatten().cloned().collect();
	let (store, units) = (hub.long.clone(), ui.units);
	let answer = tokio::task::spawn_blocking(move || {
		store.readable().map(|()| graph(&store, &widget, &range, &addresses, units, crate::activity::now_ms()).unwrap_or_default())
	});
	match answer.await {
		Ok(Ok(o)) => json_response(serde_json::to_string(&o).unwrap_or_default()),
		Ok(Err(e)) => {
			eprintln!("history: can't read {}: {e}", hub.long.dir().display());
			error(ErrorCode::Storage)
		}
		Err(_) => error(ErrorCode::Storage),
	}
}

/// The graph `id` of `ui`.
fn graph_in<'a>(ui: &'a Ui, id: &str) -> Option<&'a UiWidget> {
	ui.dashboards.iter().flat_map(|d| d.all_widgets()).find(|w| w.id == id && w.kind == "graph")
}

/// The answer for the graph `widget` over `range` at `now_ms`: for each of its series'
/// `addresses`, the mean of its points in each bucket, in `units`, to one decimal; `null` for a
/// bucket without any. The buckets end at `now_ms` rounded down to a whole step. None for a range
/// there isn't.
pub fn graph(store: &Store, widget: &str, range: &str, addresses: &[String], units: Units, now_ms: u64) -> Option<Obj> {
	let (tier, step, count) = plan(range)?;
	let now = now_ms / 1000;
	let (from, to, step, points) = match count {
		Some(n) => {
			let to = now / step * step;
			let from = to.saturating_sub(n * step);
			let slots = tier.slot(from)..tier.slot(to);
			(from, to, step, addresses.iter().map(|a| store.points(tier, a, slots.clone())).collect::<Vec<_>>())
		}
		None => {
			// The hours that have ended, back to the oldest point of any series.
			let slots = tier.slot(EARLIEST_SECS)..tier.slot(now);
			let points: Vec<Vec<Point>> = addresses.iter().map(|a| store.points(tier, a, slots.clone())).collect();
			let oldest = points.iter().filter_map(|p| p.first()).map(|(s, _)| u64::from(*s) * tier.step()).min();
			let (from, to, step) = all_buckets(oldest, now);
			(from, to, step, points)
		}
	};
	let n = ((to - from) / step) as usize;
	let series: Vec<Value> = addresses
		.iter()
		.zip(&points)
		.map(|(a, points)| {
			let mut sums = vec![(0.0f64, 0u32); n];
			for (slot, x) in points {
				let t = u64::from(*slot) * tier.step();
				if (from..to).contains(&t) {
					let b = &mut sums[((t - from) / step) as usize];
					b.0 += f64::from(*x);
					b.1 += 1;
				}
			}
			let v = sums.into_iter().map(|(sum, count)| match count {
				0 => Value::Null,
				_ => Value::from(format::round(values::in_units(a, sum / f64::from(count), units), 1)),
			});
			Value::Array(v.collect())
		})
		.collect();
	let mut o = Obj::new();
	o.put("widget", widget);
	o.put("range", range);
	o.put("from", from * 1000);
	o.put("to", to * 1000);
	o.put("step_ms", step * 1000);
	o.put("t", Value::Array((0..n as u64).map(|i| Value::from((from + i * step) * 1000)).collect()));
	o.put("v", Value::Array(series));
	Some(o)
}

/// `all`'s buckets at `now` from the `oldest` point (Unix seconds): the fewest whole hours, one
/// at least, that keep them to `ALL_BUCKETS`, from the bucket of the oldest point to the step `now`
/// is in; none without a point.
fn all_buckets(oldest: Option<u64>, now: u64) -> (u64, u64, u64) {
	let mut step = 3600;
	loop {
		let to = now / step * step;
		let Some(oldest) = oldest else { return (to, to, step) };
		let from = (oldest / step * step).min(to);
		if (to - from) / step <= ALL_BUCKETS {
			return (from, to, step);
		}
		step += 3600;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::history::disk::tests::scratch;

	const NOW_MS: u64 = 1_700_000_000_123;

	fn answer(store: &Store, range: &str, addresses: &[&str], units: Units) -> serde_json::Value {
		let addresses: Vec<String> = addresses.iter().map(|a| a.to_string()).collect();
		Value::Object(graph(store, "cpu/load", range, &addresses, units, NOW_MS).unwrap())
	}

	#[test]
	fn each_range_has_its_tier_step_and_buckets() {
		let dir = scratch("ranges");
		let store = Store::new(dir.clone());
		let table = [("1h", 10, 360), ("24h", 120, 720), ("7d", 900, 672), ("30d", 3600, 720), ("1y", 43_200, 730), ("all", 3600, 0)];
		for (range, step, buckets) in table {
			let a = answer(&store, range, &["cpu.usage"], Units::default());
			let step_ms = step * 1000;
			assert_eq!(a["step_ms"], step_ms, "{range}");
			assert_eq!(a["to"], NOW_MS / step_ms * step_ms, "{range}");
			assert_eq!(a["from"].as_u64().unwrap(), a["to"].as_u64().unwrap() - buckets * step_ms, "{range}");
			assert_eq!(a["t"].as_array().unwrap().len() as u64, buckets, "{range}");
			assert_eq!(a["v"][0].as_array().unwrap().len() as u64, buckets, "{range}");
			assert!(a["v"][0].as_array().unwrap().iter().all(Value::is_null), "{range}: nothing there yet");
			assert!(buckets <= 800);
		}
		assert!(plan("2h").is_none() && plan("").is_none() && plan("ALL").is_none());
		assert_eq!(RANGES.map(|r| plan(r).is_some()), [true; 6]);
		std::fs::remove_dir_all(&dir).unwrap();
	}

	#[test]
	fn buckets_are_the_means_of_the_points_in_them() {
		let dir = scratch("buckets");
		let store = Store::new(dir.clone());
		let now = NOW_MS / 1000;
		// Readings every second over the last 30 minutes but for a gap of 5, then 50 °C a while.
		for t in now - 1800..now {
			let x = if t < now - 600 { 40.0 } else { 50.0 };
			if !(now - 1200..now - 900).contains(&t) {
				store.push(t * 1000, [("cpu.usage", (t % 10) as f64), ("cpu.temp", x)]);
			}
		}
		let a = answer(&store, "1h", &["cpu.usage", "cpu.temp"], Units::default());
		let (usage, temp) = (a["v"][0].as_array().unwrap(), a["v"][1].as_array().unwrap());
		// The first half hour has none; the last 30 minutes a point every 10 s, the gap none.
		assert!(usage[..180].iter().all(Value::is_null));
		assert_eq!(usage[180], 4.5);
		assert!(usage[240..270].iter().all(Value::is_null));
		assert_eq!(temp[359], 50.0);
		// The last bucket ends where the step `now` is in begins.
		assert_eq!(a["t"][359], (now / 10 * 10 - 10) * 1000);
		// In °F, as the client's profile asks.
		let f = answer(&store, "24h", &["cpu.temp"], Units { fahrenheit: true, bits: false });
		let f = f["v"][0].as_array().unwrap();
		// Buckets of 2 minutes: six before the gap, seven after it and before the step `now` is in.
		assert_eq!(f.iter().filter(|x| !x.is_null()).count(), 13);
		assert_eq!((&f[710..712], &f[719]), (&[Value::from(104.0), Value::Null][..], &Value::from(122.0)));
		// Across the change: 8 points of 40 °C and 4 of 50.
		assert_eq!(f[715], 110.0);
		std::fs::remove_dir_all(&dir).unwrap();
	}

	#[test]
	fn all_reaches_back_to_the_oldest_point_in_no_more_than_800_buckets() {
		assert_eq!(all_buckets(None, 7200 * 1000 + 5), (7_200_000, 7_200_000, 3600));
		// Six hours: six buckets of an hour.
		let now = 1_700_000_000;
		let to = now / 3600 * 3600;
		assert_eq!(all_buckets(Some(to - 6 * 3600 + 100), now), (to - 6 * 3600, to, 3600));
		// 800 hours fit; 801 take buckets of two.
		assert_eq!(all_buckets(Some(to - 800 * 3600), now).2, 3600);
		let (from, to2, step) = all_buckets(Some(to - 801 * 3600), now);
		assert_eq!(step, 7200);
		assert!((to2 - from) / step <= 800 && from % step == 0 && to2 == now / 7200 * 7200);
		// Ten years: the fewest hours that keep it to 800.
		let oldest = now - 10 * 365 * 86_400;
		let buckets = |step: u64| (now / step * step - oldest / step * step) / step;
		let (_, _, step) = all_buckets(Some(oldest), now);
		assert!(buckets(step) <= 800 && buckets(step - 3600) > 800, "{step}");
		// Only a point in the hour going on: no buckets.
		assert_eq!(all_buckets(Some(now), now), (to, to, 3600));
	}
}
