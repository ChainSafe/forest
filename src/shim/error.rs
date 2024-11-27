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
pub struct ExitCode(#[schemars(with = "u32")] ExitCodeV4);

impl ExitCode {
    /// The lowest exit code that an actor may abort with.
    pub const FIRST_USER_EXIT_CODE: u32 = ExitCodeV4::FIRST_USER_EXIT_CODE;
    pub const FIRST_ACTOR_ERROR_CODE: u32 = 32;

    pub fn value(&self) -> u32 {
        self.0.value()
    }

    pub fn is_success(&self) -> bool {
        self.0.is_success()
    }
}

impl From<u32> for ExitCode {
    fn from(value: u32) -> Self {
        Self(ExitCodeV4::new(value))
    }
}

impl From<ExitCodeV4> for ExitCode {
    fn from(value: ExitCodeV4) -> Self {
        Self(value)
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

impl From<ExitCode> for ExitCodeV4 {
    fn from(value: ExitCode) -> Self {
        value.0
    }
}
