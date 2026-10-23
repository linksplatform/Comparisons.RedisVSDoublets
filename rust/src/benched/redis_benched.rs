//! # Redis Benched Implementation
//!
//! | Benchmark name         | [`Mode`]                 | Commands of one iteration                      |
//! |------------------------|--------------------------|------------------------------------------------|
//! | `Redis_NonTransaction` | [`Mode::NonTransaction`] | one round trip per command                     |
//! | `Redis_Transaction`    | [`Mode::Transaction`]    | queued in `MULTI`/`EXEC`, sent at the commit   |
//! | `Redis_Pipeline`       | [`Mode::Pipeline`]       | pipelined, sent at the commit                  |
//!
//! The background links are created by the function `link_points`, 10,000
//! links per call, and removed with `FLUSHDB`.

use doublets::data::LinkReference;

use super::Benched;
use crate::{Fork, Mode, Redis, redis_url};

impl<T: LinkReference> Benched for Redis<T> {
    type Builder<'a> = Mode;

    fn setup(mode: Self::Builder<'_>) -> crate::Result<Self> {
        let mut store = Self::connect(&redis_url(), mode)?;
        store.clear()?;
        Ok(store)
    }

    fn fork(&mut self, background: usize) -> crate::Result<Fork<'_, Self>> {
        self.create_points(background)?;
        Ok(Fork(self))
    }

    fn begin(&mut self) -> crate::Result<()> {
        Redis::begin(self);
        Ok(())
    }

    fn commit(&mut self) -> crate::Result<()> {
        Redis::commit(self)
    }

    fn unfork(&mut self) -> crate::Result<()> {
        self.clear()
    }
}
