// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::address::{Address, Payload};
use anyhow::anyhow;
use fil_actor_datacap_state::v12::DATACAP_GRANULARITY;
use fil_actors_shared::ext::TokenStateExt;
use fil_actors_shared::frc46_token::token::state::TokenState;
use fvm_ipld_blockstore::Blockstore;
use num::BigInt;
use num::traits::Euclid;
use serde::Serialize;
use spire_enum::prelude::delegated_enum;

/// Datacap actor method.
pub type Method = fil_actor_datacap_state::v10::Method;

/// Datacap actor address.
pub const ADDRESS: Address = Address::new_id(7);

/// Datacap actor state.
#[delegated_enum(impl_conversions)]
#[derive(Serialize, Debug)]
#[serde(untagged)]
pub enum State {
    V9(fil_actor_datacap_state::v9::State),
    V10(fil_actor_datacap_state::v10::State),
    V11(fil_actor_datacap_state::v11::State),
    V12(fil_actor_datacap_state::v12::State),
    V13(fil_actor_datacap_state::v13::State),
    V14(fil_actor_datacap_state::v14::State),
    V15(fil_actor_datacap_state::v15::State),
    V16(fil_actor_datacap_state::v16::State),
    V17(fil_actor_datacap_state::v17::State),
}

impl State {
    pub fn default_latest_version(
        governor: fvm_shared4::address::Address,
        token: TokenState,
    ) -> Self {
        State::V17(fil_actor_datacap_state::v17::State { governor, token })
    }

    // NOTE: This code currently mimics that of Lotus and is only used for RPC compatibility.
    pub fn verified_client_data_cap<BS>(
        &self,
        store: &BS,
        addr: Address,
    ) -> anyhow::Result<Option<BigInt>>
    where
        BS: Blockstore,
    {
        let id = match addr.payload() {
            Payload::ID(id) => Ok(*id),
            _ => Err(anyhow!("can only look up ID addresses")),
        }?;
        let int = delegate_state!(self.token.get_balance_opt(store, id)?);
        Ok(int
            .map(|amount| amount.atto().to_owned())
            .map(|opt| opt.div_euclid(&BigInt::from(DATACAP_GRANULARITY))))
    }
}
