// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV18` upgrade for the Init
//! actor.

use std::sync::Arc;

use cid::{multihash::Code::Blake2b256, Cid};
use fil_actor_power_v10::State as StateV10;
use fil_actor_power_v11::State as StateV11;
// TODO: use v11, but should somewhat work with v10
use fil_actors_runtime_v10::{
    make_empty_map, make_map_with_root_and_bitwidth, Map,
};
use fil_actors_runtime_v11::builtin::HAMT_BIT_WIDTH;
use forest_shim::{
    address::{Address, PAYLOAD_HASH_LEN},
    state_tree::ActorID,
};
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;
use fil_actor_miner_v11::convert_window_post_proof_v1p1_to_v1;
use fil_actor_power_v10::Claim as ClaimV10;
use fil_actor_power_v11::Claim as ClaimV11;

// TODO: get convert_window_post_proof_v1p1_to_v1 from v11 miner
use crate::common::{ActorMigration, ActorMigrationInput, ActorMigrationOutput};

pub struct PowerMigrator(Cid);

pub(crate) fn power_migrator<BS: Blockstore + Clone + Send + Sync>(
    cid: Cid,
) -> Arc<dyn ActorMigration<BS> + Send + Sync> {
    Arc::new(PowerMigrator(cid))
}

// original golang code: https://github.com/filecoin-project/go-state-types/blob/master/builtin/v11/migration/power.go
impl<BS: Blockstore + Clone + Send + Sync> ActorMigration<BS> for PowerMigrator {
    fn migrate_state(
        &self,
        store: BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<ActorMigrationOutput> {
        let in_state: StateV10 = store
            .get_obj(&input.head)?
            .ok_or_else(|| anyhow::anyhow!("Power actor: could not read v10 state"))?;

        //
        let in_claims = make_map_with_root_and_bitwidth(&in_state.claims, &store, HAMT_BIT_WIDTH)?;

        // TODO: should be v11
        let empty_claims = make_empty_map(&store, HAMT_BIT_WIDTH).flush()?;

        // TODO: should be v11
        let out_claims = make_map_with_root_and_bitwidth(&empty_claims, &store, HAMT_BIT_WIDTH)?;

        in_claims.for_each(|key, claim: &ClaimV10| {
            let address = Address::from_bytes(key)?;
            let new_proof_type = convert_window_post_proof_v1p1_to_v1(claim.window_post_proof_type)?;
            // TODO: use v11 Claim
            let out_claim = ClaimV11 {
                window_post_proof_type: new_proof_type,
                raw_byte_power: claim.raw_byte_power,
                quality_adj_power: claim.quality_adj_power,
            };
            out_claims.set(address.to_bytes().into(), out_claim)?;
            Ok(())
        })?;

        let out_claims_root = out_claims.flush()?;

        let out_state = StateV11 {
            // TODO: check if we need to pass the filter estimate
            claims: out_claims_root,
            ..in_state
        };

        let new_head = store.put_obj(&out_state, Blake2b256)?;

        Ok(ActorMigrationOutput {
            new_code_cid: self.0,
            new_head,
        })
    }
}
