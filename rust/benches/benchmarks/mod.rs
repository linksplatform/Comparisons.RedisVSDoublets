//! # Benchmark Implementations
//!
//! The operations are written once and run unchanged on every backend, so
//! Redis and Doublets execute exactly the same sequence of calls:
//!
//! - **[`links`]** - the links storage task, through the
//!   [`Doublets`](doublets::Doublets) trait
//! - **[`usage`]** - typical Redis usage, through the
//!   [`Usage`](redis_vs_doublets::Usage) trait
//!
//! | Benchmark name             | Store                                                  |
//! |----------------------------|--------------------------------------------------------|
//! | `Redis_NonTransaction`     | Redis, one round trip per command                      |
//! | `Redis_Transaction`        | Redis, the commands of an iteration in `MULTI`/`EXEC`  |
//! | `Redis_Pipeline`           | Redis, the commands of an iteration pipelined          |
//! | `Doublets_United_Volatile` | Doublets unit store in RAM                             |
//! | `Doublets_Split_Volatile`  | Doublets split store in RAM (links only)               |
//!
//! The measured time of an iteration starts before [`Benched::begin`] and ends
//! after [`Benched::commit`], so for Redis in the transaction and pipeline
//! modes it includes sending the queued commands and reading all replies.
//!
//! Every benchmark is checked outside of the measured time, so a store that
//! loses data or finds wrong results fails the run instead of reporting a
//! time: before the first and after the last iteration the store must contain
//! exactly the background data, and one iteration before the measurement must
//! find the expected results ([`Found`]).

use std::{
    hint::black_box,
    ops::Add,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use criterion::{BenchmarkGroup, Criterion, SamplingMode, measurement::WallTime};
use redis_vs_doublets::{Benched, Result, background_links};

pub mod links;
pub mod usage;

/// Runs all benchmarks on Redis.
pub fn redis(c: &mut Criterion) {
    links::redis(c);
    usage::redis(c);
}

/// Runs all benchmarks on Doublets.
pub fn doublets(c: &mut Criterion) {
    links::doublets(c);
    usage::doublets(c);
}

/// Creates a benchmark group for Redis.
///
/// One Redis iteration waits for at least one network round trip, so it takes
/// from tens of microseconds to milliseconds; the minimum number of samples
/// (10) already gives stable results, and flat sampling avoids the many extra
/// iterations of linear sampling.
pub fn redis_group<'a>(c: &'a mut Criterion, name: &str) -> BenchmarkGroup<'a, WallTime> {
    let mut group = c.benchmark_group(name);
    group
        .sample_size(10)
        .sampling_mode(SamplingMode::Flat)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(5));
    group
}

/// Creates a benchmark group for Doublets.
///
/// One Doublets iteration takes microseconds, so Criterion's default number
/// of samples (100) is used to measure it precisely.
pub fn doublets_group<'a>(c: &'a mut Criterion, name: &str) -> BenchmarkGroup<'a, WallTime> {
    let mut group = c.benchmark_group(name);
    group
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(2));
    group
}

/// The results that an iteration found: how many links or values, and the
/// sum of their numbers (link ids or values).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Found {
    pub count: u64,
    pub sum: u64,
}

impl Found {
    /// The results `1, 2, ..., n`.
    pub fn first(n: usize) -> Self {
        let n = n as u64;
        Self {
            count: n,
            sum: n * (n + 1) / 2,
        }
    }

    pub fn add(&mut self, number: u64) {
        self.count += 1;
        self.sum = self.sum.wrapping_add(number);
    }
}

impl Add for Found {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            count: self.count + other.count,
            sum: self.sum.wrapping_add(other.sum),
        }
    }
}

/// Operations whose results only reach the sink of the store.
impl From<()> for Found {
    fn from((): ()) -> Self {
        Self::default()
    }
}

/// The results that reach the sink of a store while it is checked.
pub(crate) type SinkResults = Arc<Mutex<Found>>;

/// A store whose benchmarks [`measure`] can check.
pub(crate) trait Checked: Benched {
    /// Adds the results passed to the sink of the store to `results`, or, with
    /// `None`, passes them to [`black_box`] as by default. A store without a
    /// sink passes all results to the handlers of the operations.
    fn count_sink(&mut self, results: Option<SinkResults>);

    /// Panics unless the store contains exactly the background data.
    fn check_background(&mut self, background: usize) -> Result<()>;
}

/// Measures `operation` on a store that contains the background data
/// ([`background_links`] links, or keys, queue values and set members).
///
/// The background data is created once, when Criterion first runs the
/// benchmark (so benchmarks skipped by a filter create no data). After every
/// iteration `undo` puts the store back into the same state, so every
/// iteration starts from the same data. Neither of them is measured.
///
/// `operation` returns the results that reached its handlers; with the
/// results of the sink they must be `expected` (see [`check`]).
pub(crate) fn measure<B: Checked, R: Into<Found>>(
    group: &mut BenchmarkGroup<WallTime>,
    id: &str,
    benched: &mut B,
    expected: Found,
    mut operation: impl FnMut(&mut B) -> Result<R>,
    mut undo: impl FnMut(&mut B) -> Result<()>,
) {
    let background = background_links();
    let mut benched = Some(benched);
    let mut store = None;
    let mut iteration = |store: &mut B| -> Result<(Duration, Found)> {
        let started = Instant::now();
        store.begin()?;
        let found = black_box(operation(store)?);
        store.commit()?;
        let elapsed = started.elapsed();

        store.begin()?;
        undo(store)?;
        store.commit()?;
        Ok((elapsed, found.into()))
    };
    group.bench_function(id, |bencher| {
        let store = store.get_or_insert_with(|| {
            let benched = benched.take().expect("the store is forked only once");
            let mut store = benched
                .fork(background)
                .expect("failed to create the background data");
            check(&mut *store, id, expected, &mut iteration).expect("failed to check the store");
            store
        });
        bencher.iter_custom(|iterations| {
            (0..iterations)
                .map(|_| iteration(store).expect("benchmark iteration failed").0)
                .sum()
        })
    });
    if let Some(store) = store.as_mut() {
        store
            .check_background(background)
            .expect("failed to check the store");
    }
}

/// Checks that the store contains the background data before and after one
/// unmeasured `iteration`, which must find the `expected` results.
fn check<B: Checked>(
    store: &mut B,
    id: &str,
    expected: Found,
    iteration: &mut impl FnMut(&mut B) -> Result<(Duration, Found)>,
) -> Result<()> {
    let background = background_links();
    store.check_background(background)?;
    let sink = SinkResults::default();
    store.count_sink(Some(sink.clone()));
    let (_, found) = iteration(store)?;
    store.count_sink(None);
    let found = found + *sink.lock().expect("sink results mutex is poisoned");
    assert_eq!(found, expected, "{id} found wrong results");
    store.check_background(background)
}

/// Nothing to undo.
pub(crate) fn read_only<B>(_: &mut B) -> Result<()> {
    Ok(())
}
