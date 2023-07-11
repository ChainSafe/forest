// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use fvm_shared2::error::ExitCode as ExitCodeV2;
use fvm_shared3::error::ExitCode as ExitCodeV3;
use serde::{Deserialize, Serialize};

/// `Newtype` wrapper for the FVM `ExitCode`.
///
/// # Examples
/// ```
/// # use forest_filecoin::doctest_private::ExitCode;
/// let fvm2_success = fvm_shared2::error::ExitCode::new(0);
/// let fvm3_success = fvm_shared3::error::ExitCode::new(0);
///
/// let shim_from_v2 = ExitCode::from(fvm2_success);
/// let shim_from_v3 = ExitCode::from(fvm3_success);
///
/// assert_eq!(shim_from_v2, shim_from_v3);
/// assert_eq!(shim_from_v2, fvm2_success.into());
/// assert_eq!(*shim_from_v3, fvm3_success);
/// ```
#[derive(PartialEq, Eq, Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExitCode(ExitCodeV3);
impl ExitCode {
    // Exit codes which originate inside the VM.
    // These values may not be used by actors when aborting.

    /// The code indicating successful execution.
    pub const OK: ExitCode = ExitCode(ExitCodeV3::OK);
    /// The message sender doesn't exist.
    pub const SYS_SENDER_INVALID: ExitCode = ExitCode(ExitCodeV3::SYS_SENDER_INVALID);
    /// The message sender was not in a valid state to send this message.
    ///
    /// Either:
    /// - The sender's nonce nonce didn't match the message nonce.
    /// - The sender didn't have the funds to cover the message gas.
    pub const SYS_SENDER_STATE_INVALID: ExitCode = ExitCode(ExitCodeV3::SYS_SENDER_STATE_INVALID);
    //pub const SYS_RESERVED_3 ExitCode = ExitCode::new(3);
    /// The message receiver trapped (panicked).
    pub const SYS_ILLEGAL_INSTRUCTION: ExitCode = ExitCode(ExitCodeV3::SYS_ILLEGAL_INSTRUCTION);
    /// The message receiver doesn't exist and can't be automatically created
    pub const SYS_INVALID_RECEIVER: ExitCode = ExitCode(ExitCodeV3::SYS_INVALID_RECEIVER);
    /// The message sender didn't have the requisite funds.
    pub const SYS_INSUFFICIENT_FUNDS: ExitCode = ExitCode(ExitCodeV3::SYS_INSUFFICIENT_FUNDS);
    /// Message execution (including subcalls) used more gas than the specified limit.
    pub const SYS_OUT_OF_GAS: ExitCode = ExitCode(ExitCodeV3::SYS_OUT_OF_GAS);
    // pub const SYS_RESERVED_8: ExitCode = ExitCode::new(8);
    /// The message receiver aborted with a reserved exit code.
    pub const SYS_ILLEGAL_EXIT_CODE: ExitCode = ExitCode(ExitCodeV3::SYS_ILLEGAL_EXIT_CODE);
    /// An internal VM assertion failed.
    pub const SYS_ASSERTION_FAILED: ExitCode = ExitCode(ExitCodeV3::SYS_ASSERTION_FAILED);
    /// The actor returned a block handle that doesn't exist
    pub const SYS_MISSING_RETURN: ExitCode = ExitCode(ExitCodeV3::SYS_MISSING_RETURN);
    // pub const SYS_RESERVED_12: ExitCode = ExitCode::new(12);
    // pub const SYS_RESERVED_13: ExitCode = ExitCode::new(13);
    // pub const SYS_RESERVED_14: ExitCode = ExitCode::new(14);
    // pub const SYS_RESERVED_15: ExitCode = ExitCode::new(15);

    /// The lowest exit code that an actor may abort with.
    pub const FIRST_USER_EXIT_CODE: u32 = ExitCodeV3::FIRST_USER_EXIT_CODE;

    // Standard exit codes according to the built-in actors' calling convention.
    /// The method parameters are invalid.
    pub const USR_ILLEGAL_ARGUMENT: ExitCode = ExitCode(ExitCodeV3::USR_ILLEGAL_ARGUMENT);
    /// The requested resource does not exist.
    pub const USR_NOT_FOUND: ExitCode = ExitCode(ExitCodeV3::USR_NOT_FOUND);
    /// The requested operation is forbidden.
    pub const USR_FORBIDDEN: ExitCode = ExitCode(ExitCodeV3::USR_FORBIDDEN);
    /// The actor has insufficient funds to perform the requested operation.
    pub const USR_INSUFFICIENT_FUNDS: ExitCode = ExitCode(ExitCodeV3::USR_INSUFFICIENT_FUNDS);
    /// The actor's internal state is invalid.
    pub const USR_ILLEGAL_STATE: ExitCode = ExitCode(ExitCodeV3::USR_ILLEGAL_STATE);
    /// There was a de/serialization failure within actor code.
    pub const USR_SERIALIZATION: ExitCode = ExitCode(ExitCodeV3::USR_SERIALIZATION);
    /// The message cannot be handled (usually indicates an unhandled method number).
    pub const USR_UNHANDLED_MESSAGE: ExitCode = ExitCode(ExitCodeV3::USR_UNHANDLED_MESSAGE);
    /// The actor failed with an unspecified error.
    pub const USR_UNSPECIFIED: ExitCode = ExitCode(ExitCodeV3::USR_UNSPECIFIED);
    /// The actor failed a user-level assertion.
    pub const USR_ASSERTION_FAILED: ExitCode = ExitCode(ExitCodeV3::USR_ASSERTION_FAILED);
    /// The requested operation cannot be performed in "read-only" mode.
    pub const USR_READ_ONLY: ExitCode = ExitCode(ExitCodeV3::USR_READ_ONLY);
    /// The method cannot handle a transfer of value.
    pub const USR_NOT_PAYABLE: ExitCode = ExitCode(ExitCodeV3::USR_NOT_PAYABLE);
}

impl From<u32> for ExitCode {
    fn from(value: u32) -> Self {
        Self(ExitCodeV3::new(value))
    }
}

impl From<ExitCodeV3> for ExitCode {
    fn from(value: ExitCodeV3) -> Self {
        Self(value)
    }
}

impl From<ExitCodeV2> for ExitCode {
    fn from(value: ExitCodeV2) -> Self {
        Self::from(value.value())
    }
}

impl From<ExitCode> for ExitCodeV2 {
    fn from(value: ExitCode) -> Self {
        Self::new(value.0.value())
    }
}

impl From<ExitCode> for ExitCodeV3 {
    fn from(value: ExitCode) -> Self {
        value.0
    }
}
