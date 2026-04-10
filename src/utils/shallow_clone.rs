// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

pub trait ShallowClone {
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
