// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::ops::{Deref, DerefMut};

use fvm_shared::error::ExitCode as ExitCodeV2;
use fvm_shared3::error::ExitCode as ExitCodeV3;
use serde::{Deserialize, Serialize};

use crate::Inner;

/// `Newtype` wrapper for the FVM `ExitCode`.
///
/// # Examples
/// ```
/// let fvm2_success = fvm_shared::error::ExitCode::new(0);
/// let fvm3_success = fvm_shared3::error::ExitCode::new(0);
///
/// let shim_from_v2 = forest_shim::error::ExitCode::from(fvm2_success);
/// let shim_from_v3 = forest_shim::error::ExitCode::from(fvm3_success);
///
/// assert_eq!(shim_from_v2, shim_from_v3);
/// assert_eq!(shim_from_v2, fvm2_success.into());
/// assert_eq!(*shim_from_v3, fvm3_success);
/// ```
#[derive(PartialEq, Eq, Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExitCode(ExitCodeV3);

impl Inner for ExitCode {
    type FVM = ExitCodeV3;
}

impl Deref for ExitCode {
    type Target = ExitCodeV3;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ExitCode {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
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
        Self::new(value.value())
    }
}

impl From<ExitCode> for ExitCodeV3 {
    fn from(value: ExitCode) -> Self {
        *value
    }
}
