// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

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
}
