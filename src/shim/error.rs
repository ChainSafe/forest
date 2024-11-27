// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use fvm_shared2::error::ExitCode as ExitCodeV2;
use fvm_shared3::error::ExitCode as ExitCodeV3;
use fvm_shared4::error::ExitCode as ExitCodeV4;
use schemars::JsonSchema;
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
/// assert_eq!(shim_from_v3, fvm3_success.into());
/// ```
#[derive(PartialEq, Eq, Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub struct ExitCode(#[schemars(with = "u32")] ExitCodeV3);

impl ExitCode {
    /// The lowest exit code that an actor may abort with.
    pub const FIRST_USER_EXIT_CODE: u32 = ExitCodeV3::FIRST_USER_EXIT_CODE;
    pub const FIRST_ACTOR_ERROR_CODE: u32 = 32;

    // See https://github.com/filecoin-project/builtin-actors/blob/6e781444cee5965278c46ef4ffe1fb1970f18d7d/actors/evm/src/lib.rs#L35-L42
    pub const EVM_CONTRACT_REVERTED: ExitCode = ExitCode::new(33);
    pub const EVM_CONTRACT_INVALID_INSTRUCTION: ExitCode = ExitCode::new(34);
    pub const EVM_CONTRACT_UNDEFINED_INSTRUCTION: ExitCode = ExitCode::new(35);
    pub const EVM_CONTRACT_STACK_UNDERFLOW: ExitCode = ExitCode::new(36);
    pub const EVM_CONTRACT_STACK_OVERFLOW: ExitCode = ExitCode::new(37);
    pub const EVM_CONTRACT_ILLEGAL_MEMORY_ACCESS: ExitCode = ExitCode::new(38);
    pub const EVM_CONTRACT_BAD_JUMPDEST: ExitCode = ExitCode::new(39);
    pub const EVM_CONTRACT_SELFDESTRUCT_FAILED: ExitCode = ExitCode::new(40);

    pub fn value(&self) -> u32 {
        self.0.value()
    }

    pub fn is_success(&self) -> bool {
        self.0.is_success()
    }

    pub const fn new(value: u32) -> Self {
        Self(ExitCodeV3::new(value))
    }
}

impl From<u32> for ExitCode {
    fn from(value: u32) -> Self {
        Self(ExitCodeV3::new(value))
    }
}

impl From<ExitCodeV4> for ExitCode {
    fn from(value: ExitCodeV4) -> Self {
        value.value().into()
    }
}

impl From<ExitCodeV3> for ExitCode {
    fn from(value: ExitCodeV3) -> Self {
        Self(value)
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
        value.0
    }
}

impl From<ExitCode> for ExitCodeV4 {
    fn from(value: ExitCode) -> Self {
        Self::new(value.0.value())
    }
}
