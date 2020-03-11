// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{MethodNum, Serialized, TokenAmount};

use address::Address;

/// Input variables for actor method invocation.
pub struct InvocInput {
    pub to: Address,
    pub method: MethodNum,
    pub params: Serialized,
    pub value: TokenAmount,
}
