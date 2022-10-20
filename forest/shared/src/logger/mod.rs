// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::cli::LogValue;
use log::LevelFilter;
use std::str::FromStr;

pub fn setup_logger(log_config: &[LogValue]) {
    let mut logger_builder = pretty_env_logger::formatted_timed_builder();

    // Assign default log level settings
    logger_builder.filter(None, LevelFilter::Info);

    for item in log_config {
        let level = LevelFilter::from_str(item.level.as_str())
            .unwrap_or_else(|_| {
                eprintln!("Could not parse LevelFilter {}", item.level);
                std::process::exit(1)
            });
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
