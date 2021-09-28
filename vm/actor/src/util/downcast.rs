// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use encoding::{error::Error as CborError, Error as EncodingError};
use ipld_amt::Error as AmtError;
use ipld_hamt::Error as HamtError;
use std::error::Error as StdError;
use vm::{ActorError, ExitCode};

/// Trait to allow multiple error types to be able to be downcasted into an `ActorError`.
pub trait ActorDowncast {
    /// Downcast a dynamic std Error into an `ActorError`. If the error cannot be downcasted
    /// into an ActorError automatically, use the provided `ExitCode` to generate a new error.
    fn downcast_default(self, default_exit_code: ExitCode, msg: impl AsRef<str>) -> ActorError;

    /// Downcast a dynamic std Error into an `ActorError`. If the error cannot be downcasted
    /// then it will convert the error into a fatal error.
    fn downcast_fatal(self, msg: impl AsRef<str>) -> ActorError;

    /// Wrap the error with a message, without overwriting an exit code.
    fn downcast_wrap(self, msg: impl AsRef<str>) -> Box<dyn StdError>;
}

impl ActorDowncast for Box<dyn StdError> {
    fn downcast_default(self, default_exit_code: ExitCode, msg: impl AsRef<str>) -> ActorError {
        match downcast_util(self) {
            Ok(actor_error) => actor_error.wrap(msg),
            Err(other) => {
                ActorError::new(default_exit_code, format!("{}: {}", msg.as_ref(), other))
            }
        }
    }
    fn downcast_fatal(self, msg: impl AsRef<str>) -> ActorError {
        match downcast_util(self) {
            Ok(actor_error) => actor_error.wrap(msg),
            Err(other) => ActorError::new_fatal(format!("{}: {}", msg.as_ref(), other)),
        }
    }
    fn downcast_wrap(self, msg: impl AsRef<str>) -> Box<dyn StdError> {
        match downcast_util(self) {
            Ok(actor_error) => Box::new(actor_error.wrap(msg)),
            Err(other) => format!("{}: {}", msg.as_ref(), other).into(),
        }
    }
}

impl ActorDowncast for AmtError {
    fn downcast_default(self, default_exit_code: ExitCode, msg: impl AsRef<str>) -> ActorError {
        match self {
            AmtError::Dynamic(e) => e.downcast_default(default_exit_code, msg),
            other => ActorError::new(default_exit_code, format!("{}: {}", msg.as_ref(), other)),
        }
    }
    fn downcast_fatal(self, msg: impl AsRef<str>) -> ActorError {
        match self {
            AmtError::Dynamic(e) => e.downcast_fatal(msg),
            other => ActorError::new_fatal(format!("{}: {}", msg.as_ref(), other)),
        }
    }
    fn downcast_wrap(self, msg: impl AsRef<str>) -> Box<dyn StdError> {
        match self {
            AmtError::Dynamic(e) => e.downcast_wrap(msg),
            other => format!("{}: {}", msg.as_ref(), other).into(),
        }
    }
}

impl ActorDowncast for HamtError {
    fn downcast_default(self, default_exit_code: ExitCode, msg: impl AsRef<str>) -> ActorError {
        match self {
            HamtError::Dynamic(e) => e.downcast_default(default_exit_code, msg),
            other => ActorError::new(default_exit_code, format!("{}: {}", msg.as_ref(), other)),
        }
    }
    fn downcast_fatal(self, msg: impl AsRef<str>) -> ActorError {
        match self {
            HamtError::Dynamic(e) => e.downcast_fatal(msg),
            other => ActorError::new_fatal(format!("{}: {}", msg.as_ref(), other)),
        }
    }
    fn downcast_wrap(self, msg: impl AsRef<str>) -> Box<dyn StdError> {
        match self {
            HamtError::Dynamic(e) => e.downcast_wrap(msg),
            other => format!("{}: {}", msg.as_ref(), other).into(),
        }
    }
}

/// Attempts to downcast a `Box<dyn std::error::Error>` into an actor error.
/// Returns `Ok` with the actor error if it can be downcasted automatically
/// and returns `Err` with the original error if it cannot.
fn downcast_util(error: Box<dyn StdError>) -> Result<ActorError, Box<dyn StdError>> {
    // Check if error is ActorError, return as such
    let error = match error.downcast::<ActorError>() {
        Ok(actor_err) => return Ok(*actor_err),
        Err(other) => other,
    };

    // Check if error is Encoding error, if so return `ErrSerialization`
    let error = match error.downcast::<EncodingError>() {
        Ok(enc_error) => {
            return Ok(ActorError::new(
                ExitCode::ErrSerialization,
                enc_error.to_string(),
            ))
        }
        Err(other) => other,
    };

    // Check also for Cbor error to be safe. All should be converted to EncodingError, but to
    // future proof.
    let error = match error.downcast::<CborError>() {
        Ok(enc_error) => {
            return Ok(ActorError::new(
                ExitCode::ErrSerialization,
                enc_error.to_string(),
            ))
        }
        Err(other) => other,
    };

    // Dynamic errors can come from Amt and Hamt through blockstore usages, check them.
    let error = match error.downcast::<AmtError>() {
        Ok(amt_err) => match *amt_err {
            AmtError::Dynamic(de) => match downcast_util(de) {
                Ok(a) => return Ok(a),
                Err(other) => other,
            },
            other => Box::new(other),
        },
        Err(other) => other,
    };
    let error = match error.downcast::<HamtError>() {
        Ok(amt_err) => match *amt_err {
            HamtError::Dynamic(de) => match downcast_util(de) {
                Ok(a) => return Ok(a),
                Err(other) => other,
            },
            other => Box::new(other),
        },
        Err(other) => other,
    };

    // Could not be downcasted automatically to actor error, return initial dynamic error.
    Err(error)
}
