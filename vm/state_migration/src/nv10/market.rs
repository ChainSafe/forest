// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::nv10::util::{migrate_amt_raw, migrate_hamt_hamt_raw, migrate_hamt_raw};
use crate::{
    ActorMigration, ActorMigrationInput, MigrationError, MigrationOutput, MigrationResult,
};
use actor::BalanceTable;
use actor::{Set, BALANCE_TABLE_BITWIDTH};
use actor_interface::actorv2::market::State as V2MarketState;
use actor_interface::actorv3;
use actor_interface::actorv3::market::State as V3MarketState;
use actor_interface::actorv3::market::{
    DealProposal, DealState, PROPOSALS_AMT_BITWIDTH, STATES_AMT_BITWIDTH,
};
use actor_interface::{ActorVersion, Array, Map as Map2};
use address::Address;
use async_std::sync::Arc;
use cid::{Cid, Code};
use fil_types::HAMT_BIT_WIDTH;
use forest_hash_utils::BytesKey;
use ipld_blockstore::BlockStore;
use num_bigint::bigint_ser::BigIntDe;

struct MarketMigrator;

impl<BS: BlockStore + Send + Sync> ActorMigration<BS> for MarketMigrator {
    fn migrate_state(
        &self,
        store: Arc<BS>,
        input: ActorMigrationInput,
    ) -> MigrationResult<MigrationOutput> {
        let v2_in_state: Option<V2MarketState> = store
            .get(&input.head)
            .map_err(|e| MigrationError::BlockStoreRead(e.to_string()))?;

        let v2_in_state = v2_in_state.unwrap();

        let pending_proposals_cid_out = self
            .map_pending_proposals(&*store, v2_in_state.pending_proposals)
            .map_err(|_| MigrationError::Other)?;

        let proposals_cid_out = migrate_amt_raw::<_, DealProposal>(
            &*store,
            v2_in_state.proposals,
            PROPOSALS_AMT_BITWIDTH as i32,
        )
        .map_err(|_| MigrationError::Other)?;

        let states_cid_out = migrate_amt_raw::<_, DealState>(
            &*store,
            v2_in_state.states,
            STATES_AMT_BITWIDTH as i32,
        )
        .map_err(|_| MigrationError::Other)?;

        let escrow_table_cid_out = migrate_hamt_raw::<_, BigIntDe>(
            &*store,
            v2_in_state.escrow_table,
            BALANCE_TABLE_BITWIDTH,
        )
        .map_err(|_| MigrationError::Other)?;

        let locked_table_cid_out = migrate_hamt_raw::<_, BigIntDe>(
            &*store,
            v2_in_state.locked_table,
            BALANCE_TABLE_BITWIDTH,
        )
        .map_err(|_| MigrationError::Other)?;

        let dobe_cid_out = migrate_hamt_hamt_raw::<_, Address>(
            &*store,
            v2_in_state.deal_ops_by_epoch,
            HAMT_BIT_WIDTH,
            HAMT_BIT_WIDTH,
        )
        .map_err(|_| MigrationError::Other)?;

        let out_state = V3MarketState {
            proposals: proposals_cid_out,
            states: states_cid_out,
            pending_proposals: pending_proposals_cid_out,
            escrow_table: escrow_table_cid_out,
            locked_table: locked_table_cid_out,
            next_id: v2_in_state.next_id,
            deal_ops_by_epoch: dobe_cid_out,
            last_cron: v2_in_state.last_cron,
            total_client_locked_colateral: v2_in_state.total_client_locked_colateral,
            total_provider_locked_colateral: v2_in_state.total_provider_locked_colateral,
            total_client_storage_fee: v2_in_state.total_client_storage_fee,
        };

        let new_head = store
            .put(&out_state, Code::Blake2b256)
            .map_err(|_| MigrationError::Other)?;

        Ok(MigrationOutput {
            new_code_cid: *actorv3::MARKET_ACTOR_CODE_ID,
            new_head,
        })
    }
}

impl MarketMigrator {
    fn map_pending_proposals<BS: BlockStore>(
        &self,
        store: &BS,
        pending_proposals_root: Cid,
    ) -> MigrationResult<Cid> {
        // DealCid
        let old_pending_proposals =
            Map2::<_, Cid>::load(&pending_proposals_root, store, ActorVersion::V2)
                .map_err(|e| MigrationError::HAMTLoad(e.to_string()))?;

        let mut new_pending_proposals = Set::new_set_with_bitwidth(store, HAMT_BIT_WIDTH);

        old_pending_proposals
            .for_each(|k: &BytesKey, _| {
                new_pending_proposals.put(k.clone())?;
                Ok(())
            })
            .map_err(|_| MigrationError::Other)?; // FIXME error handling

        let new_pending_proposals_cid = new_pending_proposals
            .root()
            .map_err(|_| MigrationError::Other)?; // FIXME: error handling

        Ok(new_pending_proposals_cid)
    }
}
