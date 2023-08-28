// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use serde_tuple::{self, Deserialize_tuple, Serialize_tuple};

use crate::blocks::BlockHeader;

/// Block message used as serialized `gossipsub` messages for blocks topic.
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple, Default)]
pub struct GossipBlock {
    pub header: BlockHeader,
    pub bls_messages: Vec<Cid>,
    pub secpk_messages: Vec<Cid>,
}
