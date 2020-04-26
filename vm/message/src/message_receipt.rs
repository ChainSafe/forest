// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vm::{ExitCode, Serialized};

/// Result of a state transition from a message
#[derive(PartialEq, Clone)]
pub struct MessageReceipt {
    pub exit_code: ExitCode,
    pub return_data: Serialized,
    pub gas_used: u64,
}

impl Serialize for MessageReceipt {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.exit_code, &self.return_data, &self.gas_used).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MessageReceipt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (exit_code, return_data, gas_used) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            exit_code,
            return_data,
            gas_used,
        })
    }
}
