// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::error::ExitCode;
use serde::Serialize;
use std::fmt::Debug;
use thiserror::Error;

#[derive(Clone, Debug, Error, Serialize)]
pub enum EthErrors {
    #[error("{message}")]
    ExecutionReverted {
        message: String,
        data: Option<String>,
    },
}

impl EthErrors {
    /// Create a new ExecutionReverted error with formatted message
    pub fn execution_reverted(exit_code: ExitCode, error: &str, reason: &str, data: &[u8]) -> Self {
        Self::ExecutionReverted {
            message: format!(
                "message execution failed (exit=[{}], revert reason=[{}], vm error=[{}])",
                exit_code, reason, error
            ),
            data: (!data.is_empty()).then(|| format!("0x{}", hex::encode(data))),
        }
    }
}