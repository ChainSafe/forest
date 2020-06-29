// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use log::LevelFilter;

pub(crate) fn setup_logger() {
    let mut logger_builder = pretty_env_logger::formatted_timed_builder();
    if let Ok(s) = ::std::env::var("RUST_LOG") {
        // Set log level based on filters if set
        logger_builder.parse_filters(&s);
    } else {
        // If no ENV variable specified, default to info
        logger_builder.filter(Some("libp2p_gossipsub"), LevelFilter::Warn);
        logger_builder.filter(Some("surf::middleware"), LevelFilter::Warn);
        logger_builder.filter(None, LevelFilter::Info);
    }
    let logger = logger_builder.build();

    // Wrap Logger in async_log
    async_log::Logger::wrap(logger, || 0)
        .start(LevelFilter::Trace)
        .unwrap();
}
