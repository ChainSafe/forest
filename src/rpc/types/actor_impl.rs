// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

impl ApiActorState {
    pub fn new(balance: TokenAmount, code: Cid, state: Ipld) -> Self {
        Self {
            balance,
            code,
            state: ApiState { state },
        }
    }
}

impl HasLotusJson for ActorState {
    type LotusJson = ActorStateJson;
    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![]
    }
    fn into_lotus_json(self) -> Self::LotusJson {
        ActorStateJson {
            code: self.code,
            head: self.state,
            nonce: self.sequence,
            balance: self.balance.clone().into(),
            address: self.delegated_address.map(|a| a.into()),
        }
    }
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        ActorState::new(
            lotus_json.code,
            lotus_json.head,
            lotus_json.balance,
            lotus_json.nonce,
            lotus_json.address,
        )
    }
}
