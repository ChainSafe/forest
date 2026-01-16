// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use fvm_shared2::error::ExitCode as ExitCodeV2;
use fvm_shared3::error::ExitCode as ExitCodeV3;
use fvm_shared4::error::ExitCode as ExitCodeV4;
use fvm_shared4::error::ExitCode as ExitCode_latest;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;

/// `Newtype` wrapper for the FVM `ExitCode`.
///
/// # Examples
/// ```
/// # use forest::doctest_private::ExitCode;
/// let fvm2_success = fvm_shared2::error::ExitCode::new(0);
/// let fvm3_success = fvm_shared3::error::ExitCode::new(0);
///
/// let shim_from_v2 = ExitCode::from(fvm2_success);
/// let shim_from_v3 = ExitCode::from(fvm3_success);
///
/// assert_eq!(shim_from_v2, shim_from_v3);
/// assert_eq!(shim_from_v2, fvm2_success.into());
/// assert_eq!(shim_from_v3, fvm3_success.into());
/// ```
#[derive(
    PartialEq,
    Eq,
    Debug,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    JsonSchema,
    derive_more::From,
    derive_more::Into,
)]
pub struct ExitCode(#[schemars(with = "u32")] ExitCodeV4);

impl PartialOrd for ExitCode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.value().cmp(&other.value()))
    }
}

impl fmt::Display for ExitCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.0 {
            ExitCode_latest::SYS_SENDER_INVALID => Some("SysErrSenderInvalid"),
            ExitCode_latest::SYS_SENDER_STATE_INVALID => Some("SysErrSenderStateInvalid"),
            ExitCode_latest::SYS_ILLEGAL_INSTRUCTION => Some("SysErrIllegalInstruction"),
            ExitCode_latest::SYS_INVALID_RECEIVER => Some("SysErrInvalidReceiver"),
            ExitCode_latest::SYS_INSUFFICIENT_FUNDS => Some("SysErrInsufficientFunds"),
            ExitCode_latest::SYS_OUT_OF_GAS => Some("SysErrOutOfGas"),
            ExitCode_latest::SYS_ILLEGAL_EXIT_CODE => Some("SysErrIllegalExitCode"),
            ExitCode_latest::SYS_ASSERTION_FAILED => Some("SysFatal"),
            ExitCode_latest::SYS_MISSING_RETURN => Some("SysErrMissingReturn"),

            ExitCode_latest::USR_ILLEGAL_ARGUMENT => Some("ErrIllegalArgument"),
            ExitCode_latest::USR_NOT_FOUND => Some("ErrNotFound"),
            ExitCode_latest::USR_FORBIDDEN => Some("ErrForbidden"),
            ExitCode_latest::USR_INSUFFICIENT_FUNDS => Some("ErrInsufficientFunds"),
            ExitCode_latest::USR_ILLEGAL_STATE => Some("ErrIllegalState"),
            ExitCode_latest::USR_SERIALIZATION => Some("ErrSerialization"),
            ExitCode_latest::USR_UNHANDLED_MESSAGE => Some("ErrUnhandledMessage"),
            ExitCode_latest::USR_UNSPECIFIED => Some("ErrUnspecified"),
            ExitCode_latest::USR_ASSERTION_FAILED => Some("ErrAssertionFailed"),
            ExitCode_latest::USR_READ_ONLY => Some("ErrReadOnly"),
            ExitCode_latest::USR_NOT_PAYABLE => Some("ErrNotPayable"),

            _ => None,
        };
        if let Some(name) = name {
            write!(f, "{}({})", name, self.value())
        } else {
            match self.value() {
                code if code > ExitCode_latest::SYS_MISSING_RETURN.value()
                    && code < ExitCode_latest::FIRST_USER_EXIT_CODE =>
                {
                    // We want to match Lotus display exit codes
                    // See <https://github.com/filecoin-project/go-state-types/blob/v0.15.0/exitcode/names.go#L16-L21>
                    write!(
                        f,
                        "SysErrReserved{}({})",
                        code - (ExitCode_latest::SYS_ASSERTION_FAILED.value()),
                        code
                    )
                }
                _ => write!(f, "{}", self.value()),
            }
        }
    }
}

impl ExitCode {
    /// The lowest exit code that an actor may abort with.
    pub const FIRST_USER_EXIT_CODE: u32 = ExitCode_latest::FIRST_USER_EXIT_CODE;

    /// Message execution (including sub-calls) used more gas than the specified limit.
    pub const SYS_OUT_OF_GAS: Self = Self::new(ExitCode_latest::SYS_OUT_OF_GAS);

    /// The message sender didn't have the requisite funds.
    pub const SYS_INSUFFICIENT_FUNDS: Self = Self::new(ExitCode_latest::SYS_INSUFFICIENT_FUNDS);

    /// The initial range of exit codes is reserved for system errors.
    /// Actors may define codes starting with this one.
    pub const FIRST_ACTOR_ERROR_CODE: u32 = 16;

    pub fn value(&self) -> u32 {
        self.0.value()
    }

    pub fn is_success(&self) -> bool {
        self.0.is_success()
    }

    pub const fn new(value: ExitCode_latest) -> Self {
        Self(value)
    }
}

impl From<u32> for ExitCode {
    fn from(value: u32) -> Self {
        Self(ExitCodeV4::new(value))
    }
}

impl From<ExitCodeV3> for ExitCode {
    fn from(value: ExitCodeV3) -> Self {
        value.value().into()
    }
}

impl From<ExitCodeV2> for ExitCode {
    fn from(value: ExitCodeV2) -> Self {
        value.value().into()
    }
}

impl From<ExitCode> for ExitCodeV2 {
    fn from(value: ExitCode) -> Self {
        Self::new(value.0.value())
    }
}

impl From<ExitCode> for ExitCodeV3 {
    fn from(value: ExitCode) -> Self {
        Self::new(value.0.value())
    }
}
