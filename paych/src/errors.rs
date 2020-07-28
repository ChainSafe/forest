// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use thiserror::Error;

// Payment Channel Errors
#[derive(Debug, PartialEq, Error)]
pub enum Error {
    #[error("Channel not tracked")]
    ChannelNotTracked,
    #[error("Already Tracking Channel")]
    DupChannelTracking,
    #[error("Address not found")]
    NoAddress,
    #[error("{0}")]
    Other(String),
}
