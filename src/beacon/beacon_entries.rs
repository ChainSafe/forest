// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::utils::encoding::serde_byte_array;
use serde_tuple::{self, Deserialize_tuple, Serialize_tuple};

/// The result from getting an entry from `Drand`.
/// The entry contains the round, or epoch as well as the BLS signature for that
/// round of randomness.
/// This beacon entry is stored on chain in the block header.
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize_tuple, Serialize_tuple)]
pub struct BeaconEntry {
    round: u64,
    #[serde(with = "serde_byte_array")]
    data: Vec<u8>,
}

impl BeaconEntry {
    pub fn new(round: u64, data: Vec<u8>) -> Self {
        Self { round, data }
    }
    /// Returns the current round number.
    pub fn round(&self) -> u64 {
        self.round
    }
    /// The signature of message `H(prev_round, prev_round.data, round)`.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn into_parts(self) -> (u64, Vec<u8>) {
        let Self { round, data } = self;
        (round, data)
    }
}

pub mod json {
    use base64::{prelude::BASE64_STANDARD, Engine};
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    use super::*;

    /// Wrapper for serializing and de-serializing a `BeaconEntry` from JSON.
    #[derive(Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct BeaconEntryJson(#[serde(with = "self")] pub BeaconEntry);

    /// Wrapper for serializing a `BeaconEntry` reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct BeaconEntryJsonRef<'a>(#[serde(with = "self")] pub &'a BeaconEntry);

    impl From<BeaconEntryJson> for BeaconEntry {
        fn from(wrapper: BeaconEntryJson) -> Self {
            wrapper.0
        }
    }

    #[derive(Serialize, Deserialize)]
    struct JsonHelper {
        #[serde(rename = "Round")]
        round: u64,
        #[serde(rename = "Data")]
        data: String,
    }

    pub fn serialize<S>(m: &BeaconEntry, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            round: m.round,
            data: BASE64_STANDARD.encode(&m.data),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<BeaconEntry, D::Error>
    where
        D: Deserializer<'de>,
    {
        let m: JsonHelper = Deserialize::deserialize(deserializer)?;
        Ok(BeaconEntry {
            round: m.round,
            data: BASE64_STANDARD.decode(m.data).map_err(de::Error::custom)?,
        })
    }

    pub mod vec {
        use crate::utils::json::GoVecVisitor;
        use serde::ser::SerializeSeq;

        use super::*;

        pub fn serialize<S>(m: &[BeaconEntry], serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut seq = serializer.serialize_seq(Some(m.len()))?;
            for e in m {
                seq.serialize_element(&BeaconEntryJsonRef(e))?;
            }
            seq.end()
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<BeaconEntry>, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(GoVecVisitor::<BeaconEntry, BeaconEntryJson>::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use quickcheck_macros::quickcheck;

    use super::{
        json::{BeaconEntryJson, BeaconEntryJsonRef},
        *,
    };

    #[quickcheck]
    fn beacon_entry_roundtrip(entry: BeaconEntry) {
        let serialized = serde_json::to_string(&BeaconEntryJsonRef(&entry)).unwrap();
        let parsed: BeaconEntryJson = serde_json::from_str(&serialized).unwrap();
        assert_eq!(entry, parsed.into());
    }
}
