// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_shared4::error::ExitCode;
use std::fmt;
use std::fmt::Debug;

pub enum EthErrors {
    ExecutionReverted { message: String, data: String },
}

impl EthErrors {
    /// Create a new ExecutionReverted error with formatted message
    pub fn execution_reverted(exit_code: ExitCode, error: &str, reason: &str, data: &[u8]) -> Self {
        Self::ExecutionReverted {
            message: format!(
                "message execution failed (exit=[{}], revert reason=[{}], vm error=[{}])",
                exit_code, reason, error
            ),
            data: format!("0x{}", hex::encode(data)),
        }
    }
}

impl fmt::Display for EthErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExecutionReverted { message, data } => {
                if data == "0x" {
                    write!(f, "message: {message}")
                } else {
                    write!(f, "message: {message}, data: {data}")
                }
            }
        }
    }
}

impl Debug for EthErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExecutionReverted { message, data } => {
                if data == "0x" {
                    write!(f, "message: {message}")
                } else {
                    write!(f, "message: {message}, data: {data}")
                }
            }
        }
    }
}

// Implement standard Error trait
impl std::error::Error for EthErrors {}
