// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::ExitCode;
use encoding::Error as EncodingError;
use std::error::Error as StdError;
use thiserror::Error;

/// The error type that gets returned by actor method calls.
#[derive(Error, Debug, Clone, PartialEq)]
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
    pub fn new(exit_code: ExitCode, msg: String) -> Self {
        Self {
            fatal: false,
            exit_code,
            msg,
        }
    }

    pub fn new_fatal(msg: String) -> Self {
        Self {
            fatal: true,
            exit_code: ExitCode::ErrPlaceholder,
            msg,
        }
    }

    /// Downcast a dynamic std Error into an ActorError
    pub fn downcast(
        error: Box<dyn StdError>,
        default_exit_code: ExitCode,
        msg: impl AsRef<str>,
    ) -> Self {
        match error.downcast::<ActorError>() {
            Ok(actor_err) => actor_err.wrap(msg.as_ref()),
            Err(other) => match other.downcast::<EncodingError>() {
                Ok(enc_error) => ActorError::new(ExitCode::ErrSerialization, enc_error.to_string()),
                Err(other) => {
                    ActorError::new(default_exit_code, format!("{}: {}", msg.as_ref(), other))
                }
            },
        }
    }

    /// Downcast a dynamic std Error into a fatal ActorError
    pub fn downcast_fatal(error: Box<dyn StdError>, msg: impl AsRef<str>) -> Self {
        match error.downcast::<ActorError>() {
            Ok(actor_err) => actor_err.wrap(msg.as_ref()),
            Err(other) => match other.downcast::<EncodingError>() {
                Ok(enc_error) => ActorError::new(ExitCode::ErrSerialization, enc_error.to_string()),
                Err(other) => ActorError::new_fatal(format!("{}: {}", msg.as_ref(), other)),
            },
        }
    }

    /// Prefix a dynamic std Error with an error message.
    pub fn downcast_wrap(error: Box<dyn StdError>, msg: impl AsRef<str>) -> Box<dyn StdError> {
        match error.downcast::<ActorError>() {
            Ok(actor_err) => actor_err.wrap(msg.as_ref()).into(),
            Err(other) => match other.downcast::<EncodingError>() {
                Ok(enc_error) => {
                    ActorError::new(ExitCode::ErrSerialization, enc_error.to_string()).into()
                }
                Err(other) => format!("{}: {}", msg.as_ref(), other).into(),
            },
        }
    }

    /// Returns true if error is fatal.
    pub fn is_fatal(&self) -> bool {
        self.fatal
    }

    /// Returns the exit code of the error.
    pub fn exit_code(&self) -> ExitCode {
        self.exit_code
    }

    /// Returns true when the exit code is `Ok`.
    pub fn is_ok(&self) -> bool {
        self.exit_code == ExitCode::Ok
    }

    /// Error message of the actor error.
    pub fn msg(&self) -> &str {
        &self.msg
    }

    /// Prefix error message with a string message.
    pub fn wrap(mut self, msg: impl AsRef<str>) -> Self {
        self.msg = format!("{}: {}", msg.as_ref(), self.msg);
        self
    }
}

impl From<EncodingError> for ActorError {
    fn from(e: EncodingError) -> Self {
        Self {
            fatal: false,
            exit_code: ExitCode::ErrSerialization,
            msg: e.to_string(),
        }
    }
}

/// Convenience macro for generating Actor Errors
#[macro_export]
macro_rules! actor_error {
    // Fatal Errors
    ( fatal($msg:expr) ) => { ActorError::new_fatal($msg.to_string()) };
    ( fatal($msg:literal $(, $ex:expr)+) ) => {
        ActorError::new_fatal(format!($msg, $($ex,)*))
    };

    // Error with only one stringable expression
    ( $code:ident; $msg:expr ) => { ActorError::new(ExitCode::$code, $msg.to_string()) };

    // String with positional arguments
    ( $code:ident; $msg:literal $(, $ex:expr)+ ) => {
        ActorError::new(ExitCode::$code, format!($msg, $($ex,)*))
    };

    // Error with only one stringable expression, with comma separator
    ( $code:ident, $msg:expr ) => { actor_error!($code; $msg) };

    // String with positional arguments, with comma separator
    ( $code:ident, $msg:literal $(, $ex:expr)+ ) => {
        actor_error!($code; $msg $(, $ex)*)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_macro_generation() {
        assert_eq!(
            actor_error!(SysErrSenderInvalid; "test"),
            ActorError::new(ExitCode::SysErrSenderInvalid, "test".to_owned())
        );
        assert_eq!(
            actor_error!(SysErrSenderInvalid; "test {}, {}", 8, 10),
            ActorError::new(ExitCode::SysErrSenderInvalid, format!("test {}, {}", 8, 10))
        );
        assert_eq!(
            actor_error!(fatal("test {}, {}", 8, 10)),
            ActorError::new_fatal(format!("test {}, {}", 8, 10))
        );
    }
}
