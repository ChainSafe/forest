// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod json {
    use std::str::FromStr;

    use cid::Cid;
    use forest_shim::econ::TokenAmount;
    use forest_shim::state_tree::ActorState;
    use num_bigint::BigInt;
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    /// Wrapper for serializing and de-serializing a `SignedMessage` from JSON.
    #[derive(Deserialize, Serialize, Clone, Debug)]
    #[serde(transparent)]
    pub struct ActorStateJson(#[serde(with = "self")] pub ActorState);

    /// Wrapper for serializing a `SignedMessage` reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct ActorStateJsonRef<'a>(#[serde(with = "self")] pub &'a ActorState);

    impl From<ActorStateJson> for ActorState {
        fn from(wrapper: ActorStateJson) -> Self {
            wrapper.0
        }
    }

    pub fn serialize<S>(m: &ActorState, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        #[serde(rename_all = "PascalCase")]
        struct ActorStateSer<'a> {
            #[serde(with = "crate::cid")]
            pub code: &'a Cid,
            #[serde(rename = "Head", with = "crate::cid")]
            pub state: &'a Cid,
            #[serde(rename = "Nonce")]
            pub sequence: u64,
            pub balance: String,
        }
        ActorStateSer {
            code: &m.code,
            state: &m.state,
            sequence: m.sequence,
            balance: m.balance.atto().to_str_radix(10),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ActorState, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct ActorStateDe {
            #[serde(with = "crate::cid")]
            pub code: Cid,
            #[serde(rename = "Head", with = "crate::cid")]
            pub state: Cid,
            #[serde(rename = "Nonce")]
            pub sequence: u64,
            pub balance: String,
        }
        let ActorStateDe {
            code,
            state,
            sequence,
            balance,
        } = Deserialize::deserialize(deserializer)?;
        Ok(ActorState::new(
            code,
            state,
            TokenAmount::from_atto(BigInt::from_str(&balance).map_err(de::Error::custom)?),
            sequence,
            None,
        ))
    }
}

#[cfg(test)]
mod tests {
    use forest_shim::state_tree::ActorState;
    use quickcheck_macros::quickcheck;

    use crate::actor_state::json::{ActorStateJson, ActorStateJsonRef};

    #[quickcheck]
    fn actorstate_roundtrip(actorstate: ActorState) {
        let serialized: String = serde_json::to_string(&ActorStateJsonRef(&actorstate)).unwrap();
        let parsed: ActorStateJson = serde_json::from_str(&serialized).unwrap();
        assert_eq!(actorstate, parsed.0);
    }
}
