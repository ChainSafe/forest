// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use fvm_shared2::address::Address;
use serde::Serialize;
use spire_enum::prelude::delegated_enum;
use std::borrow::Borrow as _;

/// System actor address.
pub const ADDRESS: Address = Address::new_id(0);

/// System actor method.
pub type Method = fil_actor_system_state::v8::Method;

/// System actor state.
#[delegated_enum(impl_conversions)]
#[derive(Serialize, Debug)]
#[serde(untagged)]
pub enum State {
    V8(fil_actor_system_state::v8::State),
    V9(fil_actor_system_state::v9::State),
    V10(fil_actor_system_state::v10::State),
    V11(fil_actor_system_state::v11::State),
    V12(fil_actor_system_state::v12::State),
    V13(fil_actor_system_state::v13::State),
    V14(fil_actor_system_state::v14::State),
    V15(fil_actor_system_state::v15::State),
    V16(fil_actor_system_state::v16::State),
    V17(fil_actor_system_state::v17::State),
}

impl State {
    pub fn default_latest_version(builtin_actors: cid::Cid) -> Self {
        State::V17(fil_actor_system_state::v17::State { builtin_actors })
    }

    /// Returns the builtin actors Cid.
    pub fn builtin_actors_cid(&self) -> &Cid {
        delegate_state!(self.builtin_actors.borrow())
    }
}
