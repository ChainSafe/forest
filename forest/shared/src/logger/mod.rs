// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::cli::{cli_error_and_die, LogValue};
use atty::Stream;
use log::LevelFilter;
use pretty_env_logger::env_logger::WriteStyle;
use std::str::FromStr;

#[derive(Debug)]
pub enum LoggingColor {
    Always,
    Auto,
    Never,
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

impl From<LoggingColor> for WriteStyle {
    fn from(color: LoggingColor) -> WriteStyle {
        match color {
            LoggingColor::Always => WriteStyle::Always,
            LoggingColor::Auto => {
                if atty::is(Stream::Stdout) {
                    WriteStyle::Always
                } else {
                    WriteStyle::Never
                }
            }
            LoggingColor::Never => WriteStyle::Never,
        }
    }
}

pub fn setup_logger(log_config: &[LogValue], write_style: WriteStyle) {
    let mut logger_builder = pretty_env_logger::formatted_timed_builder();

    // Assign default log level settings
    logger_builder.filter(None, LevelFilter::Info);

    logger_builder.write_style(write_style);

    for item in log_config {
        let level = LevelFilter::from_str(item.level.as_str())
            .unwrap_or_else(|_| cli_error_and_die("could not parse LevelFilter enum value", 1));
        logger_builder.filter(Some(item.module.as_str()), level);
    }

    // Override log level based on filters if set
    if let Ok(s) = ::std::env::var("RUST_LOG") {
        logger_builder.parse_filters(&s);
    }

    let logger = logger_builder.build();

    // Wrap Logger in async_log
    async_log::Logger::wrap(logger, || 0)
        .start(LevelFilter::Trace)
        .unwrap();
}
