//! Module for defining a `no_std` compatible `Parallel` thread local.
#![allow(dead_code)]

use alloc::vec::Vec;
use core::ops::DerefMut;

/// A no_std-compatible version of bevy's `Parallel`.
#[derive(Default)]
pub struct PortableParallel<T: Send>(
    #[cfg(feature = "std")] bevy_utils::Parallel<T>,
    #[cfg(not(feature = "std"))] core::cell::RefCell<Option<T>>,
);

/// A scope guard of a `Parallel`, when this struct is dropped ,the value will writeback to its `Parallel`
impl<T: Send> PortableParallel<T> {
    /// Gets a mutable iterator over all of the per-thread queues.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &'_ mut T> {
        self.0.iter_mut()
    }

    /// Clears all of the stored thread local values.
    pub fn clear(&mut self) {
        #[cfg(feature = "std")]
        self.0.clear();
        #[cfg(not(feature = "std"))]
        {
            *self.0.borrow_mut() = None;
        }
    }
}

impl<T: Default + Send> PortableParallel<T> {
    /// Retrieves the thread-local value for the current thread and runs `f` on it.
    ///
    /// If there is no thread-local value, it will be initialized to its default.
    pub fn scope<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        #[cfg(feature = "std")]
        let ret = self.0.scope(f);
        #[cfg(not(feature = "std"))]
        let ret = {
            let cell = self.0.borrow_mut().deref_mut();
            let value = cell.get_or_insert_default();
            f(value)
        };
        ret
    }

    /// Mutably borrows the thread-local value.
    ///
    /// If there is no thread-local value, it will be initialized to it's default.
    pub fn borrow_local_mut(&self) -> impl DerefMut<Target = T> + '_ {
        #[cfg(feature = "std")]
        let ret = self.0.borrow_local_mut();
        #[cfg(not(feature = "std"))]
        let ret = self.0.borrow_mut();
        ret
    }
}

impl<T, I> PortableParallel<I>
where
    I: IntoIterator<Item = T> + Default + Send + 'static,
{
    /// Drains all enqueued items from all threads and returns an iterator over them.
    ///
    /// Unlike [`Vec::drain`], this will piecemeal remove chunks of the data stored.
    /// If iteration is terminated part way, the rest of the enqueued items in the same
    /// chunk will be dropped, and the rest of the undrained elements will remain.
    ///
    /// The ordering is not guaranteed.
    pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
        #[cfg(feature = "std")]
        let ret = self.0.drain();
        #[cfg(not(feature = "std"))]
        let ret = self.0.borrow_mut().take();
        ret
    }
}

impl<T: Send> PortableParallel<Vec<T>> {
    /// Collect all enqueued items from all threads and appends them to the end of a
    /// single Vec.
    ///
    /// The ordering is not guaranteed.
    pub fn drain_into(&mut self, out: &mut Vec<T>) {
        #[cfg(feature = "std")]
        self.0.drain_into(out);
        #[cfg(not(feature = "std"))]
        out.extend(self.drain());
    }
}
