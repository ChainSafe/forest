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
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use std::fmt;

impl From<N2> for ErrorNumber {
    fn from(value: N2) -> Self {
        FromPrimitive::from_u32(value as u32).expect("conversion from N2 must work")
    }
}

impl From<N3> for ErrorNumber {
    fn from(value: N3) -> Self {
        FromPrimitive::from_u32(value as u32).expect("conversion from N3 must work")
    }
}

impl From<N4> for ErrorNumber {
    fn from(value: N4) -> Self {
        FromPrimitive::from_u32(value as u32).expect("conversion from N4 must work")
    }
}

#[repr(u32)]
#[derive(Copy, Clone, Debug, FromPrimitive)]
pub enum ErrorNumber {
    IllegalArgument = 1,
    IllegalOperation = 2,
    LimitExceeded = 3,
    AssertionFailed = 4,
    InsufficientFunds = 5,
    NotFound = 6,
    InvalidHandle = 7,
    IllegalCid = 8,
    IllegalCodec = 9,
    Serialization = 10,
    Forbidden = 11,
    BufferTooSmall = 12,
    ReadOnly = 13,
}

impl fmt::Display for ErrorNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n4: N4 = FromPrimitive::from_u32(*self as u32).expect("conversion to N4 must work");
        n4.fmt(f)
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

        assert_eq!(format!("{}", shim), "illegal argument");
    }

    #[test]
    fn test_n3_error_fmt() {
        let shim: NShim = N3::ReadOnly.into();

        assert_eq!(format!("{}", shim), "execution context is read-only");
    }
}
