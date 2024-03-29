// Copyright 2024 Ross Light
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use std::cell::UnsafeCell;
use std::fmt;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};

/// `LazyFailInit<T>` is a lazily initialized synchronized container type
/// where the initialization may occur as many times as needed to succeed,
/// but will be immutable once initialization is successful.
pub(crate) struct LazyFailInit<T> {
    initialized: AtomicBool,
    lock: Mutex<()>,
    value: UnsafeCell<Option<T>>,
}

impl<T> LazyFailInit<T> {
    /// Construct a new, uninitialized `LazyFailInit<T>`.
    #[inline]
    pub(crate) fn new() -> Self {
        LazyFailInit {
            initialized: AtomicBool::new(false),
            lock: Mutex::new(()),
            value: UnsafeCell::new(None),
        }
    }

    /// Get a reference to the contained value,
    /// invoking `f` to create it if the `LazyFailInit<T>` is uninitialized.
    /// At most one initialization function may run concurrently,
    /// and once the first function returns `Ok`,
    /// initialization functions will no longer be called.
    pub(crate) fn get_or_create<E>(&self, f: impl FnOnce() -> Result<T, E>) -> Result<&T, E> {
        if !self.initialized.load(Ordering::Acquire) {
            let _lock = self.lock.lock().unwrap();
            if !self.initialized.load(Ordering::Relaxed) {
                let value = unsafe { &mut *self.value.get() };
                match f() {
                    Ok(x) => {
                        *value = Some(x);
                        self.initialized.store(true, Ordering::Release);
                    }
                    Err(err) => return Err(err),
                }
            }
        }

        Ok(unsafe { self.extract() }.unwrap())
    }

    /// Get a reference to the contained value,
    /// returning `Some(ref)` if the `LazyFailInit<T>` has been initialized
    /// or `None` if it has not.
    pub(crate) fn get(&self) -> Option<&T> {
        if self.initialized.load(Ordering::Acquire) {
            unsafe { self.extract() }
        } else {
            let _lock = self.lock.lock().unwrap();
            unsafe { self.extract() }
        }
    }

    #[inline(always)]
    unsafe fn extract(&self) -> Option<&T> {
        (*self.value.get()).as_ref()
    }
}

unsafe impl<T> Sync for LazyFailInit<T> where T: Send + Sync {}

impl<T> fmt::Debug for LazyFailInit<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(x) = self.get() {
            f.write_fmt(format_args!("LazyFailInit({:?})", x))
        } else {
            f.write_str("LazyFailInit(<uninitialized>)")
        }
    }
}

impl<T> Default for LazyFailInit<T> {
    fn default() -> Self {
        LazyFailInit::new()
    }
}
