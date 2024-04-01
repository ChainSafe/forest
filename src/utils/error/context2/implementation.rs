// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Context2;
use std::{convert::Infallible, fmt::Display};

impl<T> Context2<T, Infallible> for Option<T> {
    fn context<C>(self, context: C) -> anyhow::Result<T>
    where
        C: Display + Send + Sync + 'static,
    {
        self.ok_or_else(|| anyhow::anyhow!("{context}"))
    }

    fn with_context<C, F>(self, context: F) -> anyhow::Result<T>
    where
        C: Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        self.ok_or_else(|| anyhow::anyhow!("{}", context()))
    }
}

impl<T, E> Context2<T, E> for Result<T, E>
where
    E: Display,
{
    fn context<C>(self, context: C) -> anyhow::Result<T>
    where
        C: Display + Send + Sync + 'static,
    {
        self.map_err(|e| anyhow::anyhow!("{context}: {e}"))
    }

    fn with_context<C, F>(self, context: F) -> anyhow::Result<T>
    where
        C: Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        self.map_err(|e| anyhow::anyhow!("{}: {e}", context()))
    }
}
