// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use num_bigint::BigUint;
use vm::ExitCode;

/// Result of a state transition from a message
#[derive(PartialEq, Clone)]
pub struct MessageReceipt {
    // TODO: determine if this is necessary, code returned from cbor
    pub exit_code: ExitCode,
    pub return_data: Vec<u8>,
    pub gas_used: BigUint,
}
