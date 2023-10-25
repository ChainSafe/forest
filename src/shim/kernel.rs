// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//! We have three goals for our error shims:
//! - preserve upstream error _numbers_.
//! - preserve upstream error messages.
//! - allow _matching_ on specific errors.
//!
//! There are a couple of things that make this difficult:
//! - `fvm_shared*::error::ErrorNumber` is `#[non_exhaustive]`
//! - ...
//!
//! We have designed with the following assumptions about the `fvm*` crates:
//! - new error variants are append-only
//! - error messages are consistent between crates

use self::ErrorNumber as NShim;
use self::SyscallError as EShim;
use fvm2::kernel::SyscallError as E2;
use fvm3::kernel::SyscallError as E3;
use fvm4::kernel::SyscallError as E4;
use fvm_shared2::error::ErrorNumber as N2;
use fvm_shared3::error::ErrorNumber as N3;
use fvm_shared4::error::ErrorNumber as N4;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use std::fmt;
use std::fmt::Debug;

macro_rules! error_number {
    ($($variant:ident),* $(,)?) => {
        #[derive(Debug, Clone)]
        pub enum ErrorNumber {
            $($variant,)*
            Unknown(u32),
        }

        #[repr(u32)]
        #[derive(Debug, Clone, FromPrimitive)]
        enum KnownErrorNumber {
            $($variant = N4::$variant as u32,)*
        }

        impl From<KnownErrorNumber> for ErrorNumber {
            fn from(error: KnownErrorNumber) -> Self {
                match error {
                    $(KnownErrorNumber::$variant => Self::$variant,)*
                }
            }
        }

        impl fmt::Display for ErrorNumber {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    $(Self::$variant => KnownErrorNumber::$variant.fmt(f),)*
                    Self::Unknown(u) => std::fmt::Debug::fmt(&u, f),
                }
            }
        }
    }
}

error_number! {
    IllegalArgument,
    IllegalOperation,
    LimitExceeded,
    AssertionFailed,
    InsufficientFunds,
    NotFound,
    InvalidHandle,
    IllegalCid,
    IllegalCodec,
    Serialization,
    Forbidden,
    BufferTooSmall,
    ReadOnly,
}

impl From<N2> for ErrorNumber {
    fn from(value: N2) -> Self {
        let opt: Option<KnownErrorNumber> = FromPrimitive::from_u32(value as u32);
        match opt {
            Some(err) => err.into(),
            None => Self::Unknown(value as u32),
        }
    }
}

impl From<N3> for ErrorNumber {
    fn from(value: N3) -> Self {
        let opt: Option<KnownErrorNumber> = FromPrimitive::from_u32(value as u32);
        match opt {
            Some(err) => err.into(),
            None => Self::Unknown(value as u32),
        }
    }
}

impl From<N4> for ErrorNumber {
    fn from(value: N4) -> Self {
        let opt: Option<KnownErrorNumber> = FromPrimitive::from_u32(value as u32);
        match opt {
            Some(err) => err.into(),
            None => Self::Unknown(value as u32),
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("syscall error: {message} (exit_code={number})")]
pub struct SyscallError {
    pub message: String,
    pub number: NShim,
}

impl From<E2> for EShim {
    fn from(value: E2) -> Self {
        let E2(message, number) = value;
        Self {
            message,
            number: number.into(),
        }
    }
}

impl From<E3> for EShim {
    fn from(value: E3) -> Self {
        let E3(message, number) = value;
        Self {
            message,
            number: number.into(),
        }
    }
}

impl From<E4> for EShim {
    fn from(value: E4) -> Self {
        let E4(message, number) = value;
        Self {
            message,
            number: number.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_n2_error_fmt() {
        let shim: NShim = N2::IllegalArgument.into();

        assert_eq!(format!("{}", shim), "IllegalArgument");
    }

    #[test]
    fn test_unknown_error_fmt() {
        let shim: NShim = ErrorNumber::Unknown(23);

        assert_eq!(format!("{}", shim), "23");
    }
}
