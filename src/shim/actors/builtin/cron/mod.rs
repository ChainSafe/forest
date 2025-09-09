// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_shared2::address::Address;
use serde::Serialize;

/// Cron actor address.
pub const ADDRESS: Address = Address::new_id(3);

/// Cron actor method.
pub type Method = fil_actor_cron_state::v8::Method;

/// Cron actor state.
#[derive(Serialize, Debug)]
#[serde(untagged)]
pub enum State {
    V8(fil_actor_cron_state::v8::State),
    V9(fil_actor_cron_state::v9::State),
    V10(fil_actor_cron_state::v10::State),
    V11(fil_actor_cron_state::v11::State),
    V12(fil_actor_cron_state::v12::State),
    V13(fil_actor_cron_state::v13::State),
    V14(fil_actor_cron_state::v14::State),
    V15(fil_actor_cron_state::v15::State),
    V16(fil_actor_cron_state::v16::State),
    V17(fil_actor_cron_state::v17::State),
}

#[derive(Clone, Serialize, Debug)]
#[serde(untagged)]
pub enum Entry {
    V8(fil_actor_cron_state::v8::Entry),
    V9(fil_actor_cron_state::v9::Entry),
    V10(fil_actor_cron_state::v10::Entry),
    V11(fil_actor_cron_state::v11::Entry),
    V12(fil_actor_cron_state::v12::Entry),
    V13(fil_actor_cron_state::v13::Entry),
    V14(fil_actor_cron_state::v14::Entry),
    V15(fil_actor_cron_state::v15::Entry),
    V16(fil_actor_cron_state::v16::Entry),
    V17(fil_actor_cron_state::v17::Entry),
}
