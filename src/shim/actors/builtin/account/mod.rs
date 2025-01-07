// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::super::convert::{from_address_v3_to_v2, from_address_v4_to_v2};
use fvm_shared2::address::Address;
use serde::Serialize;

/// Account actor method.
pub type Method = fil_actor_account_state::v8::Method;

/// Account actor state.
#[derive(Serialize, Debug)]
#[serde(untagged)]
pub enum State {
    V8(fil_actor_account_state::v8::State),
    V9(fil_actor_account_state::v9::State),
    V10(fil_actor_account_state::v10::State),
    V11(fil_actor_account_state::v11::State),
    V12(fil_actor_account_state::v12::State),
    V13(fil_actor_account_state::v13::State),
    V14(fil_actor_account_state::v14::State),
    V15(fil_actor_account_state::v15::State),
    V16(fil_actor_account_state::v16::State),
}

impl State {
    pub fn pubkey_address(&self) -> Address {
        match self {
            State::V8(st) => st.address,
            State::V9(st) => st.address,
            State::V10(st) => from_address_v3_to_v2(st.address),
            State::V11(st) => from_address_v3_to_v2(st.address),
            State::V12(st) => from_address_v4_to_v2(st.address),
            State::V13(st) => from_address_v4_to_v2(st.address),
            State::V14(st) => from_address_v4_to_v2(st.address),
            State::V15(st) => from_address_v4_to_v2(st.address),
            State::V16(st) => from_address_v4_to_v2(st.address),
        }
    }
}
