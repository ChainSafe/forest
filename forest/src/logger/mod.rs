// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_utils::{read_file_to_string, read_toml};
use log::LevelFilter;
use serde::{Deserialize, Serialize};
use std::{str::FromStr, path::PathBuf};
use crate::cli_error_and_die;

#[derive(Serialize, Deserialize)]
pub struct LogConfig {
    pub log_values: Vec<LogValue>
}

#[derive(Serialize, Deserialize)]
pub struct LogValue {
    pub module: String,
    pub level: String,
}

pub(crate) fn setup_logger(option_log_file_pathbuf: Option<PathBuf>) {
    let mut logger_builder = pretty_env_logger::formatted_timed_builder();

    match option_log_file_pathbuf {
        Some(log_file_pathbuf) => {
            let toml = read_file_to_string(&log_file_pathbuf).unwrap_or_else(|_| {
                cli_error_and_die(
                    format!("could not read log file at {:?}", log_file_pathbuf),
                    1,
                )
            });
            let log_config: LogConfig = read_toml(&toml).unwrap_or_else(|_| {
                cli_error_and_die(
                    format!("could not parse log file at {:?}", log_file_pathbuf),
                    1,
                )
            });
            for item in log_config.log_values {
                let level = LevelFilter::from_str(item.level.as_str()).unwrap_or_else(|_| {
                    cli_error_and_die(
                        format!("could not parse LevelFilter enum value"),
                        1,
                    )
                });
                logger_builder.filter(Some(item.module.as_str()), level);
            }
        },
        None => {
            // Assign default log level settings
            logger_builder.filter(Some("libp2p_gossipsub"), LevelFilter::Error);
            logger_builder.filter(Some("filecoin_proofs"), LevelFilter::Warn);
            logger_builder.filter(Some("storage_proofs_core"), LevelFilter::Warn);
            logger_builder.filter(Some("surf::middleware"), LevelFilter::Warn);
            logger_builder.filter(
                Some("bellperson::groth16::aggregate::verify"),
                LevelFilter::Warn,
            );
            logger_builder.filter(Some("tide"), LevelFilter::Warn);
            logger_builder.filter(Some("libp2p_bitswap"), LevelFilter::Warn);
            logger_builder.filter(Some("rpc"), LevelFilter::Info);
            logger_builder.filter(None, LevelFilter::Info);
        },
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
