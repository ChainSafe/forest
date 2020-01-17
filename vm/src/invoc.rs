// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use crate::{ExitCode, MethodNum, MethodParams, TokenAmount};
use address::Address;

/// Input variables for actor method invocation.
pub struct InvocInput {
    pub to: Address,
    pub method: MethodNum,
    pub params: MethodParams,
    pub value: TokenAmount,
}

/// Output variables for actor method invocation.
pub struct InvocOutput {
    pub exit_code: ExitCode,
    pub return_value: Vec<u8>,
}

impl InvocOutput {
    pub fn create_error(code: ExitCode) -> Self {
        Self {
            exit_code: code,
            return_value: vec![],
        }
    }
}

impl From<ExitCode> for InvocOutput {
    fn from(code: ExitCode) -> InvocOutput {
        InvocOutput {
            exit_code: code,
            return_value: Vec::new(),
        }
    }
}
