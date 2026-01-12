// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::Context as _;
use std::ops::AddAssign;

#[derive(Default)]
pub struct Stats<T: num::Num + num::NumCast + Copy + PartialOrd + AddAssign + Default> {
    n: usize,
    sum: T,
}

impl<T> Stats<T>
where
    T: num::Num + num::NumCast + Copy + PartialOrd + AddAssign + Default,
{
    pub fn new() -> Self {
        Default::default()
    }

    /// Update the moments with the given value.
    pub fn update(&mut self, x: T) {
        self.sum += x;
        self.n += 1;
    }

    pub fn mean(&self) -> anyhow::Result<T> {
        if self.n == 0 {
            anyhow::bail!("not enough data");
        }
        let sum_f64: f64 = num::NumCast::from(self.sum).context("error casting T to f64")?;
        let n_f64: f64 = num::NumCast::from(self.n).context("error casting T to f64")?;
        let result: T = num::NumCast::from(sum_f64 / n_f64).context("error casting f64 to T")?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_mean() {
        let mut stats = Stats::new();
        stats.mean().unwrap_err();
        stats.update(10);
        assert_eq!(stats.mean().unwrap(), 10);
        stats.update(5);
        assert_eq!(stats.mean().unwrap(), 7);
        stats.update(3);
        assert_eq!(stats.mean().unwrap(), 6);
    }
}
