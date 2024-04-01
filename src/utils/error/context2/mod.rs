// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt::Display;

mod implementation;

pub trait Context2<T, E> {
    /// Wrap the error value with additional context.
    fn context<C>(self, context: C) -> anyhow::Result<T>
    where
        C: Display + Send + Sync + 'static;

    /// Wrap the error value with additional context that is evaluated lazily
    /// only once an error does occur.
    fn with_context<C, F>(self, f: F) -> anyhow::Result<T>
    where
        C: Display + Send + Sync + 'static,
        F: FnOnce() -> C;
}
