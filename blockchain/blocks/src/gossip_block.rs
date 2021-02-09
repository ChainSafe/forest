// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::BlockHeader;
use cid::Cid;
use encoding::{tuple::*, Cbor};

/// Block message used as serialized gossipsub messages for blocks topic.
#[derive(Clone, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct GossipBlock {
    pub header: BlockHeader,
    pub bls_messages: Vec<Cid>,
    pub secpk_messages: Vec<Cid>,
}

impl Cbor for GossipBlock {}

#[cfg(feature = "json")]
pub mod json {
    use super::*;
    use crate::header;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Wrapper for serializing and deserializing a GossipBlock from JSON.
    #[derive(Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct GossipBlockJson(#[serde(with = "self")] pub GossipBlock);

    /// Wrapper for serializing a GossipBlock reference to JSON.
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
            #[serde(with = "cid::json::vec")]
            pub bls_messages: &'a [Cid],
            #[serde(with = "cid::json::vec")]
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
            #[serde(with = "cid::json::vec")]
            pub bls_messages: Vec<Cid>,
            #[serde(with = "cid::json::vec")]
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
