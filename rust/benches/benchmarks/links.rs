//! # Links Storage Benchmarks
//!
//! `B` is [`background_links`] and `N` is [`benchmark_links`]. Before every
//! iteration the store contains the point links `1..=B`.
//!
//! | Group           | Measured work of one iteration                                      | Undone after the iteration by                 |
//! |-----------------|---------------------------------------------------------------------|-----------------------------------------------|
//! | `Create`        | `N` × `create_point()`, which creates links `B + 1..=B + N`         | deleting links `B + N` to `B + 1`             |
//! | `Update`        | `N` × (`update(id, 0, 0)` then `update(id, id, id)`) = `2N` updates | nothing (the second update restores the link) |
//! | `Delete`        | `N` × `delete(id)`, from link `B` down to link `B - N + 1`          | `N` × `create_point()`                        |
//! | `Each_All`      | 1 × `each(...)`, which visits all `B` links                         | nothing (read only)                           |
//! | `Each_Identity` | `N` × `each_by([id, *, *])` for the ids `1..=N`                     | nothing (read only)                           |
//! | `Each_Concrete` | `N` × `each_by([*, id, id])`                                        | nothing (read only)                           |
//! | `Each_Outgoing` | `N` × `each_by([*, id, *])`                                         | nothing (read only)                           |
//! | `Each_Incoming` | `N` × `each_by([*, *, id])`                                         | nothing (read only)                           |
//!
//! Every benchmark runs on Redis in the three [`Mode`]s and on the Doublets
//! unit and split stores. See [`redis_vs_doublets::redis_impl`] and
//! [`redis_vs_doublets::doublets_impl`] for how every operation is
//! implemented. In the transaction and pipeline modes Redis reads the found
//! links at the commit and passes them to its sink, which, like [`visit`],
//! consumes them with [`black_box`].
//!
//! One unmeasured iteration of `Each_All` must find the links `1..=B`, and
//! one of the other `Each_*` groups the links `1..=N` (counted by their number
//! and the sum of their ids); the changes find nothing.

use std::hint::black_box;

use criterion::{BenchmarkGroup, Criterion, measurement::WallTime};
use doublets::{
    Doublets, Link,
    data::{Flow, LinksConstants},
};
use redis_vs_doublets::{
    Benched, DoubletsSplitVolatile, DoubletsUnitedVolatile, Mode, Redis, background_links,
    benchmark_links,
};

use super::{Checked, Found, SinkResults, doublets_group, measure, read_only, redis_group};

/// Signature shared by all operations, so they can be passed around.
type Operation<B> = fn(&mut BenchmarkGroup<WallTime>, &str, &mut B);

