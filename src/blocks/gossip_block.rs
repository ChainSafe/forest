// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use fvm_ipld_encoding::tuple::*;

use crate::blocks::CachingBlockHeader;

/// Block message used as serialized `gossipsub` messages for blocks topic.
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary, Default))]
#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct GossipBlock {
    pub header: CachingBlockHeader,
    pub bls_messages: Vec<Cid>,
    pub secpk_messages: Vec<Cid>,
}
