// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::BlockHeader;
use cid::Cid;
use forest_encoding::tuple::*;
use fvm_ipld_encoding::Cbor;
use fvm_shared::{address::Address, clock::ChainEpoch};

/// Block message used as serialized `gossipsub` messages for blocks topic.
#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct GossipBlock {
    pub header: BlockHeader,
    pub bls_messages: Vec<Cid>,
    pub secpk_messages: Vec<Cid>,
}

impl quickcheck::Arbitrary for GossipBlock {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let header = BlockHeader::builder()
            .miner_address(Address::new_id(0))
            .epoch(ChainEpoch::arbitrary(g))
            .build()
            .unwrap();
        let cid = Cid::new_v1(
            u64::arbitrary(g),
            cid::multihash::Multihash::wrap(u64::arbitrary(g), &[u8::arbitrary(g)]).unwrap(),
        );
        Self {
            header,
            bls_messages: vec![cid],
            secpk_messages: vec![cid],
        }
    }
}

impl Cbor for GossipBlock {}

pub mod json {
    use super::*;
    use crate::header;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Wrapper for serializing and de-serializing a `GossipBlock` from JSON.
    #[derive(Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct GossipBlockJson(#[serde(with = "self")] pub GossipBlock);

    /// Wrapper for serializing a `GossipBlock` reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct GossipBlockJsonRef<'a>(#[serde(with = "self")] pub &'a GossipBlock);

    pub fn serialize<S>(m: &GossipBlock, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        #[serde(rename_all = "PascalCase")]
        struct GossipBlockSer<'a> {
            #[serde(with = "header::json")]
            pub header: &'a BlockHeader,
            #[serde(with = "forest_json::cid::vec")]
            pub bls_messages: &'a [Cid],
            #[serde(with = "forest_json::cid::vec")]
            pub secpk_messages: &'a [Cid],
        }
        GossipBlockSer {
            header: &m.header,
            bls_messages: &m.bls_messages,
            secpk_messages: &m.secpk_messages,
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<GossipBlock, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct GossipBlockDe {
            #[serde(with = "header::json")]
            pub header: BlockHeader,
            #[serde(with = "forest_json::cid::vec")]
            pub bls_messages: Vec<Cid>,
            #[serde(with = "forest_json::cid::vec")]
            pub secpk_messages: Vec<Cid>,
        }
        let GossipBlockDe {
            header,
            bls_messages,
            secpk_messages,
        } = Deserialize::deserialize(deserializer)?;
        Ok(GossipBlock {
            header,
            bls_messages,
            secpk_messages,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::json::{GossipBlockJson, GossipBlockJsonRef};
    use super::*;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn gossip_block_roundtrip(block: GossipBlock) {
        let serialized = serde_json::to_string(&GossipBlockJsonRef(&block)).unwrap();
        let parsed: GossipBlockJson = serde_json::from_str(&serialized).unwrap();
        assert_eq!(block, parsed.0);
    }
}
