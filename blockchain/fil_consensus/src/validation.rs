// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use async_std::task::{self, Context, Poll};
use log::{debug, error, info, trace, warn};

use beacon::{Beacon, BeaconEntry, BeaconSchedule, IGNORE_DRAND_VAR};
use blocks::Block;
use chain_sync::Consensus;
use cid::Cid;
use fil_types::verifier::ProofVerifier;
use ipld_blockstore::BlockStore;
use nonempty::NonEmpty;
use state_manager::StateManager;

use crate::FilecoinConsensusError;

/// Validates block semantically according to https://github.com/filecoin-project/specs/blob/6ab401c0b92efb6420c6e198ec387cf56dc86057/validation.md
/// Returns the validated block if `Ok`.
/// Returns the block cid (for marking bad) and `Error` if invalid (`Err`).
///
/// Validation includes:
/// * Sanity checks
/// * Timestamps and clock drifts
/// * Signatures
/// * Message inclusion (fees, sequences)
/// * Elections and Proof-of-SpaceTime, Beacon values
/// * Parent related fields: base fee, weight, the state root
/// * NB: This is where the messages in the *parent* tipset are executed.
pub(crate) async fn validate_block<
    DB: BlockStore + Sync + Send + 'static,
    //TBeacon: Beacon + Sync + Send + 'static,
    //V: ProofVerifier + Sync + Send + 'static,
