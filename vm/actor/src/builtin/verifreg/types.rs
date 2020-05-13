// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::StoragePower;
use address::Address;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifierParams {
    pub address: Address,
    pub allowance: Datacap,
}

pub type AddVerifierParams = VerifierParams;
pub type AddVerifierClientParams = VerifierParams;

pub const MINIMUM_VERIFIED_SIZE: u32 = 1 << 20;

pub type Datacap = StoragePower;

#[derive(Clone, Debug, PartialEq)]
pub struct BytesParams {
    pub address: Address,
    pub deal_size: Datacap,
}

pub type UseBytesParams = BytesParams;
pub type RestoreBytesParams = BytesParams;
