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
        match self {
            Self::IllegalArgument => f.write_str("illegal argument"),
            Self::IllegalOperation => f.write_str("illegal operation"),
            Self::LimitExceeded => f.write_str("limit exceeded"),
            Self::AssertionFailed => f.write_str("filecoin assertion failed"),
            Self::InsufficientFunds => f.write_str("insufficient funds"),
            Self::NotFound => f.write_str("resource not found"),
            Self::InvalidHandle => f.write_str("invalid ipld block handle"),
            Self::IllegalCid => f.write_str("illegal cid specification"),
            Self::IllegalCodec => f.write_str("illegal ipld codec"),
            Self::Serialization => f.write_str("serialization error"),
            Self::Forbidden => f.write_str("operation forbidden"),
            Self::BufferTooSmall => f.write_str("buffer too small"),
            Self::Unknown(u) => write!(f, "{}", u),
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
