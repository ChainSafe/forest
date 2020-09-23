// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crypto::VRFProof;
use encoding::{serde_bytes, tuple::*};
use fil_types::PoStProof;

/// A Ticket is a marker of a tick of the blockchain's clock.  It is the source
/// of randomness for proofs of storage and leader election.  It is generated
/// by the miner of a block using a VRF and a VDF.
#[derive(
    Clone, Debug, PartialEq, PartialOrd, Eq, Default, Ord, Serialize_tuple, Deserialize_tuple,
)]
pub struct Ticket {
    /// A proof output by running a VRF on the VDFResult of the parent ticket
    pub vrfproof: VRFProof,
}

impl Ticket {
    /// Ticket constructor
    pub fn new(vrfproof: VRFProof) -> Self {
        Self { vrfproof }
    }
}

/// PoSt election candidates
#[derive(Clone, Debug, PartialEq, Default, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct EPostTicket {
    #[serde(with = "serde_bytes")]
    pub partial: Vec<u8>,
    pub sector_id: u64,
    pub challenge_index: u64,
}

/// Proof of Spacetime election proof
#[derive(Clone, Debug, PartialEq, Default, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct EPostProof {
    pub proof: Vec<PoStProof>,
    #[serde(with = "serde_bytes")]
    pub post_rand: Vec<u8>,
    pub candidates: Vec<EPostTicket>,
}

#[cfg(feature = "json")]
pub mod json {
    use super::*;
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct TicketJson(#[serde(with = "self")] pub Ticket);

    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct TicketJsonRef<'a>(#[serde(with = "self")] pub &'a Ticket);

    #[derive(Serialize, Deserialize)]
    struct JsonHelper {
        #[serde(rename = "VRFProof")]
        vrfproof: String,
    }

    pub fn serialize<S>(m: &Ticket, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            vrfproof: base64::encode(m.vrfproof.as_bytes()),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Ticket, D::Error>
    where
        D: Deserializer<'de>,
    {
        let m: JsonHelper = Deserialize::deserialize(deserializer)?;
        Ok(Ticket {
            vrfproof: VRFProof::new(base64::decode(m.vrfproof).map_err(de::Error::custom)?),
        })
    }

    pub mod opt {
        use super::*;
        use serde::{self, Deserialize, Deserializer, Serialize, Serializer};

        pub fn serialize<S>(v: &Option<Ticket>, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            v.as_ref().map(|s| TicketJsonRef(s)).serialize(serializer)
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Ticket>, D::Error>
        where
            D: Deserializer<'de>,
        {
            let s: Option<TicketJson> = Deserialize::deserialize(deserializer)?;
            Ok(s.map(|v| v.0))
        }
    }
}
