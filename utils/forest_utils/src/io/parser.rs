// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

pub fn parse_duration(arg: &str) -> anyhow::Result<Duration> {
    let seconds = arg.parse()?;
    Ok(Duration::from_secs(seconds))
}
