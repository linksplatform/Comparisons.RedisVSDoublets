//! # Comparisons.RedisVSDoublets
//!
//! This crate measures how long the same work takes when the data is kept in
//! Redis and when it is kept in an in-memory Doublets store, in two kinds of
//! benchmarks.
//!
//! ## Links
//!
//! Both databases store links `(id, source, target)` and implement the same
//! [`Doublets`](doublets::Doublets) trait, and every benchmark calls the same
//! trait methods on every backend:
//!
//! - **[`redis_impl`]** - how Redis implements each operation (native commands
//!   and the functions of `redis/links.lua`, through redis-rs)
//! - **[`doublets_impl`]** - how Doublets implements each operation (direct
//!   access to its in-process data structures)
//!
//! | Operation       | Method                  | Description                                    |
//! |-----------------|-------------------------|------------------------------------------------|
//! | Create          | `create_point()`        | Insert a point link (id = source = target)     |
//! | Update          | `update(id, src, tgt)`  | Modify source and target of existing link      |
//! | Delete          | `delete(id)`            | Remove a link by its ID                        |
//! | Each All        | `each(handler)`         | Iterate all links matching `[*, *, *]`         |
//! | Each Identity   | `each_by([id,*,*], h)`  | Find links by ID constraint                    |
//! | Each Concrete   | `each_by([*,s,t], h)`   | Find links by source AND target                |
//! | Each Outgoing   | `each_by([*,s,*], h)`   | Find links by source (outgoing edges)          |
//! | Each Incoming   | `each_by([*,*,t], h)`   | Find links by target (incoming edges)          |
//!
//! ## Typical Redis usage
//!
//! The data structures that Redis is typically used for (a key-value cache
//! with counters, a queue and a set) behind the [`Usage`] trait: Redis keeps
//! them in its native types, and Doublets reimplements them in links (see
//! [`usage`]).
//!
//! ## Benchmarked Implementations
//!
//! | Implementation             | Backend                                   | A finished operation is              |
//! |----------------------------|-------------------------------------------|--------------------------------------|
//! | `Doublets_United_Volatile` | in-process, RAM, unit store               | in RAM, in the benchmark process     |
//! | `Doublets_Split_Volatile`  | in-process, RAM, split store (links only) | in RAM, in the benchmark process     |
//! | `Redis_NonTransaction`     | Redis server, one round trip per command  | in the server's RAM                  |
//! | `Redis_Transaction`        | Redis server, `MULTI`/`EXEC` per iteration | in the server's RAM at `EXEC`       |
//! | `Redis_Pipeline`           | Redis server, one pipeline per iteration  | in the server's RAM at the commit    |
//!
//! Redis keeps its data in RAM as well; whether it also writes it to disk
//! depends on the server's configuration, which the benchmarks do not change
//! (the official Docker image saves RDB snapshots in the background and has
//! no append-only file).
//!
//! ## How the Benchmark Works
//!
//! For every operation and store:
//! 1. [`Benched::fork`] creates the background data (not measured)
//! 2. every iteration runs the operation [`benchmark_links`] times between
//!    [`Benched::begin`] and [`Benched::commit`] (measured), and then undoes
//!    its changes (not measured), so all iterations start from the same data
//! 3. [`Benched::unfork`] removes all data (not measured)

use std::{env, error, result};

pub use benched::Benched;
use doublets::{
    mem::Global,
    split::{self, DataPart, IndexPart},
    unit::{self, LinkPart},
};
pub use fork::Fork;
pub use memory::Whole;
pub use redis_impl::{Mode, Redis};
pub use usage::{DoubletsUsage, RedisUsage, Snapshot, Usage};

mod benched;
mod commands;
pub mod doublets_impl;
mod fork;
pub mod memory;
pub mod redis_impl;
pub mod usage;

pub type Result<T, E = Box<dyn error::Error + Sync + Send>> = result::Result<T, E>;

/// Number of point links (and keys, queue values and set members) that exist
/// before the measured operations start.
///
/// Configurable via the `BENCHMARK_BACKGROUND_LINKS` environment variable.
pub fn background_links() -> usize {
    env_usize("BENCHMARK_BACKGROUND_LINKS", 1000)
}

/// Number of links (or keys, values and members) that each benchmark
/// iteration creates, updates, deletes or looks up.
///
/// Configurable via the `BENCHMARK_LINKS` environment variable. Update and
/// Delete work on existing background links, so this value must not exceed
/// [`background_links`].
pub fn benchmark_links() -> usize {
    let links = env_usize("BENCHMARK_LINKS", 100);
    assert!(
        links <= background_links(),
        "BENCHMARK_LINKS ({links}) must not exceed BENCHMARK_BACKGROUND_LINKS ({})",
        background_links()
    );
    links
}

/// The Redis server of the benchmarks and the tests: the `REDIS_URL`
/// environment variable, by default `redis://localhost:6379`.
///
/// The benchmarks and the tests remove all keys of its database
/// (`FLUSHDB`), so it must not hold any other data.
pub fn redis_url() -> String {
    env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_owned())
}

fn env_usize(name: &str, default: usize) -> usize {
    match env::var(name) {
        Ok(value) => value
            .parse()
            .unwrap_or_else(|_| panic!("{name} must be a non-negative integer, got `{value}`")),
        Err(_) => default,
    }
}

// ============================================================================
// BENCHMARKED STORAGE TYPE ALIASES
// ============================================================================

/// Doublets United (unit) store in RAM.
///
/// Each array element holds `(source, target)` of one link together with its
/// index tree nodes; the id is the position in the array.
pub type DoubletsUnitedVolatile<T = usize> = unit::Store<T, Whole<Global<LinkPart<T>>>>;

/// Doublets Split store in RAM.
///
/// Data (`source`, `target`) and index trees are kept in separate memory
/// regions.
pub type DoubletsSplitVolatile<T = usize> =
    split::Store<T, Whole<Global<DataPart<T>>>, Whole<Global<IndexPart<T>>>>;
