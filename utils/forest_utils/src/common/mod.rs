// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt::Display;

#[derive(Debug, thiserror::Error)]
pub struct AggregatedError<T: Display>(Vec<T>);

impl<T: Display> AggregatedError<T> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn push(&mut self, item: T) {
        self.0.push(item);
    }
}

impl<T: Display> Default for AggregatedError<T> {
    fn default() -> Self {
        Self(vec![])
    }
}

impl<T: Display> Display for AggregatedError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f, "Aggregated errors:")?;
        for e in self.0.iter() {
            writeln!(f, "  {e}")?;
        }
        Ok(())
    }
}
