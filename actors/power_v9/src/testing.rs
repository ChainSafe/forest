use std::collections::HashMap;

use fil_actors_runtime_v9::{parse_uint_key, runtime::Policy, Map, MessageAccumulator, Multimap};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::RawBytes;
use fvm_shared::{
    address::Address,
    clock::ChainEpoch,
    sector::{SealVerifyInfo, StoragePower},
    HAMT_BIT_WIDTH,
};
use num_traits::{Signed, Zero};

use crate::{
    consensus_miner_min_power, Claim, CronEvent, State, CRON_QUEUE_AMT_BITWIDTH,
    CRON_QUEUE_HAMT_BITWIDTH, MAX_MINER_PROVE_COMMITS_PER_EPOCH,
    PROOF_VALIDATION_BATCH_AMT_BITWIDTH,
};

pub struct MinerCronEvent {
    pub epoch: ChainEpoch,
    pub payload: RawBytes,
}

type CronEventsByAddress = HashMap<Address, Vec<MinerCronEvent>>;
type ClaimsByAddress = HashMap<Address, Claim>;
type ProofsByAddress = HashMap<Address, SealVerifyInfo>;

pub struct StateSummary {
    pub crons: CronEventsByAddress,
    pub claims: ClaimsByAddress,
    pub proofs: ProofsByAddress,
}

/// Checks internal invariants of power state
pub fn check_state_invariants<BS: Blockstore>(
    policy: &Policy,
    state: &State,
    store: &BS,
) -> (StateSummary, MessageAccumulator) {
    let acc = MessageAccumulator::default();

    // basic invariants around recorded power
    acc.require(
        !state.total_raw_byte_power.is_negative(),
        format!("total raw power is negative {}", state.total_raw_byte_power),
    );
    acc.require(
        !state.total_quality_adj_power.is_negative(),
        format!(
            "total qa power is negative {}",
            state.total_quality_adj_power
        ),
    );
    acc.require(
        !state.total_bytes_committed.is_negative(),
        format!(
            "total raw power committed is negative {}",
            state.total_bytes_committed
        ),
    );
    acc.require(
        !state.total_qa_bytes_committed.is_negative(),
        format!(
            "total qa power committed is negative {}",
            state.total_qa_bytes_committed
        ),
    );

    acc.require(
        state.total_raw_byte_power <= state.total_quality_adj_power,
        format!(
            "total raw power {} is greater than total quality adjusted power {}",
            state.total_raw_byte_power, state.total_quality_adj_power
        ),
    );
    acc.require(
        state.total_bytes_committed <= state.total_qa_bytes_committed,
        format!(
            "committed raw power {} is greater than committed quality adjusted power {}",
            state.total_bytes_committed, state.total_qa_bytes_committed
        ),
    );
    acc.require(
        state.total_raw_byte_power <= state.total_bytes_committed,
        format!(
            "total raw power {} is greater than raw power committed {}",
            state.total_raw_byte_power, state.total_bytes_committed
        ),
    );
    acc.require(
        state.total_quality_adj_power <= state.total_qa_bytes_committed,
        format!(
            "total qa power {} is greater than qa power committed {}",
            state.total_quality_adj_power, state.total_qa_bytes_committed
        ),
    );

    let crons = check_cron_invariants(state, store, &acc);
    let claims = check_claims_invariants(policy, state, store, &acc);
    let proofs = check_proofs_invariants(state, store, &claims, &acc);

    (
        StateSummary {
            crons,
            claims,
            proofs,
        },
        acc,
    )
}

