// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::{self, address::Address};
use serde::Serialize;
use spire_enum::prelude::delegated_enum;

/// Account actor method.
pub type Method = fil_actor_account_state::v8::Method;

/// Account actor state.
#[derive(Serialize, Debug)]
#[serde(untagged)]
#[delegated_enum(impl_conversions)]
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
    V17(fil_actor_account_state::v17::State),
}

impl State {
    pub fn pubkey_address(&self) -> Address {
        delegate_state!(self.address.into())
    }

    pub fn default_latest_version(address: fvm_shared4::address::Address) -> Self {
        State::V17(fil_actor_account_state::v17::State { address })
    }
}
