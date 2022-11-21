use std::{
    collections::{BTreeMap, BTreeSet},
    convert::TryFrom,
    fmt::Debug,
};

use cid::Cid;
use fil_actors_runtime_v8::{
    make_map_with_root_and_bitwidth, parse_uint_key, MessageAccumulator, SetMultimap,
};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::Cbor;
use fvm_shared::{
    address::{Address, Protocol},
    clock::{ChainEpoch, EPOCH_UNDEFINED},
    deal::DealID,
    econ::TokenAmount,
};
use num_traits::Zero;

use crate::{balance_table::BalanceTable, DealArray, DealMetaArray, State, PROPOSALS_AMT_BITWIDTH};

pub struct DealSummary {
    pub provider: Address,
    pub start_epoch: ChainEpoch,
    pub end_epoch: ChainEpoch,
    pub sector_start_epoch: ChainEpoch,
    pub last_update_epoch: ChainEpoch,
    pub slash_epoch: ChainEpoch,
}

impl Default for DealSummary {
    fn default() -> Self {
        Self {
            provider: Address::new_id(0),
            start_epoch: 0,
            end_epoch: 0,
            sector_start_epoch: -1,
            last_update_epoch: -1,
            slash_epoch: -1,
        }
    }
}

#[derive(Default)]
pub struct StateSummary {
    pub deals: BTreeMap<DealID, DealSummary>,
    pub pending_proposal_count: u64,
    pub deal_state_count: u64,
    pub lock_table_count: u64,
    pub deal_op_epoch_count: u64,
    pub deal_op_count: u64,
}

