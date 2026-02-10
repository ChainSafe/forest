// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::error::RpcErrorData;
use crate::shim::error::ExitCode;
use serde::Serialize;
use std::fmt::Debug;
use thiserror::Error;

/// This error indicates that the execution reverted while executing the message.
/// Error code 3 was introduced in geth v1.9.15 and is now expected by most Ethereum ecosystem tooling for automatic ABI decoding of revert reasons from the error data field.
pub const EXECUTION_REVERTED_CODE: i32 = 3;

#[derive(Clone, Debug, Error, Serialize)]
pub enum EthErrors {
    #[error("{message}")]
    ExecutionReverted { message: String, data: String },
}

impl EthErrors {
    /// Create a new ExecutionReverted error with formatted message
    pub fn execution_reverted(exit_code: ExitCode, reason: &str, error: &str, data: &[u8]) -> Self {
        let revert_reason = if reason.is_empty() {
            String::new()
        } else {
            format!(", revert reason=[{reason}]")
        };

        Self::ExecutionReverted {
            message: format!(
                "message execution failed (exit=[{exit_code}]{revert_reason}, vm error=[{error}])"
            ),
            data: format!("0x{}", hex::encode(data)),
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
                Some(serde_json::Value::String(data.clone()))
            }
        }
    }
}
