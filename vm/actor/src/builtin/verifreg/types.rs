// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use encoding::tuple::*;
use fil_types::StoragePower;
use num_bigint::bigint_ser;
use num_traits::FromPrimitive;

#[cfg(not(feature = "devnet"))]
lazy_static! {
    pub static ref MINIMUM_VERIFIED_DEAL_SIZE: StoragePower = StoragePower::from_i32(1 << 20).unwrap(); // placeholder
}

#[cfg(feature = "devnet")]
lazy_static! {
    pub static ref MINIMUM_VERIFIED_DEAL_SIZE: StoragePower = StoragePower::from_i32(256).unwrap(); // placeholder
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct VerifierParams {
    pub address: Address,
    #[serde(with = "bigint_ser")]
    pub allowance: DataCap,
}

pub type AddVerifierParams = VerifierParams;
pub type AddVerifierClientParams = VerifierParams;

/// DataCap is an integer number of bytes.
/// We can introduce policy changes and replace this in the future.
pub type DataCap = StoragePower;

#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct BytesParams {
    /// Address of verified client.
    pub address: Address,
    /// Number of bytes to use.
    #[serde(with = "bigint_ser")]
    pub deal_size: StoragePower,
}

pub type UseBytesParams = BytesParams;
pub type RestoreBytesParams = BytesParams;
