// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use serde::{Deserialize, Deserializer, Serialize, Serializer};

// This is only used as a utility because go impl serializes no data as an empty
// map
#[allow(dead_code)]
#[derive(Serialize, Deserialize)]
struct EmptyMap {}

#[allow(dead_code)]
pub fn serialize<S>(serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    EmptyMap {}.serialize(serializer)
}

#[allow(dead_code)]
pub fn deserialize<'de, D>(deserializer: D) -> Result<(), D::Error>
where
    D: Deserializer<'de>,
{
    let EmptyMap {} = Deserialize::deserialize(deserializer)?;
    Ok(())
}
