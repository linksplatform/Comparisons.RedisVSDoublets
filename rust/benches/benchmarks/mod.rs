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

use std::time::{Duration, Instant};

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

/// Measures `operation` on a store that contains the background data
/// ([`background_links`] links, or keys, queue values and set members).
///
/// The background data is created once, when Criterion first runs the
/// benchmark (so benchmarks skipped by a filter create no data). After every
/// iteration `undo` puts the store back into the same state, so every
/// iteration starts from the same data. Neither of them is measured.
pub(crate) fn measure<B: Benched>(
    group: &mut BenchmarkGroup<WallTime>,
    id: &str,
    benched: &mut B,
    mut operation: impl FnMut(&mut B) -> Result<()>,
    mut undo: impl FnMut(&mut B) -> Result<()>,
) {
    let mut benched = Some(benched);
    let mut store = None;
    let mut iteration = |store: &mut B| -> Result<Duration> {
        let started = Instant::now();
        store.begin()?;
        operation(store)?;
        store.commit()?;
        let elapsed = started.elapsed();

        store.begin()?;
        undo(store)?;
        store.commit()?;
        Ok(elapsed)
    };
    group.bench_function(id, |bencher| {
        let store = store.get_or_insert_with(|| {
            let benched = benched.take().expect("the store is forked only once");
            benched
                .fork(background_links())
                .expect("failed to create the background data")
        });
        bencher.iter_custom(|iterations| {
            (0..iterations)
                .map(|_| iteration(store).expect("benchmark iteration failed"))
                .sum()
        })
    });
}

/// Nothing to undo.
pub(crate) fn read_only<B>(_: &mut B) -> Result<()> {
    Ok(())
}
