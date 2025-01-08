// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the logic for converting the mining reserve actor to a key-less account
//! actor. See the [FIP-0085](https://fips.filecoin.io/FIPS/fip-0085.html) for more details.

use crate::shim::address::Address;
use crate::shim::state_tree::ActorState;
use crate::state_migration::common::PostMigrator;
use crate::utils::db::CborStoreExt as _;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;

pub struct MiningReservePostMigrator {
    pub new_account_code_cid: Cid,
    pub new_multisig_code_cid: Cid,
}

impl<BS: Blockstore> PostMigrator<BS> for MiningReservePostMigrator {
    fn post_migrate_state(
        &self,
        store: &BS,
        actors_out: &mut crate::shim::state_tree::StateTree<BS>,
    ) -> anyhow::Result<()> {
        let f090_old_actor = actors_out.get_required_actor(&Address::RESERVE_ACTOR)?;
        // only migrate f090 if it is a `multisig`
        if f090_old_actor.code != self.new_multisig_code_cid {
            return Ok(());
        }
        let f090_new_state = fil_actor_account_state::v14::State {
            address: Address::RESERVE_ACTOR.into(),
        };
        let f090_new_state = store.put_cbor_default(&f090_new_state)?;

        actors_out.set_actor(
            &Address::RESERVE_ACTOR,
            ActorState::new(
                self.new_account_code_cid,
                f090_new_state,
                f090_old_actor.balance.clone().into(),
                f090_old_actor.sequence,
                None,
            ),
        )?;

        Ok(())
    }
}
