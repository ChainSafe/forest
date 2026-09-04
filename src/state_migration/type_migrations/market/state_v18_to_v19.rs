// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::state_migration::common::{TypeMigration, TypeMigrator};
use fil_actor_market_state::{v18::State as MarketStateV18, v19::State as MarketStateV19};
use fvm_ipld_blockstore::Blockstore;

impl TypeMigration<MarketStateV18, MarketStateV19> for TypeMigrator {
    fn migrate_type(from: MarketStateV18, _: &impl Blockstore) -> anyhow::Result<MarketStateV19> {
        // FIP-0118 drops `pending_deal_allocation_ids`: verified allocations no longer take part
        // in deal activation, so nothing reads them again.
        // https://github.com/filecoin-project/go-state-types/blob/6cb27cf2e8be76d9b20f0d58d6d580cd99e31ce6/builtin/v19/migration/market.go#L33-L49
        Ok(MarketStateV19 {
            proposals: from.proposals,
            states: from.states,
            pending_proposals: from.pending_proposals,
            escrow_table: from.escrow_table,
            locked_table: from.locked_table,
            next_id: from.next_id,
            deal_ops_by_epoch: from.deal_ops_by_epoch,
            last_cron: from.last_cron,
            total_client_locked_collateral: from.total_client_locked_collateral,
            total_provider_locked_collateral: from.total_provider_locked_collateral,
            total_client_storage_fee: from.total_client_storage_fee,
            provider_sectors: from.provider_sectors,
        })
    }
}
