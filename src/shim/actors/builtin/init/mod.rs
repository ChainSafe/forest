// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_shared2::address::Address;
use serde::Serialize;
use spire_enum::prelude::delegated_enum;

/// Init actor address.
pub const ADDRESS: Address = Address::new_id(1);

/// Init actor method.
pub type Method = fil_actor_init_state::v8::Method;

/// Init actor state.
#[delegated_enum(impl_conversions)]
#[derive(Serialize, Debug)]
#[serde(untagged)]
pub enum State {
    V0(fil_actor_init_state::v0::State),
    V8(fil_actor_init_state::v8::State),
    V9(fil_actor_init_state::v9::State),
    V10(fil_actor_init_state::v10::State),
    V11(fil_actor_init_state::v11::State),
    V12(fil_actor_init_state::v12::State),
    V13(fil_actor_init_state::v13::State),
    V14(fil_actor_init_state::v14::State),
    V15(fil_actor_init_state::v15::State),
    V16(fil_actor_init_state::v16::State),
    V17(fil_actor_init_state::v17::State),
}

impl State {
    pub fn default_latest_version(
        address_map: ::cid::Cid,
        next_id: u64,
        network_name: String,
    ) -> Self {
        State::V17(fil_actor_init_state::v17::State {
            address_map,
            next_id,
            network_name,
        })
    }

    pub fn into_network_name(self) -> String {
        delegate_state!(self.network_name)
    }
}
