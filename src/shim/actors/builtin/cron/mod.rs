// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::lotus_json::HasLotusJson;
use crate::shim;
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

impl State {
    pub fn default_latest_version_from_entries(entries: Vec<Entry>) -> Self {
        let latest_entries = entries
            .into_iter()
            .map(|entry| entry.into_latest_inner())
            .collect();
        State::V17(fil_actor_cron_state::v17::State {
            entries: latest_entries,
        })
    }

    pub fn default_latest_version(entries: Vec<fil_actor_cron_state::v17::Entry>) -> Self {
        State::V17(fil_actor_cron_state::v17::State { entries })
    }
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

impl Entry {
    pub fn default_latest_version(
        receiver: fvm_shared4::address::Address,
        method_num: u64,
    ) -> Self {
        Entry::V17(fil_actor_cron_state::v17::Entry {
            receiver,
            method_num,
        })
    }

    pub fn into_latest_inner(self) -> fil_actor_cron_state::v17::Entry {
        let latest_entry = self.into_lotus_json();
        fil_actor_cron_state::v17::Entry {
            receiver: latest_entry.receiver.into(),
            method_num: latest_entry.method_num,
        }
    }
}
