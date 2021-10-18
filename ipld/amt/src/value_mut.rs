// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::ops::{Deref, DerefMut};

/// A wrapper around a mutable reference to a value in an Amt. Keeps track of whether
/// the value was mutated by setting a flag when `DerefMut::deref_mut` is called.
pub struct ValueMut<'a, V> {
    value: &'a mut V,
    value_mutated: bool,
}

impl<'a, V> ValueMut<'a, V> {
    pub fn new(value: &'a mut V) -> Self {
        Self {
            value,
            value_mutated: false,
        }
    }

    pub fn value_changed(&self) -> bool {
        self.value_mutated
    }

    /// Marks guard as unchanged. This should only be used when the value was updated but it is
    /// intended to remove it. Otherwise, this function would give unexpected behaviour on flush.
    #[cfg(feature = "go-interop")]
    pub fn mark_unchanged(&mut self) {
        self.value_mutated = false;
    }
}

impl<V> Deref for ValueMut<'_, V> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<V> DerefMut for ValueMut<'_, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value_mutated = true;
        &mut self.value
    }
}
