//! The history of the values graphs and sparks show: the last minutes of each in memory, at every
//! reading (`ring`), and on disk every 10 seconds for a day, every 5 minutes for 30 days and every
//! hour for good (`disk`), which `GET /api/v1/history` answers from (`api`).

pub mod api;
pub mod disk;
pub mod ring;

use std::collections::{BTreeMap, BTreeSet};

pub use disk::Store;
pub use ring::{json, Ring};

use crate::values::Values;

/// The addresses the long history keeps, with their values in `values`: those graphs and sparks
/// show (`tracked`, as the ring keeps).
pub fn recorded<'a>(values: &'a Values, tracked: &'a BTreeSet<String>) -> impl Iterator<Item = (&'a str, f64)> + 'a {
	tracked.iter().filter_map(|a| Some((a.as_str(), values.scalars.get(a)?.v.num()?)))
}

/// Fills `ring`, as the service starts, with the 10-second points of `addresses` over the `span_ms`
/// before `now_ms`: graphs reach back past a restart, a point every 10 seconds.
pub fn prefill(ring: &mut Ring, store: &Store, addresses: &BTreeSet<String>, now_ms: u64, span_ms: u64) {
	let tier = disk::Tier::S10;
	let now = now_ms / 1000;
	let slots = tier.slot(now.saturating_sub(span_ms / 1000))..tier.slot(now).saturating_add(1);
	let mut at: BTreeMap<u32, Vec<(String, Option<f64>)>> = BTreeMap::new();
	for a in addresses {
		for (slot, x) in store.points(tier, a, slots.clone()) {
			at.entry(slot).or_default().push((a.clone(), Some(f64::from(x))));
		}
	}
	for (slot, points) in at {
		ring.push_points(u64::from(slot) * tier.step() * 1000, points);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn after_a_restart_the_ring_starts_from_the_10_second_points() {
		let dir = disk::tests::scratch("prefill");
		let now = 1_700_000_000;
		let before = Store::new(dir.clone());
		// Ten minutes and more of readings a second, then the service stops midway through a slot.
		for t in now - 700..now - 5 {
			before.push(t * 1000, [("cpu.usage", (t % 100) as f64), ("mem.used", 5.0)]);
		}
		before.flush(now, true);
		let after = Store::new(dir.clone());
		let mut ring = Ring::new(600);
		let tracked: BTreeSet<String> = ["cpu.usage", "net.rx"].map(String::from).into();
		prefill(&mut ring, &after, &tracked, (now + 30) * 1000, 600_000);
		let (t, v) = ring.window("cpu.usage", 0);
		// 60 slots of 10 s at most, the one the restart cut short last.
		assert_eq!(t.len(), 57);
		assert_eq!((t[0], *t.last().unwrap()), ((now - 570) * 1000, (now - 10) * 1000));
		assert!(t.windows(2).all(|w| w[1] - w[0] == 10_000));
		// Each the mean of its readings: 30..=39 → 34.5.
		assert_eq!(v[0], Some(((now - 570) % 100) as f64 + 4.5));
		assert_eq!(ring.window("net.rx", 0).1, vec![None; 57]);
		assert!(ring.window("mem.used", 0).1.iter().all(Option::is_none), "not tracked");
		// Readings go on after them.
		ring.push_points((now + 31) * 1000, [("cpu.usage".to_string(), Some(1.0))]);
		assert_eq!(ring.window("cpu.usage", 0).0.len(), 58);
		std::fs::remove_dir_all(&dir).unwrap();
	}
}
