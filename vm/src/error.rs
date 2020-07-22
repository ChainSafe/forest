// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use encoding::Error as EncodingError;
use thiserror::Error;

use crate::ExitCode;

/// The error type that gets returned by actor method calls.
#[derive(Error, Debug, PartialEq)]
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
