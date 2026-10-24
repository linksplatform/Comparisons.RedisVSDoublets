use std::ops::{Deref, DerefMut, Drop};

use crate::Benched;

/// A store that contains the background links created by [`Benched::fork`].
///
/// When the fork is dropped, the store is cleaned up by [`Benched::unfork`].
pub struct Fork<'f, B: Benched>(pub(crate) &'f mut B);

impl<B: Benched> Deref for Fork<'_, B> {
    type Target = B;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<B: Benched> DerefMut for Fork<'_, B> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

impl<B: Benched> Drop for Fork<'_, B> {
    fn drop(&mut self) {
        let result = self.0.unfork();
        // A second panic while the benchmark is already failing would abort
        // the process and hide the original error.
        if !std::thread::panicking() {
            result.expect("failed to remove the links after a benchmark");
        }
    }
}
