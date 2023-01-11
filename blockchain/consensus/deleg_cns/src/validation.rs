// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use forest_blocks::{Block, BlockHeader, Tipset};
use forest_db::Store;
use forest_networks::ChainConfig;
use forest_state_manager::StateManager;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::address::Address;

use crate::DelegatedConsensusError;

/// Validates block semantically according to the rules of Delegated Consensus.
/// Returns all encountered errors, so they can be merged with the common validations performed by the synchronizer.
///
/// Validation includes:
/// * Sanity checks
/// * Timestamps
/// * The block was proposed by the only only miner eligible
#[allow(clippy::unused_async)]
pub(crate) async fn validate_block<DB: Blockstore + Store + Clone + Sync + Send + 'static>(
    chosen_one: &Address,
    state_manager: Arc<StateManager<DB>>,
    block: Arc<Block>,
) -> Result<(), Box<DelegatedConsensusError>> {
    let chain_store = state_manager.chain_store().clone();
    let header = block.header();

    block_sanity_checks(header)?;

    let base_tipset = chain_store.tipset_from_keys(header.parents())?;

    block_timestamp_checks(
        header,
        base_tipset.as_ref(),
        state_manager.chain_config().as_ref(),
    )?;

    // Miner validations
    validate_miner(
        header,
        base_tipset.as_ref(),
        state_manager.as_ref(),
        chosen_one,
    )?;

    Ok(())
}

/// Checks optional values in header.
///
/// In particular it looks for an election proof and a ticket,
/// which are needed for Filecoin consensus.
fn block_sanity_checks(header: &BlockHeader) -> Result<(), Box<DelegatedConsensusError>> {
    if header.election_proof().is_some() {
        return Err(Box::new(DelegatedConsensusError::BlockWithElectionProof));
    }
    if header.ticket().is_some() {
        return Err(Box::new(DelegatedConsensusError::BlockWithTicket));
    }
    Ok(())
}

/// Check the timestamp corresponds exactly to the number of epochs since the parents.
///
/// This is the same as in the default `FilecoinConsensus`.
fn block_timestamp_checks(
    header: &BlockHeader,
    base_tipset: &Tipset,
    chain_config: &ChainConfig,
) -> Result<(), Box<DelegatedConsensusError>> {
    // Timestamp checks
    let block_delay = chain_config.block_delay_secs;
    let nulls = (header.epoch() - (base_tipset.epoch() + 1)) as u64;
    let target_timestamp = base_tipset.min_timestamp() + block_delay * (nulls + 1);
    if target_timestamp != header.timestamp() {
        return Err(Box::new(DelegatedConsensusError::UnequalBlockTimestamps(
            header.timestamp(),
            target_timestamp,
        )));
    }
    Ok(())
}

/// Check that the miner who produced the block is the one we delegated to.
fn validate_miner<DB>(
    header: &BlockHeader,
    base_tipset: &Tipset,
    state_manager: &StateManager<DB>,
    chosen_one: &Address,
) -> Result<(), Box<DelegatedConsensusError>>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
{
    use DelegatedConsensusError::*;
    let miner_addr = header.miner_address();

    // Workaround for the bug where Forest strips the network type from the Address
    // and then puts back always the mainnet variant, so the `t` prefix becomes `f`.
    let chosen_addr = state_manager
        .lookup_id(chosen_one, base_tipset)?
        .unwrap_or(*chosen_one);

    // This is where a miner address of `t01000` becomes `f01000`.
    match state_manager.lookup_id(miner_addr, base_tipset) {
        Ok(Some(id)) if id == chosen_addr => Ok(()),
        Ok(Some(id)) => Err(Box::new(MinerNotEligibleToMine(chosen_addr, id))),
        Ok(None) => Err(Box::new(UnknownMiner(*miner_addr))),
        Err(e) => Err(Box::new(e.into())),
    }
}
