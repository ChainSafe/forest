// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

#[allow(clippy::enum_variant_names)]
pub enum FormattingMode {
    /// mode to show data in `FIL` units
    /// in full accuracy
    /// E.g. 0.50023677980 `FIL`
    ExactFixed,
    /// mode to show data in `FIL` units
    /// with 4 significant digits
    /// E.g. 0.5002 `FIL`
    NotExactFixed,
    /// mode to show data in SI units
    /// in full accuracy
    /// E.g. 500.2367798 `milli FIL`
    ExactNotFixed,
    /// mode to show data in SI units
    /// with 4 significant digits
    /// E.g. ~500.2 milli `FIL`
    NotExactNotFixed,
}

pub fn parse_duration(arg: &str) -> anyhow::Result<Duration> {
    let seconds = arg.parse()?;
    Ok(Duration::from_secs(seconds))
}
