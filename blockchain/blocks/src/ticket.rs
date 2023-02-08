// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_crypto::VRFProof;
use forest_encoding::tuple::*;

/// A Ticket is a marker of a tick of the blockchain's clock.  It is the source
/// of randomness for proofs of storage and leader election.  It is generated
/// by the miner of a block using a `VRF` and a `VDF`.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize_tuple, Deserialize_tuple)]
pub struct Ticket {
    /// A proof output by running a `VRF` on the `VDFResult` of the parent
    /// ticket
    pub vrfproof: VRFProof,
}

impl Ticket {
    /// Ticket constructor
    pub fn new(vrfproof: VRFProof) -> Self {
        Self { vrfproof }
    }
}

#[cfg(test)]
impl quickcheck::Arbitrary for Ticket {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let fmt_str = format!("===={}=====", u64::arbitrary(g));
        let vrfproof = VRFProof::new(fmt_str.into_bytes());
        Self { vrfproof }
    }
}

pub mod json {
    use base64::{prelude::BASE64_STANDARD, Engine};
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    use super::*;

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
            vrfproof: BASE64_STANDARD.encode(m.vrfproof.as_bytes()),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Ticket, D::Error>
    where
        D: Deserializer<'de>,
    {
        let m: JsonHelper = Deserialize::deserialize(deserializer)?;
        Ok(Ticket {
            vrfproof: VRFProof::new(
                BASE64_STANDARD
                    .decode(m.vrfproof)
                    .map_err(de::Error::custom)?,
            ),
        })
    }

    pub mod opt {
        use serde::{self, Deserialize, Deserializer, Serialize, Serializer};

        use super::*;

        pub fn serialize<S>(v: &Option<Ticket>, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            v.as_ref().map(TicketJsonRef).serialize(serializer)
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

#[cfg(test)]
mod tests {
    use quickcheck_macros::quickcheck;

    use super::{
        json::{TicketJson, TicketJsonRef},
        *,
    };

    #[quickcheck]
    fn ticket_round_trip(ticket: Ticket) {
        let serialized = serde_json::to_string(&TicketJsonRef(&ticket)).unwrap();
        let parsed: TicketJson = serde_json::from_str(&serialized).unwrap();
        assert_eq!(ticket, parsed.0);
    }
}
