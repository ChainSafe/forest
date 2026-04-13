// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

/// A trait for performing a lightweight clone of a type.
///
/// Implementations should clone only the outer wrapper and preserve any
/// shared internal state where appropriate (for example, `Arc<T>` clones the
/// pointer without cloning the inner value).
pub trait ShallowClone {
    /// Performs a lightweight clone.
    ///
    /// Implementations should clone only the outer wrapper and preserve any
    /// shared internal state where appropriate (for example, `Arc<T>` clones the
    /// pointer without cloning the inner value).
    fn shallow_clone(&self) -> Self;
}

impl<T> ShallowClone for Arc<T> {
    fn shallow_clone(&self) -> Self {
        self.clone()
    }
}

impl<T: ShallowClone> ShallowClone for Option<T> {
    fn shallow_clone(&self) -> Self {
        self.as_ref().map(ShallowClone::shallow_clone)
    }
}