/// The operation of every group of a store type, in the order in which they
/// run.
fn operations<B: Checked + Doublets<usize>>() -> [(&'static str, Operation<B>); 8] {
    [
        ("Create", create),
        ("Delete", delete),
        ("Each_Identity", each_identity),
        ("Each_Concrete", each_concrete),
        ("Each_Outgoing", each_outgoing),
        ("Each_Incoming", each_incoming),
        ("Each_All", each_all),
        ("Update", update),
    ]
}

/// Runs every group on Redis in all three modes.
pub fn redis(c: &mut Criterion) {
    for (name, operation) in operations::<Redis<usize>>() {
        let mut group = redis_group(c, name);
        for mode in Mode::ALL {
            let mut store = Redis::setup(mode).expect("cannot connect to Redis");
            operation(&mut group, mode.name(), &mut store);
        }
        group.finish();
    }
}

/// Runs every group on the Doublets unit and split stores in RAM.
pub fn doublets(c: &mut Criterion) {
    let united = operations::<DoubletsUnitedVolatile>();
    let split = operations::<DoubletsSplitVolatile>();
    for ((name, united), (_, split)) in united.into_iter().zip(split) {
        let mut group = doublets_group(c, name);
        united(
            &mut group,
            "Doublets_United_Volatile",
            &mut DoubletsUnitedVolatile::setup(()).expect("cannot create the unit store"),
        );
        split(
            &mut group,
            "Doublets_Split_Volatile",
            &mut DoubletsSplitVolatile::setup(()).expect("cannot create the split store"),
        );
        group.finish();
    }
}

/// Consumes every found link, so the compiler cannot skip reading it, and
/// adds it to `found`.
fn visit(found: &mut Found, link: Link<usize>) -> Flow {
    found.add(black_box(link).index as u64);
    Flow::Continue
}

/// Panics unless `store` contains exactly the point links `1..=background`.
fn check_links<B: Doublets<usize>>(store: &B, background: usize) {
    let mut links = Vec::with_capacity(background);
    store.each(|link| {
        links.push((link.index, link.source, link.target));
        Flow::Continue
    });
    links.sort_unstable();
    let points = (1..=background).map(|id| (id, id, id));
    assert!(
        links.iter().copied().eq(points),
        "the store must contain exactly the point links 1..={background}, \
         but it contains {} links",
        links.len()
    );
}

impl Checked for Redis<usize> {
    fn count_sink(&mut self, results: Option<SinkResults>) {
        match results {
            Some(results) => self.set_sink(move |link| {
                results
                    .lock()
                    .expect("sink results mutex is poisoned")
                    .add(link.index as u64);
            }),
            None => self.set_sink(|link| {
                black_box(link);
            }),
        }
    }

    fn check_background(&mut self, background: usize) -> redis_vs_doublets::Result<()> {
        check_links(self, background);
        Ok(())
    }
}

/// Doublets stores pass all found links to the handlers.
macro_rules! checked_doublets {
    ($($store:ty),*) => {$(
        impl Checked for $store {
            fn count_sink(&mut self, _: Option<SinkResults>) {}

            fn check_background(&mut self, background: usize) -> redis_vs_doublets::Result<()> {
                check_links(self, background);
                Ok(())
            }
        }
    )*};
}

checked_doublets!(DoubletsUnitedVolatile, DoubletsSplitVolatile);

fn any() -> usize {
    LinksConstants::<usize>::new().any
}

fn create<B: Checked + Doublets<usize>>(
    group: &mut BenchmarkGroup<WallTime>,
    id: &str,
    benched: &mut B,
) {
    let (background, links) = (background_links(), benchmark_links());
    measure(
        group,
        id,
        benched,
        Found::default(),
        |store| {
            for _ in 0..links {
                store.create_point()?;
            }
            Ok(())
        },
        |store| {
            for id in (background + 1..=background + links).rev() {
                store.delete(id)?;
            }
            Ok(())
        },
    );
}

fn update<B: Checked + Doublets<usize>>(
    group: &mut BenchmarkGroup<WallTime>,
    id: &str,
    benched: &mut B,
) {
    let (background, links) = (background_links(), benchmark_links());
    measure(
        group,
        id,
        benched,
        Found::default(),
        |store| {
            for id in background - links + 1..=background {
                store.update(id, 0, 0)?;
                store.update(id, id, id)?;
            }
            Ok(())
        },
        read_only,
    );
}

fn delete<B: Checked + Doublets<usize>>(
    group: &mut BenchmarkGroup<WallTime>,
    id: &str,
    benched: &mut B,
) {
    let (background, links) = (background_links(), benchmark_links());
    measure(
        group,
        id,
        benched,
        Found::default(),
        |store| {
            for id in (background - links + 1..=background).rev() {
                store.delete(id)?;
            }
            Ok(())
        },
        |store| {
            for _ in 0..links {
                store.create_point()?;
            }
            Ok(())
        },
    );
}

fn each_all<B: Checked + Doublets<usize>>(
    group: &mut BenchmarkGroup<WallTime>,
    id: &str,
    benched: &mut B,
) {
    measure(
        group,
        id,
        benched,
        Found::first(background_links()),
        |store| {
            let mut found = Found::default();
            store.each(|link| visit(&mut found, link));
            Ok(found)
        },
        read_only,
    );
}

/// Measures `N` queries `each_by(query(id))` for the ids `1..=N`.
fn each_by<B: Checked + Doublets<usize>>(
    group: &mut BenchmarkGroup<WallTime>,
    id: &str,
    benched: &mut B,
    query: impl Fn(usize) -> [usize; 3],
) {
    let links = benchmark_links();
    measure(
        group,
        id,
        benched,
        Found::first(links),
        |store| {
            let mut found = Found::default();
            for id in 1..=links {
                store.each_by(query(id), |link| visit(&mut found, link));
            }
            Ok(found)
        },
        read_only,
    );
}

fn each_identity<B: Checked + Doublets<usize>>(
    group: &mut BenchmarkGroup<WallTime>,
    id: &str,
    benched: &mut B,
) {
    let any = any();
    each_by(group, id, benched, |id| [id, any, any]);
}

fn each_concrete<B: Checked + Doublets<usize>>(
    group: &mut BenchmarkGroup<WallTime>,
    id: &str,
    benched: &mut B,
) {
    let any = any();
    each_by(group, id, benched, |id| [any, id, id]);
}

fn each_outgoing<B: Checked + Doublets<usize>>(
    group: &mut BenchmarkGroup<WallTime>,
    id: &str,
    benched: &mut B,
) {
    let any = any();
    each_by(group, id, benched, |id| [any, id, any]);
}

fn each_incoming<B: Checked + Doublets<usize>>(
    group: &mut BenchmarkGroup<WallTime>,
    id: &str,
    benched: &mut B,
) {
    let any = any();
    each_by(group, id, benched, |id| [any, any, id]);
}
