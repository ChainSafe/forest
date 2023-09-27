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
            Unknown(UnknownErrorNumber),
        }

        $(
            static_assertions::const_assert_eq!(N2::$variant as u32, N3::$variant as u32);
        )*

        impl From<N2> for ErrorNumber {
            fn from(value: N2) -> Self {
                match value {
                    $(N2::$variant => Self::$variant,)*
                    u => Self::Unknown(UnknownErrorNumber::N2(u)),
                }
            }
        }

        impl From<N3> for ErrorNumber {
            fn from(value: N3) -> Self {
                match value {
                    $(N3::$variant => Self::$variant,)*
                    u => Self::Unknown(UnknownErrorNumber::N3(u)),
                }
            }
        }

        impl From<N4> for ErrorNumber {
            fn from(value: N4) -> Self {
                match value {
                    $(N4::$variant => Self::$variant,)*
                    u => Self::Unknown(UnknownErrorNumber::N4(u)),
                }
            }
        }

        impl ErrorNumber {
            fn as_unshimmed(&self) -> Either<N3, Either<N4, N2>> {
                match self {
                    $(Self::$variant => Either::Left(N3::$variant),)*
                    Self::Unknown(u) => u.as_unshimmed(),
                }
            }
        }
    };
}

#[derive(Debug, Clone)]
pub enum UnknownErrorNumber {
    N2(N2),
    N3(N3),
    N4(N4),
}

impl UnknownErrorNumber {
    fn as_unshimmed(&self) -> Either<N3, Either<N4, N2>> {
        match self {
            Self::N2(n) => Either::Right(Either::Right(*n)),
            Self::N3(n) => Either::Left(*n),
            Self::N4(n) => Either::Right(Either::Left(*n)),
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
}

impl fmt::Display for ErrorNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_unshimmed().fmt(f)
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
