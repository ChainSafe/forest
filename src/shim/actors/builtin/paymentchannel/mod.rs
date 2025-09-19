// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_shared2::{address::Address, clock::ChainEpoch, econ::TokenAmount};
use jsonrpsee::core::Serialize;

#[derive(Serialize, Debug)]
#[serde(untagged)]
pub enum State {
    V9(fil_actor_paych_state::v9::State),
    V10(fil_actor_paych_state::v10::State),
    V11(fil_actor_paych_state::v11::State),
    V12(fil_actor_paych_state::v12::State),
    V13(fil_actor_paych_state::v13::State),
    V14(fil_actor_paych_state::v14::State),
    V15(fil_actor_paych_state::v15::State),
    V16(fil_actor_paych_state::v16::State),
    V17(fil_actor_paych_state::v17::State),
}

impl State {
    pub fn default_latest_version(
        from: fvm_shared4::address::Address,
        to: fvm_shared4::address::Address,
        to_send: fvm_shared4::econ::TokenAmount,
        settling_at: ChainEpoch,
        min_settle_height: ChainEpoch,
        lane_states: cid::Cid,
    ) -> Self {
        State::V17(fil_actor_paych_state::v17::State {
            from,
            to,
            to_send,
            settling_at,
            min_settle_height,
            lane_states,
        })
    }
}
