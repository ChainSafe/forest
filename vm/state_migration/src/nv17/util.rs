// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::{
    multihash::{Code::Blake2b256, MultihashDigest},
    Cid,
};
use forest_shim::{address::Address, deal::DealID};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::DAG_CBOR;
use fvm_ipld_hamt::BytesKey;

/// Translated from <https://github.com/filecoin-project/go-state-types/blob/master/builtin/v9/migration/util.go#L72>
pub(super) fn get_pending_verified_deals_and_total_size(
    store: &impl Blockstore,
    state: &fil_actor_market_state::v8::State,
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
pub(super) fn hamt_addr_key_to_key(addr_key: &BytesKey) -> anyhow::Result<BytesKey> {
    let addr = Address::from_bytes(addr_key)?;
    Ok(addr.payload_bytes().into())
}
