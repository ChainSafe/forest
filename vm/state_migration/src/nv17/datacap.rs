// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV17` upgrade for the datacap
//! actor.

use std::{str::FromStr, sync::Arc};

use cid::multihash::Code::Blake2b256;
use forest_shim::{bigint::StoragePowerV2, econ::TokenAmount_v2};
use forest_utils::db::CborStoreExt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_hamt::BytesKey;

use crate::common::{ActorMigration, ActorMigrationInput, ActorMigrationOutput};

use super::util::hamt_addr_key_to_key;

const TOKEN_PRECISION: u64 = 1_000_000_000_000_000_000;
const DATA_CAP_GRANULARITY: u64 = TOKEN_PRECISION;
lazy_static::lazy_static! {
    static ref INFINITE_ALLOWANCE: StoragePowerV2 = StoragePowerV2::from_str("1000000000000000000000").expect("Failed to parse INFINITE_ALLOWANCE") * TOKEN_PRECISION;
}

pub struct DataCapMigrator {
    verifreg_state: fil_actor_verifreg_state::v8::State,
    pending_verified_deal_size: u64,
}

pub(crate) fn datacap_migrator<BS: Blockstore + Clone + Send + Sync>(
    verifreg_state: fil_actor_verifreg_state::v8::State,
    pending_verified_deal_size: u64,
) -> anyhow::Result<Arc<dyn ActorMigration<BS> + Send + Sync>> {
    Ok(Arc::new(DataCapMigrator {
        verifreg_state,
        pending_verified_deal_size,
    }))
}

impl<BS: Blockstore + Clone + Send + Sync> ActorMigration<BS> for DataCapMigrator {
    fn migrate_state(
        &self,
        store: BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<ActorMigrationOutput> {
        let verified_clients = fil_actors_shared::v8::make_map_with_root::<_, StoragePowerV2>(
            &self.verifreg_state.verified_clients,
            &store,
        )?;

        let mut token_supply = StoragePowerV2::default();

        let mut balances_map = fil_actors_shared::v9::make_empty_map::<_, StoragePowerV2>(
            &store,
            fil_actors_shared::v9::builtin::HAMT_BIT_WIDTH,
        );

        let mut allowances_map = fil_actors_shared::v9::make_empty_map(
            &store,
            fil_actors_shared::v9::builtin::HAMT_BIT_WIDTH,
        );

        verified_clients.for_each(|addr_key, value| {
        let key = hamt_addr_key_to_key(addr_key)?;
        let token_amount = value * DATA_CAP_GRANULARITY;
        token_supply = &token_supply + &token_amount;
        balances_map.set(key.clone(), token_amount)?;

        let mut allowances_map_entry = fil_actors_shared::v9::make_empty_map::<_, StoragePowerV2>(
            &store,
            fil_actors_shared::v9::builtin::HAMT_BIT_WIDTH,
        );
        allowances_map_entry.set(
            BytesKey(fil_actors_shared::v9::builtin::STORAGE_MARKET_ACTOR_ADDR.payload_bytes()),
            INFINITE_ALLOWANCE.clone(),
        )?;
        allowances_map.set(key, allowances_map_entry.flush()?)?;
        Ok(())
    })?;

        let verifreg_balance =
            StoragePowerV2::from(self.pending_verified_deal_size) * DATA_CAP_GRANULARITY;
        token_supply = &token_supply + &verifreg_balance;
        balances_map.set(
            BytesKey(fil_actors_shared::v9::builtin::VERIFIED_REGISTRY_ACTOR_ADDR.payload_bytes()),
            verifreg_balance,
        )?;

        let mut token = frc46_token::token::state::TokenState::new_with_bit_width(
            &store,
            fil_actors_shared::v9::builtin::HAMT_BIT_WIDTH,
        )?;
        token.supply = TokenAmount_v2::from_atto(token_supply);
        token.balances = balances_map.flush()?;
        token.allowances = allowances_map.flush()?;

        let datacap_state = fil_actor_datacap_state::v9::State {
            governor: fil_actors_shared::v9::builtin::VERIFIED_REGISTRY_ACTOR_ADDR,
            token,
        };

        let new_head = store.put_cbor_default(&datacap_state)?;

        Ok(ActorMigrationOutput {
            new_code_cid: input.head,
            new_head,
        })
    }
}
