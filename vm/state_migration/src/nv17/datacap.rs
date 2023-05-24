// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{str::FromStr, sync::Arc};

use anyhow::Context;
use cid::{
    multihash::{Code::Blake2b256, MultihashDigest},
    Cid,
};
use forest_shim::{
    address::Address,
    bigint::StoragePowerV2,
    deal::DealID,
    econ::TokenAmount_v2,
    state_tree::{ActorState, StateTree},
};
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{CborStore, DAG_CBOR};
use fvm_ipld_hamt::BytesKey;

use crate::common::{ActorMigration, ActorMigrationInput, ActorMigrationOutput};

const TOKEN_PRECISION: u64 = 1_000_000_000_000_000_000;
const DATA_CAP_GRANULARITY: u64 = TOKEN_PRECISION;
lazy_static::lazy_static! {
    static ref INFINITE_ALLOWANCE: StoragePowerV2 = StoragePowerV2::from_str("1000000000000000000000").expect("Failed to parse INFINITE_ALLOWANCE") * TOKEN_PRECISION;
}

pub struct DataCapMigrator {
    verifreg_actor: ActorState,
    market_actor: ActorState,
}

pub(crate) fn datacap_migrator<BS: Blockstore + Clone + Send + Sync>(
    state_tree: &StateTree<BS>,
) -> anyhow::Result<Arc<dyn ActorMigration<BS> + Send + Sync>> {
    let verifreg_actor = state_tree
        .get_actor(&Address::new_id(
            fil_actors_shared::v8::VERIFIED_REGISTRY_ACTOR_ADDR.id()?,
        ))?
        .context("Failed to load verifreg actor v8")?;

    // load market proposals
    let market_actor = state_tree
        .get_actor(&Address::new_id(
            fil_actors_shared::v8::STORAGE_MARKET_ACTOR_ADDR.id()?,
        ))?
        .context("Failed to load market actor v8")?;

    Ok(Arc::new(DataCapMigrator {
        verifreg_actor,
        market_actor,
    }))
}

impl<BS: Blockstore + Clone + Send + Sync> ActorMigration<BS> for DataCapMigrator {
    fn migrate_state(
        &self,
        store: BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<ActorMigrationOutput> {
        let verifreg_state: fil_actor_verifreg_state::v8::State = store
            .get_cbor(&self.verifreg_actor.state)?
            .context("Failed to load verifreg state v8")?;
        let verified_clients = fil_actors_shared::v8::make_map_with_root::<_, StoragePowerV2>(
            &verifreg_state.verified_clients,
            &store,
        )?;

        let market_state: fil_actor_market_state::v8::State = store
            .get_cbor(&self.market_actor.state)?
            .context("Failed to load market state v8")?;

        let (pending_verified_deals, pending_verified_deal_size) =
            get_pending_verified_deals_and_total_size(&store, market_state)?;

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
            StoragePowerV2::from(pending_verified_deal_size) * DATA_CAP_GRANULARITY;
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

        let new_head = store.put_obj(&datacap_state, Blake2b256)?;

        Ok(ActorMigrationOutput {
            new_code_cid: input.head,
            new_head,
        })
    }
}

/// Translated from <https://github.com/filecoin-project/go-state-types/blob/master/builtin/v9/migration/util.go#L72>
fn get_pending_verified_deals_and_total_size(
    store: &impl Blockstore,
    state: fil_actor_market_state::v8::State,
) -> anyhow::Result<(Vec<DealID>, u64)> {
    let pending_proposals = fil_actors_shared::v8::Set::from_root(store, &state.pending_proposals)?;
    let proposals =
        fil_actors_shared::v8::Array::<fil_actor_market_state::v8::DealProposal, _>::load(
            &state.proposals,
            store,
        )?;
    let deal_states =
        fil_actors_shared::v9::Array::<fil_actor_market_state::v8::DealState, _>::load(
            &state.states,
            store,
        )?;

    let mut pending_verified_deals = vec![];
    let mut pending_size = 0;

    proposals.for_each(|deal_id, proposal| {
        // Nothing to do for unverified deals
        if !proposal.verified_deal {
            return Ok(());
        }

        // TODO: Switch to `proposal.cid()` once it's released.
        // See <https://github.com/ChainSafe/fil-actor-states/pull/120>
        let pcid = {
            let bytes = fvm_ipld_encoding::to_vec(proposal)?;
            Ok::<_, anyhow::Error>(Cid::new_v1(DAG_CBOR, Blake2b256.digest(&bytes)))
        }?;

        // Nothing to do for not-pending deals
        if !pending_proposals.has(&pcid.to_bytes())? {
            return Ok(());
        }

        // the deal has an entry in deal states, which means it's already been
        // allocated, nothing to do
        if deal_states.get(deal_id)?.is_some() {
            return Ok(());
        }

        pending_verified_deals.push(deal_id);
        pending_size += proposal.piece_size.0;

        Ok(())
    })?;

    Ok((pending_verified_deals, pending_size))
}

/// TODO: Switch to `fil_actors_shared::v9::util::hamt_addr_key_to_key` once
/// it's released. See <https://github.com/ChainSafe/fil-actor-states/pull/122>
pub fn hamt_addr_key_to_key(addr_key: &BytesKey) -> anyhow::Result<BytesKey> {
    let addr = Address::from_bytes(addr_key)?;
    Ok(addr.payload_bytes().into())
}
