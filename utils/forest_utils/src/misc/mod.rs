// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod either;
use std::str::FromStr;

pub use either::*;
mod logo;
pub use logo::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoggingColor {
    Always,
    Auto,
    Never,
}

impl LoggingColor {
    pub fn coloring_enabled(&self) -> bool {
        match self {
            LoggingColor::Auto => atty::is(atty::Stream::Stdout),
            LoggingColor::Always => true,
            LoggingColor::Never => false,
        }
    }
}

impl Default for LoggingColor {
    fn default() -> Self {
        Self::Auto
    }
}

impl FromStr for LoggingColor {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(LoggingColor::Auto),
            "always" => Ok(LoggingColor::Always),
            "never" => Ok(LoggingColor::Never),
            _ => Err(Self::Err::msg(
                "Invalid logging color output. Must be one of Auto, Always, Never",
            )),
        }
    }
}
