// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod json {
    use cid::Cid;
    use fvm::state_tree::ActorState;
    use fvm_shared::econ::TokenAmount;
    use num_bigint::BigInt;
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
    use std::str::FromStr;

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
        Ok(ActorState {
            code,
            state,
            sequence,
            balance: TokenAmount::from_atto(BigInt::from_str(&balance).map_err(de::Error::custom)?),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::actor_state::json::{ActorStateJson, ActorStateJsonRef};
    use cid::Cid;
    use fvm::state_tree::ActorState;
    use fvm_shared::econ::TokenAmount;
    use quickcheck_macros::quickcheck;

    #[derive(Clone, Debug)]
    struct ActorStateWrapper {
        actorstate: ActorState,
    }

    impl quickcheck::Arbitrary for ActorStateWrapper {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let cid = Cid::new_v1(
                u64::arbitrary(g),
                cid::multihash::Multihash::wrap(u64::arbitrary(g), &[u8::arbitrary(g)]).unwrap(),
            );
            let actorstate = ActorState {
                code: cid,
                state: cid,
                sequence: u64::arbitrary(g),
                balance: TokenAmount::from_atto(u64::arbitrary(g)),
            };
            ActorStateWrapper { actorstate }
        }
    }

    #[quickcheck]
    fn actorstate_roundtrip(actorstate: ActorStateWrapper) {
        let serialized: String =
            serde_json::to_string(&ActorStateJsonRef(&actorstate.actorstate)).unwrap();
        let parsed: ActorStateJson = serde_json::from_str(&serialized).unwrap();
        assert_eq!(actorstate.actorstate, parsed.0);
    }
}
