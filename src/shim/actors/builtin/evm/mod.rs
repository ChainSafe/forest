// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::lotus_json::HasLotusJson;
use ::cid::Cid;
use fil_actor_evm_state::v17::{BytecodeHash, Tombstone, TransientData};
use fvm_shared2::address::Address;
use serde::Serialize;
use spire_enum::prelude::delegated_enum;

/// EVM actor method.
pub type Method = fil_actor_evm_state::v10::Method;

/// EVM actor state.
#[delegated_enum(impl_conversions)]
#[derive(Serialize, Debug)]
#[serde(untagged)]
pub enum State {
    V10(fil_actor_evm_state::v10::State),
    V11(fil_actor_evm_state::v11::State),
    V12(fil_actor_evm_state::v12::State),
    V13(fil_actor_evm_state::v13::State),
    V14(fil_actor_evm_state::v14::State),
    V15(fil_actor_evm_state::v15::State),
    V16(fil_actor_evm_state::v16::State),
    V17(fil_actor_evm_state::v17::State),
}

impl State {
    pub fn default_latest_version(
        bytecode: Cid,
        bytecode_hash: [u8; 32],
        contract_state: Cid,
        transient_data: Option<TransientData>,
        nonce: u64,
        tombstone: Option<Tombstone>,
    ) -> Self {
        State::V17(fil_actor_evm_state::v17::State {
            bytecode,
            bytecode_hash: BytecodeHash::from(bytecode_hash),
            contract_state,
            transient_data,
            nonce,
            tombstone,
        })
    }

    pub fn nonce(&self) -> u64 {
        delegate_state!(self.nonce)
    }

    pub fn is_alive(&self) -> bool {
        delegate_state!(self.tombstone.is_none())
    }

    pub fn bytecode(&self) -> Cid {
        delegate_state!(self.bytecode)
    }

    pub fn bytecode_hash(&self) -> [u8; 32] {
        delegate_state!(self.bytecode_hash.into())
    }

    pub fn contract_state(&self) -> Cid {
        delegate_state!(self.contract_state)
    }
}

#[delegated_enum(impl_conversions)]
#[derive(Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum TombstoneState {
    V10(fil_actor_evm_state::v10::Tombstone),
    V11(fil_actor_evm_state::v11::Tombstone),
    V12(fil_actor_evm_state::v12::Tombstone),
    V13(fil_actor_evm_state::v13::Tombstone),
    V14(fil_actor_evm_state::v14::Tombstone),
    V15(fil_actor_evm_state::v15::Tombstone),
    V16(fil_actor_evm_state::v16::Tombstone),
    V17(fil_actor_evm_state::v17::Tombstone),
}

impl TombstoneState {
    pub fn default_latest_version(origin: fvm_shared4::ActorID, nonce: u64) -> Self {
        TombstoneState::V17(fil_actor_evm_state::v17::Tombstone { origin, nonce })
    }
}
