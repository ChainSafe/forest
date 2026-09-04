// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Market actor migration for FIP-0118: re-encodes the state without
//! `pending_deal_allocation_ids`.

use crate::state_migration::common::{
    ActorMigration, ActorMigrationInput, ActorMigrationOutput, TypeMigration, TypeMigrator,
};
use crate::utils::db::CborStoreExt as _;
use cid::Cid;
use fil_actor_market_state::v18::State as MarketStateOld;
use fil_actor_market_state::v19::State as MarketStateNew;
use fvm_ipld_blockstore::Blockstore;

pub struct MarketMigrator {
    pub new_code_cid: Cid,
}

impl<BS: Blockstore> ActorMigration<BS> for MarketMigrator {
    fn migrate_state(
        &self,
        store: &BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<Option<ActorMigrationOutput>> {
        let in_state: MarketStateOld = store.get_cbor_required(&input.head)?;
        let out_state: MarketStateNew = TypeMigrator::migrate_type(in_state, store)?;
        let new_head = store.put_cbor_default(&out_state)?;
        Ok(Some(ActorMigrationOutput {
            new_code_cid: self.new_code_cid,
            new_head,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::MemoryDB;
    use crate::utils::cid::CidCborExt as _;
    use fvm_ipld_encoding::CborStore as _;
    use fvm_shared4::econ::TokenAmount;

    // The dropped field sits mid-tuple, so a shifted field would show up as a wrong value
    // rather than a missing one.
    #[test]
    fn drops_pending_deal_allocation_ids_and_keeps_every_other_field() {
        let store = MemoryDB::default();
        let distinct_cid = |tag: u64| store.put_cbor_default(&tag).unwrap();
        let in_state = MarketStateOld {
            proposals: distinct_cid(1),
            states: distinct_cid(2),
            pending_proposals: distinct_cid(3),
            escrow_table: distinct_cid(4),
            locked_table: distinct_cid(5),
            next_id: 1234,
            deal_ops_by_epoch: distinct_cid(6),
            last_cron: 5678,
            total_client_locked_collateral: TokenAmount::from_atto(11),
            total_provider_locked_collateral: TokenAmount::from_atto(22),
            total_client_storage_fee: TokenAmount::from_atto(33),
            pending_deal_allocation_ids: distinct_cid(7),
            provider_sectors: distinct_cid(8),
        };
        let head = store.put_cbor_default(&in_state).unwrap();
        let new_code_cid = Cid::from_cbor_blake2b256(&"market v19 code").unwrap();

        let output = MarketMigrator { new_code_cid }
            .migrate_state(&store, ActorMigrationInput::for_head(head))
            .unwrap()
            .unwrap();

        assert_eq!(output.new_code_cid, new_code_cid);
        let out_state: MarketStateNew = store.get_cbor_required(&output.new_head).unwrap();
        let expected = MarketStateNew {
            proposals: in_state.proposals,
            states: in_state.states,
            pending_proposals: in_state.pending_proposals,
            escrow_table: in_state.escrow_table,
            locked_table: in_state.locked_table,
            next_id: in_state.next_id,
            deal_ops_by_epoch: in_state.deal_ops_by_epoch,
            last_cron: in_state.last_cron,
            total_client_locked_collateral: in_state.total_client_locked_collateral.clone(),
            total_provider_locked_collateral: in_state.total_provider_locked_collateral.clone(),
            total_client_storage_fee: in_state.total_client_storage_fee.clone(),
            provider_sectors: in_state.provider_sectors,
        };
        // `State` has no `PartialEq`.
        assert_eq!(format!("{out_state:?}"), format!("{expected:?}"));
        // The v19 tuple is one field shorter, so it no longer decodes as v18.
        assert!(store.get_cbor::<MarketStateOld>(&output.new_head).is_err());
    }
}
