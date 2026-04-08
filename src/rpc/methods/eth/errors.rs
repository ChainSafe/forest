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
/// This error indicates that the block range provided in the RPC exceeds the configured maximum
/// It was introduced in EIP-1474
pub const LIMIT_EXCEEDED_CODE: i32 = -32005;

#[derive(Clone, Debug, Error, Serialize)]
pub enum EthErrors {
    #[error("{message}")]
    ExecutionReverted { message: String, data: String },
    #[error("{message}")]
    BlockRangeExceeded {
        max: i64,
        given: i64,
        message: String,
    },
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

    pub fn limit_exceeded(max_block_range: i64, given: i64) -> Self {
        Self::BlockRangeExceeded {
            max: max_block_range,
            given,
            message: format!("block range exceeds maximum of {max_block_range} (got {given})"),
        }
    }
}

impl RpcErrorData for EthErrors {
    fn error_code(&self) -> Option<i32> {
        match self {
            EthErrors::ExecutionReverted { .. } => Some(EXECUTION_REVERTED_CODE),
            EthErrors::BlockRangeExceeded { .. } => Some(LIMIT_EXCEEDED_CODE),
        }
    }

    fn error_message(&self) -> Option<String> {
        match self {
            EthErrors::ExecutionReverted { message, .. } => Some(message.clone()),
            EthErrors::BlockRangeExceeded { message, .. } => Some(message.clone()),
        }
    }

    fn error_data(&self) -> Option<serde_json::Value> {
        match self {
            EthErrors::ExecutionReverted { data, .. } => {
                Some(serde_json::Value::String(data.clone()))
            }
            EthErrors::BlockRangeExceeded { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::error::ServerError;

    #[test]
    fn test_block_range_exceeded_converts_to_server_error_with_correct_code() {
        let err = EthErrors::limit_exceeded(100, 500);
        let server_err: ServerError = err.into();

        assert_eq!(server_err.inner().code(), LIMIT_EXCEEDED_CODE);
        assert_eq!(
            server_err.message(),
            "block range exceeds maximum of 100 (got 500)"
        );
    }

    #[test]
    fn test_block_range_exceeded_via_anyhow_preserves_code() {
        let eth_err = EthErrors::limit_exceeded(2880, 5000);
        let anyhow_err: anyhow::Error = eth_err.into();
        let server_err: ServerError = anyhow_err.into();

        assert_eq!(server_err.inner().code(), LIMIT_EXCEEDED_CODE);
        assert_eq!(
            server_err.message(),
            "block range exceeds maximum of 2880 (got 5000)"
        );
    }
}
