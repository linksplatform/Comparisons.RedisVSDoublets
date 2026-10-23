//! # Typical Redis Usage Benchmarks
//!
//! `B` is [`background_links`] and `N` is [`benchmark_links`]. Before every
//! iteration the store contains the keys `1..=B` (each with its own number as
//! the value), the queue `1, ..., B` and the set `{1, ..., B}` (see
//! [`Snapshot::background`](redis_vs_doublets::Snapshot::background)).
//!
//! | Group               | Measured work of one iteration                                   | Undone after the iteration by     |
//! |---------------------|------------------------------------------------------------------|-----------------------------------|
//! | `KeyValue_Get`      | `N` × `GET k` for the keys `1..=N`                               | nothing (read only)               |
//! | `KeyValue_Set`      | `N` × `SET k k+B` for the keys `1..=N`                           | `SET k k`                         |
//! | `KeyValue_Delete`   | `N` × `DEL k` for the keys `B` down to `B - N + 1`               | `SET k k`                         |
//! | `Counter_Increment` | `N` × `INCR k` for the keys `1..=N`                              | `SET k k`                         |
//! | `Queue_Push`        | `N` × `RPUSH B+i`                                                | `N` × `RPOP`                      |
//! | `Queue_Pop`         | `N` × `LPOP`, which returns `1..=N`                              | `LPUSH` of `N` down to `1`        |
//! | `Set_Add`           | `N` × `SADD B+i`                                                 | `SREM B+i`                        |
//! | `Set_Remove`        | `N` × `SREM m` for the members `B` down to `B - N + 1`           | `SADD m`                          |
//! | `Set_Contains`      | `N` × `SISMEMBER m` for the members `1..=N`                      | nothing (read only)               |
//!
//! The commands name the Redis implementation ([`RedisUsage`]); Doublets
//! ([`DoubletsUsage`]) does the same work with links. Every read value is
//! passed to the store's sink, which consumes it with
//! [`black_box`].
//!
//! One unmeasured iteration of `KeyValue_Get` and `Queue_Pop` must read the
//! values `1..=N`, one of `Set_Contains` must find all `N` members (counted
//! by their number and the sum of the values), and the changes read nothing.

use std::hint::black_box;

use criterion::{BenchmarkGroup, Criterion, measurement::WallTime};
use redis_vs_doublets::{
    Benched, DoubletsUsage, Mode, RedisUsage, Result, Snapshot, Usage, background_links,
    benchmark_links,
};

use super::{Checked, Found, SinkResults, doublets_group, measure, read_only, redis_group};

/// The groups, in the order in which they run.
const GROUPS: [&str; 9] = [
    "KeyValue_Get",
    "KeyValue_Set",
    "KeyValue_Delete",
    "Counter_Increment",
    "Queue_Push",
    "Queue_Pop",
    "Set_Add",
    "Set_Remove",
    "Set_Contains",
];

/// Runs every group on Redis in all three modes.
pub fn redis(c: &mut Criterion) {
    for name in GROUPS {
        let mut group = redis_group(c, name);
        for mode in Mode::ALL {
            let mut store = RedisUsage::setup(mode).expect("cannot connect to Redis");
            run(&mut group, name, mode.name(), &mut store);
        }
        group.finish();
    }
}

/// Runs every group on the links of the Doublets unit store in RAM.
pub fn doublets(c: &mut Criterion) {
    for name in GROUPS {
        let mut group = doublets_group(c, name);
        let mut store = DoubletsUsage::setup(()).expect("cannot create the unit store");
        run(&mut group, name, DoubletsUsage::NAME, &mut store);
        group.finish();
    }
}

/// All usage stores pass every read value to their sink.
macro_rules! checked_usage {
    ($($store:ty),*) => {$(
        impl Checked for $store {
            fn count_sink(&mut self, results: Option<SinkResults>) {
                match results {
                    Some(results) => self.set_sink(move |value| {
                        results
                            .lock()
                            .expect("sink results mutex is poisoned")
                            .add(value);
                    }),
                    None => self.set_sink(|value| {
                        black_box(value);
                    }),
                }
            }

            fn check_background(&mut self, background: usize) -> Result<()> {
                assert_eq!(
                    self.snapshot()?,
                    Snapshot::background(background),
                    "the store must contain exactly the background data"
                );
                Ok(())
            }
        }
    )*};
}

checked_usage!(RedisUsage, DoubletsUsage);

/// Calls `action` for `1..=N`.
fn first(action: &mut impl FnMut(u64) -> Result<()>) -> Result<()> {
    (1..=benchmark_links() as u64).try_for_each(action)
}

/// Calls `action` for `B` down to `B - N + 1`.
fn last(action: &mut impl FnMut(u64) -> Result<()>) -> Result<()> {
    let (background, links) = (background_links() as u64, benchmark_links() as u64);
    (background - links + 1..=background)
        .rev()
        .try_for_each(action)
}

/// Measures the work of the group `name`.
fn run<U: Checked + Usage>(
    group: &mut BenchmarkGroup<WallTime>,
    name: &str,
    id: &str,
    benched: &mut U,
) {
    let b = background_links() as u64;
    let (nothing, first_values) = (Found::default(), Found::first(benchmark_links()));
    let all_members = Found {
        count: benchmark_links() as u64,
        sum: benchmark_links() as u64,
    };
    match name {
        "KeyValue_Get" => measure(
            group,
            id,
            benched,
            first_values,
            |s| first(&mut |k| s.get(k)),
            read_only,
        ),
        "KeyValue_Set" => measure(
            group,
            id,
            benched,
            nothing,
            |s| first(&mut |k| s.set(k, k + b)),
            |s| first(&mut |k| s.set(k, k)),
        ),
        "KeyValue_Delete" => measure(
            group,
            id,
            benched,
            nothing,
            |s| last(&mut |k| s.delete(k)),
            |s| last(&mut |k| s.set(k, k)),
        ),
        "Counter_Increment" => measure(
            group,
            id,
            benched,
            nothing,
            |s| first(&mut |k| s.increment(k)),
            |s| first(&mut |k| s.set(k, k)),
        ),
        "Queue_Push" => measure(
            group,
            id,
            benched,
            nothing,
            |s| first(&mut |i| s.push(b + i)),
            |s| first(&mut |_| s.pop_back()),
        ),
        "Queue_Pop" => measure(
            group,
            id,
            benched,
            first_values,
            |s| first(&mut |_| s.pop()),
            |s| {
                (1..=benchmark_links() as u64)
                    .rev()
                    .try_for_each(|k| s.push_front(k))
            },
        ),
        "Set_Add" => measure(
            group,
            id,
            benched,
            nothing,
            |s| first(&mut |i| s.add(b + i)),
            |s| first(&mut |i| s.remove(b + i)),
        ),
        "Set_Remove" => measure(
            group,
            id,
            benched,
            nothing,
            |s| last(&mut |m| s.remove(m)),
            |s| last(&mut |m| s.add(m)),
        ),
        "Set_Contains" => measure(
            group,
            id,
            benched,
            all_members,
            |s| first(&mut |m| s.contains(m)),
            read_only,
        ),
        _ => unreachable!("unknown group {name}"),
    }
}
