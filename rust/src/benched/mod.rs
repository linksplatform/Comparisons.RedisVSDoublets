//! # Benchmark Lifecycle
//!
//! This module defines the [`Benched`] trait, which prepares a store for a
//! benchmark. Both Redis and Doublets implement it:
//!
//! - **[`doublets_benched`]** - Doublets stores
//! - **[`redis_benched`]** - Redis

mod doublets_benched;
mod redis_benched;

use crate::Fork;

/// A store that can be benchmarked.
///
/// A benchmark of one operation is:
/// 1. [`Benched::fork`] - not measured: create the background data
/// 2. for every iteration:
///    - [`Benched::begin`], the operation, [`Benched::commit`] - measured
///    - undo the changes of the operation - not measured
/// 3. [`Benched::unfork`] - not measured: remove all data
pub trait Benched: Sized {
    /// Parameters needed to construct this store.
    type Builder<'params>;

    /// Opens the store and removes all data from it.
    fn setup(builder: Self::Builder<'_>) -> crate::Result<Self>;

    /// Creates the background data in the empty store: for the links stores
    /// the point links `1..=background` (each with `id = source = target`),
    /// for the [`Usage`](crate::Usage) stores the data described by
    /// [`Snapshot::background`](crate::Snapshot::background).
    ///
    /// The returned [`Fork`] removes all data again when it is dropped.
    fn fork(&mut self, background: usize) -> crate::Result<Fork<'_, Self>>;

    /// Starts the work of one iteration. Only Redis in the transaction and
    /// pipeline modes queues the commands from here on.
    fn begin(&mut self) -> crate::Result<()> {
        Ok(())
    }

    /// Finishes the work started by [`Benched::begin`]: Redis in the
    /// transaction and pipeline modes sends the queued commands and reads
    /// their replies. For all other stores every operation is already
    /// complete when it returns.
    fn commit(&mut self) -> crate::Result<()> {
        Ok(())
    }

    /// Removes all data.
    fn unfork(&mut self) -> crate::Result<()>;
}
