#![deny(missing_docs)]
#![cfg_attr(feature = "unstable", feature(scoped))]

//! A stable version of `std::thread::scoped`
//!
//! ## Warning
//!
//! This is inherently unsafe if the `JoinGuard` is allowed to leak without being dropped.
//! See #24292 for more details.

use std::marker::PhantomData;
use std::thread::{spawn, JoinHandle, Thread};
use std::mem::{transmute, forget};

/// A RAII guard for that joins a scoped thread upon drop
///
/// # Panics
///
/// `JoinGuard` will panic on join or drop if its owned thread panics
#[must_use = "thread will be immediately joined if `JoinGuard` is not used"]
pub struct JoinGuard<'a, T: Send + 'a> {
    inner: Option<JoinHandle<T>>,
    _marker: PhantomData<&'a T>,
}

unsafe impl<'a, T: Send + 'a> Sync for JoinGuard<'a, T> {}

impl<'a, T: Send + 'a> JoinGuard<'a, T> {
    /// Provides the backing `Thread` object
    pub fn thread(&self) -> &Thread {
        &self.inner.as_ref().unwrap().thread()
    }

    /// Joins the guarded thread and returns its result
    ///
    /// # Panics
    ///
    /// `join()` will panic if the owned thread panics
    pub fn join(mut self) -> T {
        match self.inner.take().unwrap().join() {
            Ok(res) => res,
            Err(_) => panic!("child thread {:?} panicked", self.thread()),
        }
    }
}

/// Detaches a child thread from its guard
pub trait ScopedDetach {
    /// Detaches a child thread from its guard
    ///
    /// Note: Only valid for the 'static lifetime
    fn detach(self);
}

impl<T: Send + 'static> ScopedDetach for JoinGuard<'static, T> {
    fn detach(mut self) {
        let _ = self.inner.take();
    }
}

#[cfg(feature = "unstable")]
impl<T: Send + 'static> ScopedDetach for ::std::thread::JoinGuard<'static, T> {
    fn detach(self) {
        use std::mem::forget;

        forget(self);
    }
}

impl<'a, T: Send + 'a> Drop for JoinGuard<'a, T> {
    fn drop(&mut self) {
        self.inner.take().map(|v| if v.join().is_err() {
            panic!("child thread {:?} panicked", self.thread());
        });
    }
}

/// Spawns a new scoped thread
pub fn scoped<'a, T, F>(f: F) -> JoinGuard<'a, T> where
    T: Send + 'static, F: FnOnce() -> T, F: Send + 'a
{
    struct Sendable<T>(T);

    unsafe impl<T> Send for Sendable<T> { }

    unsafe {
        let mut b = Box::new(f);
        let b_ptr = Sendable(&mut *b as *mut F as *mut ());
        forget(b);

        JoinGuard {
            inner: Some(spawn(move || {
                transmute::<_, Box<F>>(b_ptr.0 as *mut F)()
            })),
            _marker: PhantomData,
        }
    }
}


#[cfg(test)]
mod tests {
    use std::thread::sleep_ms;
    use super::scoped;

    #[test]
    fn test_scoped_stack() {
        let mut a = 5;
        scoped(|| {
            sleep_ms(50);
            a = 2;
        }).join();
        assert_eq!(a, 2);
    }

    #[test]
    fn test_join_success() {
        assert!(scoped(move|| -> String {
            "Success!".to_string()
        }).join() == "Success!");
    }

    #[test]
    fn test_scoped_success() {
        let res = scoped(move|| -> String {
            "Success!".to_string()
        }).join();
        assert!(res == "Success!");
    }

    #[test]
    #[should_panic]
    fn test_scoped_panic() {
        scoped(|| panic!()).join();
    }

    #[test]
    #[should_panic]
    fn test_scoped_implicit_panic() {
        let _ = scoped(|| panic!());
    }
}
