// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use self::ErrorNumber as NShim;
use self::SyscallError as EShim;
use fvm2::kernel::SyscallError as E2;
use fvm3::kernel::SyscallError as E3;
use fvm4::kernel::SyscallError as E4;
use fvm_shared2::error::ErrorNumber as N2;
use fvm_shared3::error::ErrorNumber as N3;
use fvm_shared4::error::ErrorNumber as N4;
use itertools::Either;
use std::fmt;

macro_rules! error_number {
    ($($variant:ident),* $(,)?) => {
        #[derive(Debug, Clone)]
        #[non_exhaustive]
        pub enum ErrorNumber {
            $($variant,)*
            Unknown(u32),
        }

        $(
            static_assertions::const_assert_eq!(N2::$variant as u32, N3::$variant as u32);
            static_assertions::const_assert_eq!(N3::$variant as u32, N4::$variant as u32);
        )*

        impl From<N2> for ErrorNumber {
            fn from(value: N2) -> Self {
                match value {
                    $(N2::$variant => Self::$variant,)*
                    u => Self::Unknown(u as u32),
                }
            }
        }

        impl From<N3> for ErrorNumber {
            fn from(value: N3) -> Self {
                match value {
                    $(N3::$variant => Self::$variant,)*
                    u => Self::Unknown(u as u32),
                }
            }
        }

        impl From<N4> for ErrorNumber {
            fn from(value: N4) -> Self {
                match value {
                    $(N4::$variant => Self::$variant,)*
                    u => Self::Unknown(u as u32),
                }
            }
        }

        impl ErrorNumber {
            fn as_unshimmed_or_u32(&self) -> Either<N4, u32> {
                match self {
                    $(Self::$variant => Either::Left(N4::$variant),)*
                    Self::Unknown(u) => Either::Right(*u),
                }
            }
        }
    };
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
}

impl fmt::Display for ErrorNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.as_unshimmed_or_u32() {
            Either::Left(n4) => n4.fmt(f),
            Either::Right(u) => write!(f, "{}", u),
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
    fn test_error_fmt() {
        let shim = SyscallError {
            message: "cthulhu".into(),
            number: ErrorNumber::IllegalArgument,
        };

        assert_eq!(
            format!("{}", shim),
            "syscall error: cthulhu (exit_code=illegal argument)"
        );
    }
}
