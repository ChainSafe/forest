// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{str::FromStr, sync::Arc};

use anyhow::Context;
use cid::Cid;
use forest_shim::{
    address::Address,
    bigint::StoragePowerV2,
    deal::DealID,
    econ::{TokenAmount, TokenAmount_v2},
    state_tree::StateTree,
};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use fvm_ipld_hamt::BytesKey;

use crate::common::{
    ActorMigration, ActorMigrationInput, ActorMigrationOutput, TypeMigration, TypeMigrator,
};

const TOKEN_PRECISION: u64 = 1_000_000_000_000_000_000;
const DATA_CAP_GRANULARITY: u64 = TOKEN_PRECISION;
lazy_static::lazy_static! {
    static ref INFINITE_ALLOWANCE: StoragePowerV2 = StoragePowerV2::from_str("1000000000000000000000").expect("Failed to parse INFINITE_ALLOWANCE") * TOKEN_PRECISION;
}

pub struct DataCapMigrator(Cid);

pub(crate) fn datacap_migrator<BS: Blockstore + Clone + Send + Sync>(
    cid: Cid,
) -> Arc<dyn ActorMigration<BS> + Send + Sync> {
    Arc::new(DataCapMigrator(cid))
}

impl<BS: Blockstore + Clone + Send + Sync> ActorMigration<BS> for DataCapMigrator {
    fn migrate_state(
        &self,
        store: BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<ActorMigrationOutput> {
        // let verifreg_state_v8 = store
        //     .get_cbor(fil_actors_shared::v8::builtin::VERIFIED_REGISTRY_ACTOR_ADDR)?
        //     .context("Failed to retrieve verifreg state v8")?;
        // let verified_clients = fil_actors_shared::v9::make_map_with_root(root,
        // &store); The DataCap actor -- needs to be created, and loading the
        // verified registry state

        // let verif_ref_state_v8 = store.get(verifreg)
        // let verified_clients = Hamt::load_with_bit_width(cid, &store,
        // DEFAULT_HAMT_BITWIDTH);

        todo!()
    }
}

/// Creates the Ethereum Account Manager actor in the state tree.
pub fn create_datacap_actor<BS: Blockstore + Clone + Send + Sync>(
    store: &BS,
    actors_out: &mut StateTree<BS>,
) -> anyhow::Result<()> {
    // The DataCap actor -- needs to be created, and loading the verified registry
    // state
    let verifreg_actor = actors_out
        .get_actor(&Address::new_id(
            fil_actors_shared::v8::VERIFIED_REGISTRY_ACTOR_ADDR.id()?,
        ))?
        .context("Failed to load verifreg actor v8")?;
    let verifreg_state: fil_actor_verifreg_state::v8::State = store
        .get_cbor(&verifreg_actor.state)?
        .context("Failed to load verifreg state v8")?;
    let verified_clients = fil_actors_shared::v8::make_map_with_root::<_, StoragePowerV2>(
        &verifreg_state.verified_clients,
        store,
    )?;

    // load market proposals
    let market_actor = actors_out
        .get_actor(&Address::new_id(
            fil_actors_shared::v8::STORAGE_MARKET_ACTOR_ADDR.id()?,
        ))?
        .context("Failed to load market actor v8")?;
    let market_state: fil_actor_market_state::v8::State = store
        .get_cbor(&market_actor.state)?
        .context("Failed to load market state v8")?;

    let (pending_verified_deals, pending_verified_deal_size) =
        get_pending_verified_deals_and_total_size(store, market_state)?;

    let mut token_supply = StoragePowerV2::default();

    let mut balances_map = fil_actors_shared::v9::make_empty_map::<_, StoragePowerV2>(
        store,
        fil_actors_shared::v9::builtin::HAMT_BIT_WIDTH,
    );

    let mut allowances_map = fil_actors_shared::v9::make_empty_map(
        store,
        fil_actors_shared::v9::builtin::HAMT_BIT_WIDTH,
    );

    verified_clients.for_each(|key, value| {
        let key2 = BytesKey(key[1..].to_vec());
        let token_amount = &value * DATA_CAP_GRANULARITY;
        token_supply = &token_supply + &token_amount;
        balances_map.set(key2.clone(), token_amount)?;

        let mut allowances_map_entry = fil_actors_shared::v9::make_empty_map::<_, StoragePowerV2>(
            store,
            fil_actors_shared::v9::builtin::HAMT_BIT_WIDTH,
        );
        allowances_map_entry.set(
            BytesKey(fil_actors_shared::v9::builtin::STORAGE_MARKET_ACTOR_ADDR.payload_bytes()),
            INFINITE_ALLOWANCE.clone(),
        )?;
        allowances_map.set(key2, allowances_map_entry)?;
        Ok(())
    })?;

    let verifreg_balance = StoragePowerV2::from(pending_verified_deal_size) * DATA_CAP_GRANULARITY;
    token_supply = &token_supply + verifreg_balance;
    balances_map.set(
        BytesKey(fil_actors_shared::v9::builtin::VERIFIED_REGISTRY_ACTOR_ADDR.payload_bytes()),
        verifreg_balance,
    )?;

    let datacap_state = fil_actor_datacap_state::v9::State {
        governor: fil_actors_shared::v9::builtin::VERIFIED_REGISTRY_ACTOR_ADDR,
        token: frc46_token::token::state::TokenState {
            supply: TokenAmount_v2::from_atto(token_supply),
            balances: balances_map.flush()?,
            allowances: allowances_map.flush()?,
            hamt_bit_width: fil_actors_shared::v9::builtin::HAMT_BIT_WIDTH,
        },
    };

    Ok(())
}

fn get_pending_verified_deals_and_total_size(
    store: &impl Blockstore,
    state: fil_actor_market_state::v8::State,
) -> anyhow::Result<(Vec<DealID>, u64)> {
    todo!()
}
