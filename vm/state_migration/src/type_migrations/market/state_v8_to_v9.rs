// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::multihash::Code::Blake2b256;
use fil_actor_market_state::{v8::State as MarketStateV8, v9::State as MarketStateV9};
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_hamt::Hamt;

use crate::common::{adt, TypeMigration, TypeMigrator};

impl TypeMigration<MarketStateV8, MarketStateV9> for TypeMigrator {
    fn migrate_type(from: MarketStateV8, store: &impl Blockstore) -> anyhow::Result<MarketStateV9> {
        // https://github.com/filecoin-project/go-state-types/blob/master/builtin/shared.go#L15
        const DEFAULT_HAMT_BITWIDTH: u32 = 5;

        type CborInt = i64;

        let empty_map_cid = adt::store_empty_map(&store, DEFAULT_HAMT_BITWIDTH)?;
        let pending_deal_allocation_ids_map =
            Hamt::<_, CborInt>::load_with_bit_width(&empty_map_cid, &store, DEFAULT_HAMT_BITWIDTH)?;

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
