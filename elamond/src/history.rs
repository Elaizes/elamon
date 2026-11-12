//! The last minutes of every value a graph or a spark of the settings shows, by address, so a
//! graph is full the moment a client opens it; and the `history` of a stream's first `data` line.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::format;
use crate::ui::{Dashboard, Obj, J};
use crate::values::{self, Units, Values};

/// Points of every tracked address, at the same times, oldest first.
#[derive(Clone, Debug, Default)]
pub struct Ring {
	cap: usize,
	t: VecDeque<u64>,
	series: BTreeMap<String, VecDeque<Option<f64>>>,
}

impl Ring {
	/// A ring of `cap` points: `history_minutes` at the service's tick.
	pub fn new(cap: usize) -> Ring {
		Ring { cap: cap.max(1), t: VecDeque::new(), series: BTreeMap::new() }
	}

	pub fn capacity(&self) -> usize {
		self.cap
	}

	pub fn len(&self) -> usize {
		self.t.len()
	}

	pub fn is_empty(&self) -> bool {
		self.t.is_empty()
	}

	/// The values of `tracked` (`ui::tracked`) in a reading.
	pub fn push(&mut self, values: &Values, tracked: &BTreeSet<String>) {
		let points = tracked.iter().map(|a| (a.clone(), values.scalars.get(a).and_then(|s| s.v.num())));
		self.push_points(values.t, points);
	}

	/// A point at `t` of each address given; an address not given has none there, and one that
	/// has had no value in all the ring holds is forgotten.
	pub fn push_points(&mut self, t: u64, points: impl IntoIterator<Item = (String, Option<f64>)>) {
		if self.t.len() == self.cap {
			self.t.pop_front();
			for s in self.series.values_mut() {
				s.pop_front();
			}
		}
		let before = self.t.len();
		self.t.push_back(t);
		for (address, x) in points {
			let s = self.series.entry(address).or_insert_with(|| std::iter::repeat_n(None, before).collect());
			if s.len() == before {
				s.push_back(x);
			}
		}
		for s in self.series.values_mut() {
			if s.len() == before {
				s.push_back(None);
			}
		}
		self.series.retain(|_, s| s.iter().any(Option::is_some));
	}

	/// The times from `since` on, and the address's value at each.
	pub fn window(&self, address: &str, since: u64) -> (Vec<u64>, Vec<Option<f64>>) {
		let from = self.t.iter().position(|t| *t >= since).unwrap_or(self.t.len());
		let t = self.t.iter().skip(from).copied().collect();
		let v = match self.series.get(address) {
			Some(s) => s.iter().skip(from).copied().collect(),
			None => vec![None; self.t.len() - from],
		};
		(t, v)
	}
}

/// `history` for a dashboard at `now` (Unix ms): for each graph its points over its `range`
/// (`range_s`, all the ring holds when none), and for each value with `spark` all of them; in the
/// client's units, rounded to one decimal.
pub fn json(d: &Dashboard, ring: &Ring, units: Units, now: u64) -> Obj {
	let mut out = Obj::new();
	for w in d.all_widgets() {
		let (addresses, range): (Vec<&str>, Option<u64>) = match w.kind.as_str() {
			"graph" => (w.def.values.iter().flatten().map(String::as_str).collect(), w.def.range),
			"value" if w.def.spark == Some(true) => (w.def.value.as_deref().into_iter().collect(), None),
			_ => continue,
		};
		let since = range.map_or(0, |r| now.saturating_sub(r.saturating_mul(1000)));
		let mut times = Vec::new();
		let mut series = Vec::new();
		for a in &addresses {
			let (t, v) = ring.window(a, since);
			times = t;
			let v: Vec<J> = v.into_iter().map(|x| x.map_or(J::Null, |x| J::from(format::round(values::in_units(a, x, units), 1)))).collect();
			series.push(J::Arr(v));
		}
		if addresses.is_empty() {
			times = ring.window("", since).0;
		}
		let mut h = Obj::new();
		h.put("t", J::Arr(times.into_iter().map(J::from).collect()));
		h.put("v", J::Arr(series));
		out.put(&w.id, h);
	}
	out
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_ring_keeps_the_newest_points_of_each_address() {
		let mut r = Ring::new(3);
		let p = |a: &str, x: Option<f64>| (a.to_string(), x);
		r.push_points(1, [p("cpu.usage", Some(1.0))]);
		r.push_points(2, [p("cpu.usage", Some(2.0)), p("cpu.temp", Some(50.0))]);
		r.push_points(3, [p("cpu.usage", Some(3.0)), p("cpu.temp", None)]);
		r.push_points(4, [p("cpu.usage", Some(4.0))]);
		assert_eq!(r.window("cpu.usage", 0), (vec![2, 3, 4], vec![Some(2.0), Some(3.0), Some(4.0)]));
		assert_eq!(r.window("cpu.temp", 0), (vec![2, 3, 4], vec![Some(50.0), None, None]));
		assert_eq!(r.window("cpu.temp", 3).1, [None, None]);
		assert_eq!(r.window("gone", 0).1, [None, None, None]);
		// Nothing in all it holds: forgotten.
		r.push_points(5, []);
		r.push_points(6, []);
		assert!(!r.series.contains_key("cpu.temp"));
		assert_eq!((r.len(), r.capacity()), (3, 3));
	}
}
