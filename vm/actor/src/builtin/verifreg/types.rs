// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use encoding::tuple::*;
use fil_types::StoragePower;
use num_bigint::bigint_ser;
use num_traits::FromPrimitive;

lazy_static! {
    pub static ref MINIMUM_VERIFIED_SIZE: StoragePower = StoragePower::from_i32(1 << 20).unwrap();// placeholder
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct VerifierParams {
    pub address: Address,
    #[serde(with = "bigint_ser")]
    pub allowance: Datacap,
}

pub type AddVerifierParams = VerifierParams;
pub type AddVerifierClientParams = VerifierParams;

pub type Datacap = StoragePower;

#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct BytesParams {
    pub address: Address,
    #[serde(with = "bigint_ser")]
    pub deal_size: Datacap,
}

pub type UseBytesParams = BytesParams;
pub type RestoreBytesParams = BytesParams;