/// Checks internal invariants of market state
pub fn check_state_invariants<BS: Blockstore + Debug>(
    state: &State,
    store: &BS,
    balance: &TokenAmount,
    current_epoch: ChainEpoch,
) -> (StateSummary, MessageAccumulator) {
    let acc = MessageAccumulator::default();

    acc.require(
        !state.total_client_locked_collateral.is_negative(),
        format!(
            "negative total client locked collateral: {}",
            state.total_client_locked_collateral
        ),
    );
    acc.require(
        !state.total_provider_locked_collateral.is_negative(),
        format!(
            "negative total provider locked collateral: {}",
            state.total_provider_locked_collateral
        ),
    );
    acc.require(
        !state.total_client_storage_fee.is_negative(),
        format!(
            "negative total client storage fee: {}",
            state.total_client_storage_fee
        ),
    );

    // Proposals
    let mut proposal_cids = BTreeSet::<Cid>::new();
    let mut max_deal_id = -1;
    let mut proposal_stats = BTreeMap::<DealID, DealSummary>::new();
    let mut expected_deal_ops = BTreeSet::<DealID>::new();
    let mut total_proposal_collateral = TokenAmount::zero();

    match DealArray::load(&state.proposals, store) {
        Ok(proposals) => {
            let ret = proposals.for_each(|deal_id, proposal| {
                let proposal_cid = proposal.cid()?;

                if proposal.start_epoch >= current_epoch {
                    expected_deal_ops.insert(deal_id);
                }

                // keep some state
                proposal_cids.insert(proposal_cid);
                max_deal_id = max_deal_id.max(deal_id as i64);

                proposal_stats.insert(
                    deal_id,
                    DealSummary {
                        provider: proposal.provider,
                        start_epoch: proposal.start_epoch,
                        end_epoch: proposal.end_epoch,
                        ..Default::default()
                    },
                );

                total_proposal_collateral +=
                    &proposal.client_collateral + &proposal.provider_collateral;

                acc.require(
                    proposal.client.protocol() == Protocol::ID,
                    "client address for deal {deal_id} is not an ID address",
                );
                acc.require(
                    proposal.provider.protocol() == Protocol::ID,
                    "provider address for deal {deal_id} is not an ID address",
                );
                Ok(())
            });
            acc.require_no_error(ret, "error iterating proposals");
        }
        Err(e) => acc.add(format!("error loading proposals: {e}")),
    };

    // next id should be higher than any existing deal
    acc.require(
        state.next_id as i64 > max_deal_id,
        format!(
            "next id, {}, is not greater than highest id in proposals, {max_deal_id}",
            state.next_id
        ),
    );

    // deal states
    let mut deal_state_count = 0;
    match DealMetaArray::load(&state.states, store) {
        Ok(deal_states) => {
            let ret = deal_states.for_each(|deal_id, deal_state| {
                acc.require(
                    deal_state.sector_start_epoch >= 0,
                    format!("deal {deal_id} state start epoch undefined: {:?}", deal_state),
                );
                acc.require(
                    deal_state.last_updated_epoch == EPOCH_UNDEFINED
                        || deal_state.last_updated_epoch >= deal_state.sector_start_epoch,
                    format!(
                        "deal {deal_id} state last updated before sector start: {deal_state:?}"
                    ),
                );
                acc.require(
                    deal_state.last_updated_epoch == EPOCH_UNDEFINED
                        || deal_state.last_updated_epoch <= current_epoch,
                    format!(
                        "deal {deal_id} last updated epoch {} after current {current_epoch}",
                        deal_state.last_updated_epoch
                    ),
                );
                acc.require(deal_state.slash_epoch == EPOCH_UNDEFINED || deal_state.slash_epoch >= deal_state.sector_start_epoch, format!("deal {deal_id} state slashed before sector start: {deal_state:?}"));
                acc.require(deal_state.slash_epoch == EPOCH_UNDEFINED || deal_state.slash_epoch <= current_epoch, format!("deal {deal_id} state slashed after current epoch {current_epoch}: {deal_state:?}"));

                if let Some(stats) = proposal_stats.get_mut(&deal_id) {
                    stats.sector_start_epoch = deal_state.sector_start_epoch;
                    stats.last_update_epoch = deal_state.last_updated_epoch;
                    stats.slash_epoch = deal_state.slash_epoch;
                } else {
                    acc.add(format!("no deal proposal for deal state {deal_id}"));
                }

                deal_state_count += 1;

                Ok(())
            });
            acc.require_no_error(ret, "error iterating deal states");
        }
        Err(e) => acc.add(format!("error loading deal states: {e}")),
    };

    // pending proposals
    let mut pending_proposal_count = 0;

    match make_map_with_root_and_bitwidth::<_, ()>(
        &state.pending_proposals,
        store,
        PROPOSALS_AMT_BITWIDTH,
    ) {
        Ok(pending_proposals) => {
            let ret = pending_proposals.for_each(|key, _| {
                let proposal_cid = Cid::try_from(key.0.to_owned())?;

                acc.require(proposal_cids.contains(&proposal_cid), format!("pending proposal with cid {proposal_cid} not found within proposals {pending_proposals:?}"));

                pending_proposal_count += 1;
                Ok(())
            });
            acc.require_no_error(ret, "error iterating pending proposals");
        }
        Err(e) => acc.add(format!("error loading pending proposals: {e}")),
    };

    // escrow table and locked table
    let mut lock_table_count = 0;
    let escrow_table = BalanceTable::from_root(store, &state.escrow_table);
    let lock_table = BalanceTable::from_root(store, &state.locked_table);

    match (escrow_table, lock_table) {
        (Ok(escrow_table), Ok(lock_table)) => {
            let mut locked_total = TokenAmount::zero();
            let ret = lock_table.0.for_each(|key, locked_amount| {
                let address = Address::from_bytes(key)?;

                locked_total += locked_amount;

                // every entry in locked table should have a corresponding entry in escrow table that is at least as high
                let escrow_amount = &escrow_table.get(&address)?;
                acc.require(escrow_amount >= locked_amount, format!("locked funds for {address}, {locked_amount}, greater than escrow amount, {escrow_amount}"));

                lock_table_count += 1;

                Ok(())
            });
            acc.require_no_error(ret, "error iterating locked table");

            // lockTable total should be sum of client and provider locked plus client storage fee
            let expected_lock_total = &state.total_provider_locked_collateral
                + &state.total_client_locked_collateral
                + &state.total_client_storage_fee;
            acc.require(locked_total == expected_lock_total, format!("locked total, {locked_total}, does not sum to provider locked, {}, client locked, {}, and client storage fee, {}", state.total_provider_locked_collateral, state.total_client_locked_collateral, state.total_client_storage_fee));

            // assert escrow <= actor balance
            // lock_table item <= escrow item and escrow_total <= balance implies lock_table total <= balance
            match escrow_table.total() {
                Ok(escrow_total) => {
                    acc.require(
                        &escrow_total <= balance,
                        format!(
                            "escrow total, {escrow_total}, greater than actor balance, {balance}"
                        ),
                    );
                    acc.require(escrow_total >= total_proposal_collateral, format!("escrow total, {escrow_total}, less than sum of proposal collateral, {total_proposal_collateral}"));
                }
                Err(e) => acc.add(format!("error calculating escrow total: {e}")),
            }
        }
        (escrow_table, lock_table) => {
            acc.require_no_error(escrow_table, "error loading escrow table");
            acc.require_no_error(lock_table, "error loading locked table");
        }
    };

    // deals ops by epoch
    let (mut deal_op_epoch_count, mut deal_op_count) = (0, 0);
    match SetMultimap::from_root(store, &state.deal_ops_by_epoch) {
        Ok(deal_ops) => {
            // get into internals just to iterate through full data structure
            let ret = deal_ops.0.for_each(|key, _| {
                let epoch = parse_uint_key(key)? as i64;

                deal_op_epoch_count += 1;

                deal_ops.for_each(epoch, |deal_id| {
                    acc.require(proposal_stats.contains_key(&deal_id), format!("deal op found for deal id {deal_id} with missing proposal at epoch {epoch}"));
                    expected_deal_ops.remove(&deal_id);
                    deal_op_count += 1;
                    Ok(())
                }).map_err(|e| anyhow::anyhow!("error iterating deal ops for epoch {}: {}", epoch, e))
            });
            acc.require_no_error(ret, "error iterating all deal ops");
        }
        Err(e) => acc.add(format!("error loading deal ops: {e}")),
    };

    acc.require(
        expected_deal_ops.is_empty(),
        format!("missing deal ops for proposals: {expected_deal_ops:?}"),
    );

    (
        StateSummary {
            deals: proposal_stats,
            pending_proposal_count,
            deal_state_count,
            lock_table_count,
            deal_op_epoch_count,
            deal_op_count,
        },
        acc,
    )
}
