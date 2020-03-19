// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use thiserror::Error;

use crate::ExitCode;

/// The error type that gets returned by actor method calls.
#[derive(Error, Debug)]
#[error("ActorError(fatal: {fatal}, exit_code: {exit_code:?}, msg: {msg})")]
pub struct ActorError {
    /// Is this a fatal error.
    fatal: bool,
    /// The exit code for this invocation, must not be `0`.
    exit_code: ExitCode,
    /// Message for debugging purposes,
    msg: String,
}

impl ActorError {
    pub fn is_fatal(&self) -> bool {
        self.fatal
    }

    pub fn exit_code(&self) -> ExitCode {
        self.exit_code
    }

    pub fn msg(&self) -> &str {
        &self.msg
    }
}
