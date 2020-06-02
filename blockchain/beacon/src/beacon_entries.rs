// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use encoding::{
    de::{Deserialize, Deserializer},
    ser::{Serialize, Serializer},
    BytesDe, BytesSer,
};

/// The result from getting an entry from Drand.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BeaconEntry {
    round: u64,
    data: Vec<u8>,
}

impl BeaconEntry {
    pub fn new(round: u64, data: Vec<u8>) -> Self {
        Self { round, data }
    }
    /// Returns the current round number
    pub fn round(&self) -> u64 {
        self.round
    }
    /// The signature of message H(prev_round, prev_round.data, round).
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

impl Serialize for BeaconEntry {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.round, BytesSer(&self.data)).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BeaconEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        let (round, data): (u64, BytesDe) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            round,
            data: data.0,
        })
    }
}

#[cfg(feature = "json")]
pub mod json {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Wrapper for serializing and deserializing a BeaconEntry from JSON.
    #[derive(Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct BeaconEntryJson(#[serde(with = "self")] pub BeaconEntry);

    /// Wrapper for serializing a BeaconEntry reference to JSON.
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
            data: base64::encode(&m.data),
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
            data: base64::decode(m.data).unwrap(),
        })
    }

    pub mod vec {
        use super::*;
        use forest_json_utils::GoVecVisitor;
        use serde::ser::SerializeSeq;

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
