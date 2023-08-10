// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::VRFProof;

pub mod json {
    use std::borrow::Cow;

    use base64::{prelude::BASE64_STANDARD, Engine};
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    use super::*;

    pub fn serialize<S>(m: &VRFProof, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        BASE64_STANDARD.encode(m.as_bytes()).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<VRFProof, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: Cow<'de, str> = Deserialize::deserialize(deserializer)?;
        Ok(VRFProof::new(
            BASE64_STANDARD
                .decode(s.as_ref())
                .map_err(de::Error::custom)?,
        ))
    }
}
