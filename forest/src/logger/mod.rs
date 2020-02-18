// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT


pub(crate) fn setup_logger() {
    let logger = pretty_env_logger::formatted_timed_builder()
        .filter(None, log::LevelFilter::Debug)
        .build();
    async_log::Logger::wrap(logger, || 0)
        .start(log::LevelFilter::Debug)
        .unwrap();
}