fn check_cron_invariants<BS: Blockstore>(
    state: &State,
    store: &BS,
    acc: &MessageAccumulator,
) -> CronEventsByAddress {
    let mut cron_events_by_address = CronEventsByAddress::new();

    match Multimap::from_root(
        store,
        &state.cron_event_queue,
        CRON_QUEUE_HAMT_BITWIDTH,
        CRON_QUEUE_AMT_BITWIDTH,
    ) {
        Ok(queue) => {
            let ret = queue.for_all::<_, CronEvent>(|key, events| {
                let epoch = match parse_uint_key(key) {
                    Ok(key) => key,
                    Err(e) => {
                        acc.add(format!("non-int key in cron array: {e}"));
                        return Ok(());
                    }
                } as i64;

                acc.require(
                    epoch >= state.first_cron_epoch,
                    format!(
                        "cron event at epoch {epoch} before first_cron_epoch {}",
                        state.first_cron_epoch
                    ),
                );
                events
                    .for_each(|_, event| {
                        cron_events_by_address
                            .entry(event.miner_addr)
                            .or_default()
                            .push(MinerCronEvent {
                                epoch,
                                payload: event.callback_payload.clone(),
                            });
                        Ok(())
                    })
                    .map_err(|e| {
                        anyhow::anyhow!("error iterating cron events for epoch {}: {}", epoch, e)
                    })
            });
            acc.require_no_error(ret, "error iterating cron tasks");
        }
        Err(e) => acc.add(format!("error loading cron event queue: {e}")),
    }

    cron_events_by_address
}
fn check_claims_invariants<BS: Blockstore>(
    policy: &Policy,
    state: &State,
    store: &BS,
    acc: &MessageAccumulator,
) -> ClaimsByAddress {
    let mut claims_by_address = ClaimsByAddress::new();

    let mut committed_raw_power = StoragePower::zero();
    let mut committed_qa_power = StoragePower::zero();
    let mut raw_power = StoragePower::zero();
    let mut qa_power = StoragePower::zero();
    let mut claims_with_sufficient_power_count = 0;

    match Map::<_, Claim>::load(&state.claims, store) {
        Ok(claims) => {
            let ret = claims.for_each(|key, claim| {
                let address = Address::from_bytes(key)?;
                claims_by_address.insert(address, claim.clone());

                committed_raw_power += &claim.raw_byte_power;
                committed_qa_power += &claim.quality_adj_power;

                let min_power =
                    match consensus_miner_min_power(policy, claim.window_post_proof_type) {
                        Ok(power) => power,
                        Err(e) => {
                            acc.add(format!(
                                "could not get consensus miner min power for miner {address}: {e}"
                            ));
                            // noted above
                            return Ok(());
                        }
                    };

                if claim.raw_byte_power >= min_power {
                    claims_with_sufficient_power_count += 1;
                    raw_power += &claim.raw_byte_power;
                    qa_power += &claim.quality_adj_power;
                }

                Ok(())
            });
            acc.require_no_error(ret, "error iterating power claims");
        }
        Err(e) => acc.add(format!("error loading power claims: {e}")),
    };

    acc.require(committed_raw_power == state.total_bytes_committed, format!("sum of raw power in claims {committed_raw_power} does not match recorded bytes committed {}", state.total_bytes_committed));
    acc.require(committed_qa_power == state.total_qa_bytes_committed, format!("sum of qa power in claims {committed_qa_power} does not match recorded qa power committed {}", state.total_qa_bytes_committed));
    acc.require(claims_with_sufficient_power_count == state.miner_above_min_power_count, format!("claims with sufficient power {claims_with_sufficient_power_count} does not match miner_above_min_power_count {}", state.miner_above_min_power_count));
    acc.require(
        state.total_raw_byte_power == raw_power,
        format!(
            "recorded raw power {} does not match raw power in claims {raw_power}",
            state.total_raw_byte_power
        ),
    );
    acc.require(
        state.total_quality_adj_power == qa_power,
        format!(
            "recorded qa power {} does not match qa power in claims {qa_power}",
            state.total_quality_adj_power
        ),
    );

    claims_by_address
}
fn check_proofs_invariants<BS: Blockstore>(
    state: &State,
    store: &BS,
    claims: &ClaimsByAddress,
    acc: &MessageAccumulator,
) -> ProofsByAddress {
    if state.proof_validation_batch.is_none() {
        return ProofsByAddress::default();
    }

    let mut proofs_by_address = ProofsByAddress::new();
    match Multimap::from_root(
        store,
        &state.proof_validation_batch.unwrap(),
        HAMT_BIT_WIDTH,
        PROOF_VALIDATION_BATCH_AMT_BITWIDTH,
    ) {
        Ok(queue) => {
            let ret = queue.for_all::<_, SealVerifyInfo>(|key, infos| {
                let address = Address::from_bytes(key)?;
                let claim = if let Some(claim) = claims.get(&address) {
                    claim
                } else {
                    acc.add(format!("miner {address} has proofs awaiting validation but no claim"));
                    return Ok(())
                };

                let ret = infos.for_each(|_, info| {
                    match info.registered_proof.registered_window_post_proof() {
                        Ok(sector_window_post_proof_type) => {
                            acc.require(claim.window_post_proof_type == sector_window_post_proof_type, format!("miner submitted proof with proof type {:?} different from claim {:?}", sector_window_post_proof_type, claim.window_post_proof_type));
                        },
                        Err(e) => acc.add(format!("Invalid PoSt proof: {e}"))
                    }
                    proofs_by_address.insert(address, info.clone());
                    Ok(())
                });

                if ret.is_err() {
                    return ret.map_err(|e| anyhow::anyhow!("error iterating proof validation batch for address {}: {}", address, e));
                }

                acc.require(proofs_by_address.len() as u64 <= MAX_MINER_PROVE_COMMITS_PER_EPOCH, format!("miner {address} has submitted too many proofs ({}) for batch verification", proofs_by_address.len()));
                Ok(())
            });
            acc.require_no_error(ret, "error iterating proof validation queue");
        }
        Err(e) => acc.add(format!("error loading proof validation queue: {e}")),
    }

    proofs_by_address
}
