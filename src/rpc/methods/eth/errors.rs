// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::error::RpcErrorData;
use crate::shim::error::ExitCode;
use serde::Serialize;
use std::fmt::Debug;
use thiserror::Error;

/// This error indicates that the execution reverted while executing the message.
/// Code is taken from https://github.com/filecoin-project/lotus/blob/release/v1.32.1/api/api_errors.go#L27
pub const EXECUTION_REVERTED_CODE: i32 = 11;

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
                "message execution failed (exit=[{exit_code}], revert reason=[{reason}], vm error=[{error}])"
            ),
            data: (!data.is_empty()).then(|| format!("0x{}", hex::encode(data))),
        }
    }
}

impl RpcErrorData for EthErrors {
    fn error_code(&self) -> Option<i32> {
        match self {
            EthErrors::ExecutionReverted { .. } => Some(EXECUTION_REVERTED_CODE),
        }
    }

    fn error_message(&self) -> Option<String> {
        match self {
            EthErrors::ExecutionReverted { message, .. } => Some(message.clone()),
        }
    }

    fn error_data(&self) -> Option<serde_json::Value> {
        match self {
            EthErrors::ExecutionReverted { data, .. } => {
                data.clone().map(serde_json::Value::String)
            }
        }
    }
}
