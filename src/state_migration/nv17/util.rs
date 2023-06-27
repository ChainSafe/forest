// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    shim::{address::Address, deal::DealID},
    utils::cid::CidCborExt,
};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
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

        let pcid = Cid::from_cbor_blake2b256(proposal)?;

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

// TODO: Switch to `fil_actors_shared::v9::util::hamt_addr_key_to_key` once
// it's released. See <https://github.com/ChainSafe/fil-actor-states/pull/122>
pub(super) fn hamt_addr_key_to_key(addr_key: &BytesKey) -> anyhow::Result<BytesKey> {
    let addr = Address::from_bytes(addr_key)?;
    Ok(addr.payload_bytes().into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::*;
    use fvm_shared::{
        bigint::Zero,
        commcid::{FIL_COMMITMENT_UNSEALED, SHA2_256_TRUNC254_PADDED},
        piece::PaddedPieceSize,
    };
    use multihash::{Multihash, MultihashDigest};

    // Go parity test
    //
    // ```go
    // func TestGetPendingVerifiedDealsAndTotalSize(t *testing.T) {
    // 	ctx := context.Background()
    // 	bs := cbor.NewCborStore(NewSyncBlockStoreInMemory())
    // 	store := adt.WrapStore(ctx, bs)
    // 	marketState8, err := market8.ConstructState(store)
    // 	require.NoError(t, err)

    // 	baseAddrId, err := address.NewIDAddress(10000)
    // 	require.NoError(t, err)
    // 	baseDeal := market8.DealProposal{
    // 		PieceCID:             cid.Undef,
    // 		PieceSize:            512,
    // 		VerifiedDeal:         true,
    // 		Client:               baseAddrId,
    // 		Provider:             baseAddrId,
    // 		Label:                market8.EmptyDealLabel,
    // 		StartEpoch:           0,
    // 		EndEpoch:             0,
    // 		StoragePricePerEpoch: big.Zero(),
    // 		ProviderCollateral:   big.Zero(),
    // 		ClientCollateral:     big.Zero(),
    // 	}

    // 	deal0 := baseDeal
    // 	deal0.PieceCID = MakeCID("0", &market8.PieceCIDPrefix)
    // 	require.NoError(t, err)

    // 	deal1 := baseDeal
    // 	deal1.PieceCID = MakeCID("1", &market8.PieceCIDPrefix)
    // 	require.NoError(t, err)

    // 	deal2 := baseDeal
    // 	deal2.PieceCID = MakeCID("2", &market8.PieceCIDPrefix)
    // 	require.NoError(t, err)

    // 	proposals, err := market8.AsDealProposalArray(store, marketState8.Proposals)
    // 	proposals.Set(abi.DealID(100), &deal0)
    // 	proposals.Set(abi.DealID(101), &deal1)
    // 	proposals.Set(abi.DealID(102), &deal2)

    // 	proposalsCID, _ := proposals.Root()
    // 	fmt.Printf("pendingVerifiedDealSize proposalsCID: %s\n", proposalsCID)
    // 	marketState8.Proposals = proposalsCID

    // 	pendingProposals, _ := adt.AsSet(store, marketState8.PendingProposals, 5)
    // 	deal1CID, _ := deal1.Cid()
    // 	pendingProposals.Put(abi.CidKey(deal1CID))
    // 	deal2CID, _ := deal2.Cid()
    // 	pendingProposals.Put(abi.CidKey(deal2CID))
    // 	pendingProposalsCID, _ := pendingProposals.Root()
    // 	fmt.Printf("pendingVerifiedDealSize pendingProposalsCID: %s\n", pendingProposalsCID)
    // 	marketState8.PendingProposals = pendingProposalsCID

    // 	pendingVerifiedDeals, pendingVerifiedDealSize, err := migration.GetPendingVerifiedDealsAndTotalSize(ctx, store, *marketState8)
    // 	require.NoError(t, err)
    // 	fmt.Printf("pendingVerifiedDealSize: %d\n", pendingVerifiedDealSize)
    // 	for _, dealId := range pendingVerifiedDeals {
    // 		fmt.Printf("pendingVerifiedDeals dealId: %d\n", dealId)
    // 	}
    // }
    // ```
    #[test]
    fn test_get_pending_verified_deals_and_total_size() -> Result<()> {
        let store = crate::db::MemoryDB::default();
        let mut market_state = fil_actor_market_state::v8::State::new(&store)?;

        let mut pending_proposals = fil_actors_shared::v8::Set::new(&store);
        market_state.proposals = {
            let mut proposals = fil_actors_shared::v8::Array::<
                fil_actor_market_state::v8::DealProposal,
                _,
            >::new_with_bit_width(&store, 5);
            let base_deal = fil_actor_market_state::v8::DealProposal {
                piece_cid: Default::default(),
                piece_size: PaddedPieceSize(512),
                verified_deal: true,
                client: Address::new_id(10000).into(),
                provider: Address::new_id(10000).into(),
                label: fil_actor_market_state::v8::Label::String("".into()),
                start_epoch: 0,
                end_epoch: 0,
                storage_price_per_epoch: Zero::zero(),
                provider_collateral: Zero::zero(),
                client_collateral: Zero::zero(),
            };
            let deal0 = {
                let mut deal = base_deal.clone();
                deal.piece_cid = make_piece_cid("0".as_bytes())?;
                deal
            };
            let deal1 = {
                let mut deal = base_deal.clone();
                deal.piece_cid = make_piece_cid("1".as_bytes())?;
                deal
            };
            let deal2 = {
                let mut deal = base_deal;
                deal.piece_cid = make_piece_cid("2".as_bytes())?;
                deal
            };

            proposals.set(100, deal0)?;
            pending_proposals.put(BytesKey(deal1.cid()?.to_bytes()))?;
            proposals.set(101, deal1)?;
            pending_proposals.put(BytesKey(deal2.cid()?.to_bytes()))?;
            proposals.set(102, deal2)?;

            proposals.flush()?
        };
        market_state.pending_proposals = pending_proposals.root()?;
        ensure!(
            market_state.pending_proposals.to_string()
                == "bafy2bzaceaznfegva7wvkm3yd66r5ej7t7726pr6lwhnosxbslmmkuoymtvtw"
        );
        ensure!(
            market_state.proposals.to_string()
                == "bafy2bzaceck7at6aj7iy4s4gkndk5njvcba4yoveucxcdpfuwdeczaw3fcly2"
        );

        let (pending_verified_deals, pending_verified_deal_size) =
            get_pending_verified_deals_and_total_size(&store, &market_state)?;

        ensure!(pending_verified_deal_size == 1024);
        ensure!(pending_verified_deals == vec![101, 102]);

        Ok(())
    }

    fn make_piece_cid(data: &[u8]) -> Result<Cid> {
        let hash = cid::multihash::Code::Sha2_256.digest(data);
        let hash = Multihash::wrap(SHA2_256_TRUNC254_PADDED, hash.digest())?;
        Ok(Cid::new_v1(FIL_COMMITMENT_UNSEALED, hash))
    }
}
