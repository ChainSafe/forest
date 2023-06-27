// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::super::super::common::{TypeMigration, TypeMigrator};
use fil_actor_market_state::{v8::State as MarketStateV8, v9::State as MarketStateV9};
use fvm_ipld_blockstore::Blockstore;

impl TypeMigration<MarketStateV8, MarketStateV9> for TypeMigrator {
    fn migrate_type(from: MarketStateV8, _: &impl Blockstore) -> anyhow::Result<MarketStateV9> {
        // https://github.com/filecoin-project/go-state-types/blob/master/builtin/v9/migration/market.go#L69
        let out_state = MarketStateV9 {
            proposals: from.proposals,
            pending_proposals: from.pending_proposals,
            escrow_table: from.escrow_table,
            locked_table: from.locked_table,
            next_id: from.next_id,
            deal_ops_by_epoch: from.deal_ops_by_epoch,
            last_cron: from.last_cron,
            total_client_locked_collateral: from.total_client_locked_collateral,
            total_provider_locked_collateral: from.total_provider_locked_collateral,
            total_client_storage_fee: from.total_client_storage_fee,

            // Changed
            states: from.states,
            pending_deal_allocation_ids: from.states,
        };

        Ok(out_state)
    }
}
