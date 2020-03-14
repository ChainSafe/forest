// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use clock::ChainEpoch;
use runtime::Runtime;
use std::collections::HashMap;
use vm::TokenAmount;

pub struct Reward {
    pub start_epoch: ChainEpoch,
    pub value: TokenAmount,
    pub release_rate: TokenAmount,
    pub amount_withdrawn: TokenAmount,
}

/// RewardActorState has no internal state
pub struct RewardActorState {
    pub reward_map: HashMap<Address, Vec<Reward>>,
}

impl RewardActorState {
    pub fn withdraw_reward<RT: Runtime>(_rt: &RT, _owner: Address) -> TokenAmount {
        // TODO
        TokenAmount::new(0)
    }
}