>(
    state_manager: Arc<StateManager<DB>>,
    //beacon_schedule: Arc<BeaconSchedule<TBeacon>>,
    block: Arc<Block>,
) -> Result<(), NonEmpty<FilecoinConsensusError>> {
    trace!(
        "Validating block: epoch = {}, weight = {}, key = {}",
        block.header().epoch(),
        block.header().weight(),
        block.header().cid(),
    );
    let chain_store = state_manager.chain_store().clone();
    let block_cid = block.cid();

    /*

    // Check block validation cache in store
    let is_validated = chain_store
        .is_block_validated(block_cid)
        .map_err(|why| (*block_cid, why.into()))?;
    if is_validated {
        return Ok(block);
    }
    let mut error_vec: Vec<String> = vec![];
    let mut validations = FuturesUnordered::new();
    let header = block.header();

    // Check to ensure all optional values exist
    block_sanity_checks(header).map_err(|e| (*block_cid, e))?;

    let base_tipset = chain_store
        .tipset_from_keys(header.parents())
        .await
        // The parent tipset will always be there when calling validate_block
        // as part of the sync_tipset_range flow because all of the headers in the range
        // have been committed to the store. When validate_block is called from sync_tipset
        // this guarantee does not exist, so we create a specific error to inform the caller
        // not to add this block to the bad blocks cache.
        .map_err(|why| {
            (
                *block_cid,
                TipsetRangeSyncerError::TipsetParentNotFound(why),
            )
        })?;
    let win_p_nv = state_manager.get_network_version(base_tipset.epoch());

    // Retrieve lookback tipset for validation
    let (lookback_tipset, lookback_state) = state_manager
        .get_lookback_tipset_for_round::<V>(base_tipset.clone(), block.header().epoch())
        .await
        .map_err(|e| (*block_cid, e.into()))?;
    let lookback_state = Arc::new(lookback_state);
    let prev_beacon = chain_store
        .latest_beacon_entry(&base_tipset)
        .await
        .map(Arc::new)
        .map_err(|e| (*block_cid, e.into()))?;

    // Timestamp checks
    let block_delay = state_manager.chain_config.block_delay_secs;
    let smoke_height = state_manager.chain_config.epoch(Height::Smoke);
    let nulls = (header.epoch() - (base_tipset.epoch() + 1)) as u64;
    let target_timestamp = base_tipset.min_timestamp() + block_delay * (nulls + 1);
    if target_timestamp != header.timestamp() {
        return Err((
            *block_cid,
            TipsetRangeSyncerError::UnequalBlockTimestamps(header.timestamp(), target_timestamp),
        ));
    }
    let time_now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Retrieved system time before UNIX epoch")
        .as_secs();
    if header.timestamp() > time_now + ALLOWABLE_CLOCK_DRIFT {
        return Err((
            *block_cid,
            TipsetRangeSyncerError::TimeTravellingBlock(time_now, header.timestamp()),
        ));
    } else if header.timestamp() > time_now {
        warn!(
            "Got block from the future, but within clock drift threshold, {} > {}",
            header.timestamp(),
            time_now
        );
    }

    // Work address needed for async validations, so necessary
    // to do sync to avoid duplication
    let work_addr = state_manager
        .get_miner_work_addr(*lookback_state, header.miner_address())
        .map_err(|e| (*block_cid, e.into()))?;

    // Async validations

    // Check block messages
    let v_block = Arc::clone(&block);
    let v_base_tipset = Arc::clone(&base_tipset);
    let v_state_manager = Arc::clone(&state_manager);
    validations.push(task::spawn_blocking(move || {
        check_block_messages::<_, V, C>(v_state_manager, &v_block, &v_base_tipset)
            .map_err(|e| TipsetRangeSyncerError::Validation(e.to_string()))
    }));

    // Miner validations
    let v_state_manager = Arc::clone(&state_manager);
    let v_block = Arc::clone(&block);
    let v_base_tipset = Arc::clone(&base_tipset);
    validations.push(task::spawn_blocking(move || {
        let headers = v_block.header();
        validate_miner(
            &v_state_manager,
            headers.miner_address(),
            v_base_tipset.parent_state(),
        )
    }));

    // Base fee check
    let v_base_tipset = Arc::clone(&base_tipset);
    let v_block_store = state_manager.blockstore_cloned();
    let v_block = Arc::clone(&block);
    validations.push(task::spawn_blocking(move || {
        let base_fee =
            chain::compute_base_fee(v_block_store.as_ref(), &v_base_tipset, smoke_height).map_err(
                |e| {
                    TipsetRangeSyncerError::Validation(format!("Could not compute base fee: {}", e))
                },
            )?;
        let parent_base_fee = v_block.header.parent_base_fee();
        if &base_fee != parent_base_fee {
            return Err(TipsetRangeSyncerError::Validation(format!(
                "base fee doesn't match: {} (header), {} (computed)",
                parent_base_fee, base_fee
            )));
        }
        Ok(())
    }));

    // Parent weight calculation check
    let v_block_store = state_manager.blockstore_cloned();
    let v_base_tipset = Arc::clone(&base_tipset);
    let weight = header.weight().clone();
    validations.push(task::spawn_blocking(move || {
        let calc_weight = chain::weight(v_block_store.as_ref(), &v_base_tipset).map_err(|e| {
            TipsetRangeSyncerError::Calculation(format!("Error calculating weight: {}", e))
        })?;
        if weight != calc_weight {
            return Err(TipsetRangeSyncerError::Validation(format!(
                "Parent weight doesn't match: {} (header), {} (computed)",
                weight, calc_weight
            )));
        }
        Ok(())
    }));

    // State root and receipt root validations
    let v_state_manager = Arc::clone(&state_manager);
    let v_base_tipset = Arc::clone(&base_tipset);
    let v_block = Arc::clone(&block);
    validations.push(task::spawn(async move {
        let header = v_block.header();
        let (state_root, receipt_root) = v_state_manager
            .tipset_state::<V>(&v_base_tipset)
            .await
            .map_err(|e| {
                TipsetRangeSyncerError::Calculation(format!("Failed to calculate state: {}", e))
            })?;
        if &state_root != header.state_root() {
            #[cfg(feature = "statediff")]
            {
                if let Err(err) = statediff::print_state_diff(
                    v_state_manager.blockstore(),
                    &state_root,
                    header.state_root(),
                    Some(1),
                ) {
                    eprintln!("Failed to print state-diff: {}", err);
                }
            }
            return Err(TipsetRangeSyncerError::Validation(format!(
                "Parent state root did not match computed state: {} (header), {} (computed)",
                header.state_root(),
                state_root,
            )));
        }
        if &receipt_root != header.message_receipts() {
            return Err(TipsetRangeSyncerError::Validation(format!(
                "Parent receipt root did not match computed root: {} (header), {} (computed)",
                header.message_receipts(),
                receipt_root
            )));
        }
        Ok(())
    }));

    // Winner election PoSt validations
    let v_block = Arc::clone(&block);
    let v_prev_beacon = Arc::clone(&prev_beacon);
    let v_base_tipset = Arc::clone(&base_tipset);
    let v_state_manager = Arc::clone(&state_manager);
    let v_lookback_state = lookback_state.clone();
    validations.push(task::spawn_blocking(move || {
        let header = v_block.header();

        // Safe to unwrap because checked to `Some` in sanity check
        let election_proof = header.election_proof().as_ref().unwrap();
        if election_proof.win_count < 1 {
            return Err(TipsetRangeSyncerError::Validation(
                "Block is not claiming to be a winner".to_string(),
            ));
        }
        let hp = v_state_manager.eligible_to_mine(
            header.miner_address(),
            &v_base_tipset,
            &lookback_tipset,
        )?;
        if !hp {
            return Err(TipsetRangeSyncerError::MinerNotEligibleToMine);
        }
        let r_beacon = header.beacon_entries().last().unwrap_or(&v_prev_beacon);
        let miner_address_buf = header.miner_address().marshal_cbor()?;
        let vrf_base = state_manager::chain_rand::draw_randomness(
            r_beacon.data(),
            DomainSeparationTag::ElectionProofProduction as i64,
            header.epoch(),
            &miner_address_buf,
        )
        .map_err(|e| TipsetRangeSyncerError::DrawingChainRandomness(e.to_string()))?;
        verify_election_post_vrf(&work_addr, &vrf_base, election_proof.vrfproof.as_bytes())?;

        if v_state_manager.is_miner_slashed(header.miner_address(), v_base_tipset.parent_state())? {
            return Err(TipsetRangeSyncerError::InvalidOrSlashedMiner);
        }
        let (mpow, tpow) = v_state_manager
            .get_power(&v_lookback_state, Some(header.miner_address()))?
            .ok_or(TipsetRangeSyncerError::MinerPowerNotAvailable)?;

        let j = election_proof.compute_win_count(&mpow.quality_adj_power, &tpow.quality_adj_power);
        if election_proof.win_count != j {
            return Err(TipsetRangeSyncerError::MinerWinClaimsIncorrect(
                election_proof.win_count,
                j,
            ));
        }

        Ok(())
    }));

    // Block signature check
    let v_block = Arc::clone(&block);
    validations.push(task::spawn_blocking(move || {
        v_block.header().check_block_signature(&work_addr)?;
        Ok(())
    }));

    // Beacon values check
    if std::env::var(IGNORE_DRAND_VAR) != Ok("1".to_owned()) {
        let v_block = Arc::clone(&block);
        let parent_epoch = base_tipset.epoch();
        let v_prev_beacon = Arc::clone(&prev_beacon);
        validations.push(task::spawn(async move {
            v_block
                .header()
                .validate_block_drand(beacon_schedule.as_ref(), parent_epoch, &v_prev_beacon)
                .await
                .map_err(|e| {
                    TipsetRangeSyncerError::Validation(format!(
                        "Failed to validate blocks random beacon values: {}",
                        e
                    ))
                })
        }));
    }

    // Ticket election proof validations
    let v_block = Arc::clone(&block);
    let v_prev_beacon = Arc::clone(&prev_beacon);
    validations.push(task::spawn_blocking(move || {
        let header = v_block.header();
        let mut miner_address_buf = header.miner_address().marshal_cbor()?;

        if header.epoch() > smoke_height {
            let vrf_proof = base_tipset
                .min_ticket()
                .ok_or(TipsetRangeSyncerError::TipsetWithoutTicket)?
                .vrfproof
                .as_bytes();
            miner_address_buf.extend_from_slice(vrf_proof);
        }

        let beacon_base = header.beacon_entries().last().unwrap_or(&v_prev_beacon);

        let vrf_base = state_manager::chain_rand::draw_randomness(
            beacon_base.data(),
            DomainSeparationTag::TicketProduction as i64,
            header.epoch() - TICKET_RANDOMNESS_LOOKBACK,
            &miner_address_buf,
        )
        .map_err(|e| TipsetRangeSyncerError::DrawingChainRandomness(e.to_string()))?;

        verify_election_post_vrf(
            &work_addr,
            &vrf_base,
            // Safe to unwrap here because of block sanity checks
            header.ticket().as_ref().unwrap().vrfproof.as_bytes(),
        )?;

        Ok(())
    }));

    // Winning PoSt proof validation
    let v_block = block.clone();
    let v_prev_beacon = Arc::clone(&prev_beacon);
    validations.push(task::spawn_blocking(move || {
        verify_winning_post_proof::<_, V, C>(
            &state_manager,
            win_p_nv,
            v_block.header(),
            &v_prev_beacon,
            &lookback_state,
        )?;
        Ok(())
    }));

    // Collect the errors from the async validations
    while let Some(result) = validations.next().await {
        if let Err(e) = result {
            error_vec.push(e.to_string());
        }
    }

    // Combine the vector of error strings and return Validation error with this resultant string
    if !error_vec.is_empty() {
        let error_string = error_vec.join(", ");
        return Err((*block_cid, TipsetRangeSyncerError::Validation(error_string)));
    }

    chain_store
        .mark_block_as_validated(block_cid)
        .map_err(|e| {
            (
                *block_cid,
                TipsetRangeSyncerError::Validation(format!(
                    "failed to mark block {} as validated {}",
                    block_cid, e
                )),
            )
        })?;

    */

    Ok(())
}
