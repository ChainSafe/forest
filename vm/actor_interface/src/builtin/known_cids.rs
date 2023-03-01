// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashSet;
use cid::Cid;
use serde::{Deserialize, Serialize};

lazy_static::lazy_static! {
    pub static ref KNOWN_CIDS: KnownCids = serde_yaml::from_str(include_str!("known_cids.yaml")).unwrap();
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct KnownCidsPerNetworkVersion {
    #[serde(with = "cid_hashset")]
    pub v8: HashSet<Cid>,
    #[serde(with = "cid_hashset")]
    pub v9: HashSet<Cid>,
    #[serde(with = "cid_hashset")]
    pub v10: HashSet<Cid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct KnownCids {
    pub market: KnownCidsPerNetworkVersion,
}

mod cid_hashset {
    use ahash::HashSetExt;
    use serde::{Deserializer, Serializer};

    use super::*;

    pub fn serialize<S>(value: &HashSet<Cid>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let transcoded = HashSet::from_iter(value.iter().map(|cid| cid.to_string()));
        transcoded.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<HashSet<Cid>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let transcoded: HashSet<String> = HashSet::deserialize(deserializer)?;
        let mut result = HashSet::with_capacity(transcoded.len());
        for cid in transcoded {
            result.insert(Cid::try_from(cid).map_err(|e| serde::de::Error::custom(e.to_string()))?);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use anyhow::{ensure, Result};

    use super::*;

    #[test]
    fn test_loading_static_value() -> Result<()> {
        ensure!(KNOWN_CIDS.market.v8.contains(&Cid::try_from(
            "bafk2bzacediohrxkp2fbsl4yj4jlupjdkgsiwqb4zuezvinhdo2j5hrxco62q"
        )?));
        ensure!(!KNOWN_CIDS.market.v9.contains(&Cid::try_from(
            "bafk2bzacediohrxkp2fbsl4yj4jlupjdkgsiwqb4zuezvinhdo2j5hrxco62q"
        )?));

        let serialized = serde_yaml::to_string(&*KNOWN_CIDS)?;
        let deserialized = serde_yaml::from_str(&serialized)?;
        ensure!(&*KNOWN_CIDS == &deserialized);

        Ok(())
    }
}
