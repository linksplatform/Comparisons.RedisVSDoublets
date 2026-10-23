//! # Doublets Benched Implementations
//!
//! | Benchmark name             | Type                                                    | Storage |
//! |----------------------------|---------------------------------------------------------|---------|
//! | `Doublets_United_Volatile` | `unit::Store<T, Whole<Global<_>>>`                      | RAM     |
//! | `Doublets_Split_Volatile`  | `split::Store<T, Whole<Global<_>>, Whole<Global<_>>>`   | RAM     |
//!
//! The background links are created with `create_point()` and removed with
//! `delete_all()`.

use doublets::{Doublets, data::LinkReference};

use super::Benched;
use crate::{DoubletsSplitVolatile, DoubletsUnitedVolatile, Fork, memory::volatile};

/// Implements the lifecycle that is the same for all Doublets stores.
macro_rules! doublets_lifecycle {
    () => {
        fn fork(&mut self, background: usize) -> crate::Result<Fork<'_, Self>> {
            for _ in 0..background {
                self.create_point()?;
            }
            Ok(Fork(self))
        }

        fn unfork(&mut self) -> crate::Result<()> {
            Ok(self.delete_all()?)
        }
    };
}

impl<T: LinkReference> Benched for DoubletsUnitedVolatile<T> {
    type Builder<'a> = ();

    fn setup(_: Self::Builder<'_>) -> crate::Result<Self> {
        Ok(Self::new(volatile())?)
    }

    doublets_lifecycle!();
}

impl<T: LinkReference> Benched for DoubletsSplitVolatile<T> {
    type Builder<'a> = ();

    fn setup(_: Self::Builder<'_>) -> crate::Result<Self> {
        Ok(Self::new(volatile(), volatile())?)
    }

    doublets_lifecycle!();
}
