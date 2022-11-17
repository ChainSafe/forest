// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use fvm_ipld_amt::Error as AmtError;
use fvm_ipld_encoding::Error as EncodingError;
use fvm_ipld_hamt::Error as HamtError;
use fvm_shared::error::ExitCode;

use crate::ActorError;

/// Trait to allow multiple error types to be able to be downcasted into an `ActorError`.
pub trait ActorDowncast {
    /// Downcast a dynamic std Error into an `ActorError`. If the error cannot be downcasted
    /// into an ActorError automatically, use the provided `ExitCode` to generate a new error.
    fn downcast_default(self, default_exit_code: ExitCode, msg: impl AsRef<str>) -> ActorError;

    /// Wrap the error with a message, without overwriting an exit code.
    fn downcast_wrap(self, msg: impl AsRef<str>) -> anyhow::Error;
}

impl ActorDowncast for anyhow::Error {
    fn downcast_default(self, default_exit_code: ExitCode, msg: impl AsRef<str>) -> ActorError {
        match downcast_util(self) {
            Ok(actor_error) => actor_error.wrap(msg),
            Err(other) => {
                ActorError::unchecked(default_exit_code, format!("{}: {}", msg.as_ref(), other))
            }
        }
    }
    fn downcast_wrap(self, msg: impl AsRef<str>) -> anyhow::Error {
        match downcast_util(self) {
            Ok(actor_error) => anyhow!(actor_error.wrap(msg)),
            Err(other) => anyhow!("{}: {}", msg.as_ref(), other),
        }
    }
}

impl ActorDowncast for AmtError {
    fn downcast_default(self, default_exit_code: ExitCode, msg: impl AsRef<str>) -> ActorError {
        match self {
            AmtError::Dynamic(e) => e.downcast_default(default_exit_code, msg),
            other => {
                ActorError::unchecked(default_exit_code, format!("{}: {}", msg.as_ref(), other))
            }
        }
    }
    fn downcast_wrap(self, msg: impl AsRef<str>) -> anyhow::Error {
        match self {
            AmtError::Dynamic(e) => e.downcast_wrap(msg),
            other => anyhow!("{}: {}", msg.as_ref(), other),
        }
    }
}

impl ActorDowncast for HamtError {
    fn downcast_default(self, default_exit_code: ExitCode, msg: impl AsRef<str>) -> ActorError {
        match self {
            HamtError::Dynamic(e) => e.downcast_default(default_exit_code, msg),
            other => {
                ActorError::unchecked(default_exit_code, format!("{}: {}", msg.as_ref(), other))
            }
        }
    }
    fn downcast_wrap(self, msg: impl AsRef<str>) -> anyhow::Error {
        match self {
            HamtError::Dynamic(e) => e.downcast_wrap(msg),
            other => anyhow!("{}: {}", msg.as_ref(), other),
        }
    }
}

/// Attempts to downcast a `Box<dyn std::error::Error>` into an actor error.
/// Returns `Ok` with the actor error if it can be downcasted automatically
/// and returns `Err` with the original error if it cannot.
fn downcast_util(error: anyhow::Error) -> anyhow::Result<ActorError> {
    // Check if error is ActorError, return as such
    let error = match error.downcast::<ActorError>() {
        Ok(actor_err) => return Ok(actor_err),
        Err(other) => other,
    };

    // Check if error is Encoding error, if so return `ErrSerialization`
    let error = match error.downcast::<EncodingError>() {
        Ok(enc_error) => {
            return Ok(ActorError::unchecked(ExitCode::USR_SERIALIZATION, enc_error.to_string()))
        }
        Err(other) => other,
    };

    // Dynamic errors can come from Array and Hamt through blockstore usages, check them.
    let error = match error.downcast::<AmtError>() {
        Ok(amt_err) => match amt_err {
            AmtError::Dynamic(de) => match downcast_util(de) {
                Ok(a) => return Ok(a),
                Err(other) => other,
            },
            other => anyhow!(other),
        },
        Err(other) => other,
    };
    let error = match error.downcast::<HamtError>() {
        Ok(amt_err) => match amt_err {
            HamtError::Dynamic(de) => match downcast_util(de) {
                Ok(a) => return Ok(a),
                Err(other) => other,
            },
            other => anyhow!(other),
        },
        Err(other) => other,
    };

    // Could not be downcasted automatically to actor error, return initial dynamic error.
    Err(error)
}
