// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::networks::Height;
use crate::shim::clock::ALLOWABLE_CLOCK_DRIFT;
use crate::shim::crypto::SignatureType;
use crate::shim::{
    address::Address, crypto::verify_bls_aggregate, econ::BLOCK_GAS_LIMIT,
    gas::price_list_by_network_version, message::Message, state_tree::StateTree,
};
use crate::state_manager::{is_valid_for_sending, Error as StateManagerError, StateManager};
use crate::{
    blocks::{Block, CachingBlockHeader, Error as ForestBlockError, FullTipset, Tipset},
    fil_cns::{self, FilecoinConsensus, FilecoinConsensusError},
};
use crate::{
    chain::{ChainStore, Error as ChainStoreError},
    metrics::HistogramTimerExt,
};
use crate::{
    eth::is_valid_eth_tx_for_sending,
    message::{valid_for_block_inclusion, Message as MessageTrait},
};
use ahash::HashMap;
use cid::Cid;
use futures::{stream::FuturesUnordered, StreamExt, TryFutureExt};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::to_vec;
use itertools::Itertools;
use nunny::Vec as NonEmpty;
use thiserror::Error;
use tracing::{error, trace, warn};

use crate::chain_sync::{consensus::collect_errs, metrics, validation::TipsetValidator};

#[derive(Debug, Error)]
pub enum TipsetProcessorError {
    #[error("TipsetRangeSyncer error: {0}")]
    RangeSyncer(#[from] TipsetRangeSyncerError),
}

#[derive(Debug, Error)]
pub enum TipsetRangeSyncerError {
    #[error("Block must have a signature")]
    BlockWithoutSignature,
    #[error("Block without BLS aggregate signature")]
    BlockWithoutBlsAggregate,
    #[error("Block received from the future: now = {0}, block = {1}")]
    TimeTravellingBlock(u64, u64),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Processing error: {0}")]
    Calculation(String),
    #[error("Chain store error: {0}")]
    ChainStore(#[from] ChainStoreError),
    #[error("StateManager error: {0}")]
    StateManager(#[from] StateManagerError),
    #[error("Block error: {0}")]
    BlockError(#[from] ForestBlockError),
    #[error("Querying tipsets from the network failed: {0}")]
    NetworkTipsetQueryFailed(String),
    #[error("BLS aggregate signature {0} was invalid for msgs {1}")]
    BlsAggregateSignatureInvalid(String, String),
    #[error("Message signature invalid: {0}")]
    MessageSignatureInvalid(String),
    #[error("Block message root does not match: expected {0}, computed {1}")]
    BlockMessageRootInvalid(String, String),
    #[error("Computing message root failed: {0}")]
    ComputingMessageRoot(String),
    #[error("Resolving address from message failed: {0}")]
    ResolvingAddressFromMessage(String),
    #[error("Loading tipset parent from the store failed: {0}")]
    TipsetParentNotFound(ChainStoreError),
    #[error("Consensus error: {0}")]
    ConsensusError(FilecoinConsensusError),
}

impl<T> From<flume::SendError<T>> for TipsetRangeSyncerError {
    fn from(err: flume::SendError<T>) -> Self {
        TipsetRangeSyncerError::NetworkTipsetQueryFailed(format!("{err}"))
    }
}

impl From<tokio::task::JoinError> for TipsetRangeSyncerError {
    fn from(err: tokio::task::JoinError) -> Self {
        TipsetRangeSyncerError::NetworkTipsetQueryFailed(format!("{err}"))
    }
}

impl TipsetRangeSyncerError {
    /// Concatenate all validation error messages into one comma separated
    /// version.
    fn concat(errs: NonEmpty<TipsetRangeSyncerError>) -> Self {
        let msg = errs
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join(", ");

        TipsetRangeSyncerError::Validation(msg)
    }
}

/// Validates full blocks in the tipset in parallel (since the messages are not
/// executed), adding the successful ones to the tipset tracker, and the failed
/// ones to the bad block cache, depending on strategy. Any bad block fails
/// validation.
pub async fn validate_tipset<DB: Blockstore + Send + Sync + 'static>(
    state_manager: Arc<StateManager<DB>>,
    chainstore: &ChainStore<DB>,
    full_tipset: FullTipset,
    genesis: &Tipset,
) -> Result<(), TipsetRangeSyncerError> {
    if full_tipset.key().eq(genesis.key()) {
        trace!("Skipping genesis tipset validation");
        return Ok(());
    }

    let timer = metrics::TIPSET_PROCESSING_TIME.start_timer();

    let epoch = full_tipset.epoch();
    let full_tipset_key = full_tipset.key().clone();

    let mut validations = FuturesUnordered::new();
    let blocks = full_tipset.into_blocks();

    trace!("Tipset keys: {full_tipset_key}");

    for b in blocks {
        let validation_fn = tokio::task::spawn(validate_block(state_manager.clone(), Arc::new(b)));
        validations.push(validation_fn);
    }

    while let Some(result) = validations.next().await {
        match result? {
            Ok(block) => {
                chainstore.add_to_tipset_tracker(block.header());
            }
            Err((cid, why)) => {
                warn!(
                    "Validating block [CID = {}] in EPOCH = {} failed: {}",
                    cid.clone(),
                    epoch,
                    why
                );
                return Err(why);
            }
        }
    }
    drop(timer);
    Ok(())
}

/// Validate the block according to the rules specific to the consensus being
/// used, and the common rules that pertain to the assumptions of the
/// `ChainSync` protocol.
///
/// Returns the validated block if `Ok`.
/// Returns the block CID (for marking bad) and `Error` if invalid (`Err`).
///
/// Common validation includes:
/// * Sanity checks
/// * Clock drifts
/// * Signatures
/// * Message inclusion (fees, sequences)
/// * Parent related fields: base fee, weight, the state root
/// * NB: This is where the messages in the *parent* tipset are executed.
///
/// Consensus specific validation should include:
/// * Checking that the messages in the block correspond to the agreed upon
///   total ordering
/// * That the block is a deterministic derivative of the underlying consensus
async fn validate_block<DB: Blockstore + Sync + Send + 'static>(
    state_manager: Arc<StateManager<DB>>,
    block: Arc<Block>,
) -> Result<Arc<Block>, (Cid, TipsetRangeSyncerError)> {
    let consensus = FilecoinConsensus::new(state_manager.beacon_schedule().clone());
    trace!(
        "Validating block: epoch = {}, weight = {}, key = {}",
        block.header().epoch,
        block.header().weight,
        block.header().cid(),
    );
    let chain_store = state_manager.chain_store().clone();
    let block_cid = block.cid();

    // Check block validation cache in store
    let is_validated = chain_store.is_block_validated(block_cid);
    if is_validated {
        return Ok(block);
    }

    let _timer = metrics::BLOCK_VALIDATION_TIME.start_timer();

    let header = block.header();

    // Check to ensure all optional values exist
    block_sanity_checks(header).map_err(|e| (*block_cid, e))?;
    block_timestamp_checks(header).map_err(|e| (*block_cid, e))?;

    let base_tipset = chain_store
        .chain_index
        .load_required_tipset(&header.parents)
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

    // Retrieve lookback tipset for validation
    let lookback_state = ChainStore::get_lookback_tipset_for_round(
        state_manager.chain_store().chain_index.clone(),
        state_manager.chain_config().clone(),
        base_tipset.clone(),
        block.header().epoch,
    )
    .map_err(|e| (*block_cid, e.into()))
    .map(|(_, s)| Arc::new(s))?;

    // Work address needed for async validations, so necessary
    // to do sync to avoid duplication
    let work_addr = state_manager
        .get_miner_work_addr(*lookback_state, &header.miner_address)
        .map_err(|e| (*block_cid, e.into()))?;

    // Async validations
    let validations = FuturesUnordered::new();

    // Check block messages
    validations.push(tokio::task::spawn(check_block_messages(
        Arc::clone(&state_manager),
        Arc::clone(&block),
        Arc::clone(&base_tipset),
    )));

    // Base fee check
    let smoke_height = state_manager.chain_config().epoch(Height::Smoke);
    let v_base_tipset = Arc::clone(&base_tipset);
    let v_block_store = state_manager.blockstore_owned();
    let v_block = Arc::clone(&block);
    validations.push(tokio::task::spawn_blocking(move || {
        let metric =
            &*metrics::BLOCK_VALIDATION_TASKS_TIME.get_or_create(&metrics::values::BASE_FEE_CHECK);
        let _timer = metric.start_timer();
        let base_fee = crate::chain::compute_base_fee(&v_block_store, &v_base_tipset, smoke_height)
            .map_err(|e| {
                TipsetRangeSyncerError::Validation(format!("Could not compute base fee: {e}"))
            })?;
        let parent_base_fee = &v_block.header.parent_base_fee;
        if &base_fee != parent_base_fee {
            return Err(TipsetRangeSyncerError::Validation(format!(
                "base fee doesn't match: {parent_base_fee} (header), {base_fee} (computed)"
            )));
        }
        Ok(())
    }));

    // Parent weight calculation check
    let v_block_store = state_manager.blockstore_owned();
    let v_base_tipset = Arc::clone(&base_tipset);
    let weight = header.weight.clone();
    validations.push(tokio::task::spawn_blocking(move || {
        let metric = &*metrics::BLOCK_VALIDATION_TASKS_TIME
            .get_or_create(&metrics::values::PARENT_WEIGHT_CAL);
        let _timer = metric.start_timer();
        let calc_weight = fil_cns::weight(&v_block_store, &v_base_tipset).map_err(|e| {
            TipsetRangeSyncerError::Calculation(format!("Error calculating weight: {e}"))
        })?;
        if weight != calc_weight {
            return Err(TipsetRangeSyncerError::Validation(format!(
                "Parent weight doesn't match: {weight} (header), {calc_weight} (computed)"
            )));
        }
        Ok(())
    }));

    // State root and receipt root validations
    let v_state_manager = Arc::clone(&state_manager);
    let v_base_tipset = Arc::clone(&base_tipset);
    let v_block = Arc::clone(&block);
    validations.push(tokio::task::spawn(async move {
        let header = v_block.header();
        let (state_root, receipt_root) = v_state_manager
            .tipset_state(&v_base_tipset)
            .await
            .map_err(|e| {
                TipsetRangeSyncerError::Calculation(format!("Failed to calculate state: {e}"))
            })?;

        if state_root != header.state_root {
            return Err(TipsetRangeSyncerError::Validation(format!(
                "Parent state root did not match computed state: {} (header), {} (computed)",
                header.state_root, state_root,
            )));
        }

        if receipt_root != header.message_receipts {
            return Err(TipsetRangeSyncerError::Validation(format!(
                "Parent receipt root did not match computed root: {} (header), {} (computed)",
                header.message_receipts, receipt_root
            )));
        }
        Ok(())
    }));

    // Block signature check
    let v_block = block.clone();
    validations.push(tokio::task::spawn_blocking(move || {
        let metric = &*metrics::BLOCK_VALIDATION_TASKS_TIME
            .get_or_create(&metrics::values::BLOCK_SIGNATURE_CHECK);
        let _timer = metric.start_timer();
        v_block.header().verify_signature_against(&work_addr)?;
        Ok(())
    }));

    let v_block = block.clone();
    validations.push(tokio::task::spawn(async move {
        consensus
            .validate_block(state_manager, v_block)
            .map_err(|errs| {
                // NOTE: Concatenating errors here means the wrapper type of error
                // never surfaces, yet we always pay the cost of the generic argument.
                // But there's no reason `validate_block` couldn't return a list of all
                // errors instead of a single one that has all the error messages,
                // removing the caller's ability to distinguish between them.

                TipsetRangeSyncerError::concat(
                    errs.into_iter_ne()
                        .map(TipsetRangeSyncerError::ConsensusError)
                        .collect_vec(),
                )
            })
            .await
    }));

    // Collect the errors from the async validations
    if let Err(errs) = collect_errs(validations).await {
        return Err((*block_cid, TipsetRangeSyncerError::concat(errs)));
    }

    chain_store.mark_block_as_validated(block_cid);

    Ok(block)
}

/// Validate messages in a full block, relative to the parent tipset.
///
/// This includes:
/// * signature checks
/// * gas limits, and prices
/// * account nonce values
/// * the message root in the header
///
/// NB: This loads/computes the state resulting from the execution of the parent
/// tipset.
async fn check_block_messages<DB: Blockstore + Send + Sync + 'static>(
    state_manager: Arc<StateManager<DB>>,
    block: Arc<Block>,
    base_tipset: Arc<Tipset>,
) -> Result<(), TipsetRangeSyncerError> {
    let network_version = state_manager
        .chain_config()
        .network_version(block.header.epoch);
    let eth_chain_id = state_manager.chain_config().eth_chain_id;

    if let Some(sig) = &block.header().bls_aggregate {
        // Do the initial loop here
        // check block message and signatures in them
        let mut pub_keys = Vec::with_capacity(block.bls_msgs().len());
        let mut cids = Vec::with_capacity(block.bls_msgs().len());
        let db = state_manager.blockstore_owned();
        for m in block.bls_msgs() {
            let pk = StateManager::get_bls_public_key(&db, &m.from, *base_tipset.parent_state())?;
            pub_keys.push(pk);
            cids.push(m.cid().to_bytes());
        }

        if !verify_bls_aggregate(
            &cids.iter().map(|x| x.as_slice()).collect_vec(),
            &pub_keys,
            sig,
        ) {
            return Err(TipsetRangeSyncerError::BlsAggregateSignatureInvalid(
                format!("{sig:?}"),
                format!("{cids:?}"),
            ));
        }
    } else {
        return Err(TipsetRangeSyncerError::BlockWithoutBlsAggregate);
    }

    let price_list = price_list_by_network_version(network_version);
    let mut sum_gas_limit = 0;

    // Check messages for validity
    let mut check_msg = |msg: &Message,
                         account_sequences: &mut HashMap<Address, u64>,
                         tree: &StateTree<DB>|
     -> Result<(), anyhow::Error> {
        // Phase 1: Syntactic validation
        let min_gas = price_list.on_chain_message(to_vec(msg).unwrap().len());
        valid_for_block_inclusion(msg, min_gas.total(), network_version)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        sum_gas_limit += msg.gas_limit;
        if sum_gas_limit > BLOCK_GAS_LIMIT {
            anyhow::bail!("block gas limit exceeded");
        }

        // Phase 2: (Partial) Semantic validation
        // Send exists and is an account actor, and sequence is correct
        let sequence: u64 = match account_sequences.get(&msg.from()) {
            Some(sequence) => *sequence,
            None => {
                let actor = tree.get_actor(&msg.from)?.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Failed to retrieve nonce for addr: Actor does not exist in state"
                    )
                })?;
                let network_version = state_manager
                    .chain_config()
                    .network_version(block.header.epoch);
                if !is_valid_for_sending(network_version, &actor) {
                    anyhow::bail!("not valid for sending!");
                }
                actor.sequence
            }
        };

        // Sequence equality check
        if sequence != msg.sequence {
            anyhow::bail!(
                "Message has incorrect sequence (exp: {} got: {})",
                sequence,
                msg.sequence
            );
        }
        account_sequences.insert(msg.from(), sequence + 1);
        Ok(())
    };

    let mut account_sequences: HashMap<Address, u64> = HashMap::default();
    let (state_root, _) = state_manager
        .tipset_state(&base_tipset)
        .await
        .map_err(|e| TipsetRangeSyncerError::Calculation(format!("Could not update state: {e}")))?;
    let tree =
        StateTree::new_from_root(state_manager.blockstore_owned(), &state_root).map_err(|e| {
            TipsetRangeSyncerError::Calculation(format!(
                "Could not load from new state root in state manager: {e}"
            ))
        })?;

    // Check validity for BLS messages
    for (i, msg) in block.bls_msgs().iter().enumerate() {
        check_msg(msg, &mut account_sequences, &tree).map_err(|e| {
            TipsetRangeSyncerError::Validation(format!(
                "Block had invalid BLS message at index {i}: {e}"
            ))
        })?;
    }

    // Check validity for SECP messages
    for (i, msg) in block.secp_msgs().iter().enumerate() {
        if msg.signature().signature_type() == SignatureType::Delegated
            && !is_valid_eth_tx_for_sending(eth_chain_id, network_version, msg)
        {
            return Err(TipsetRangeSyncerError::Validation(
                "Network version must be at least NV23 for legacy Ethereum transactions".to_owned(),
            ));
        }
        check_msg(msg.message(), &mut account_sequences, &tree).map_err(|e| {
            TipsetRangeSyncerError::Validation(format!(
                "block had an invalid secp message at index {i}: {e}"
            ))
        })?;
        // Resolve key address for signature verification
        let key_addr = state_manager
            .resolve_to_key_addr(&msg.from(), &base_tipset)
            .await
            .map_err(|e| TipsetRangeSyncerError::ResolvingAddressFromMessage(e.to_string()))?;
        // SecP256K1 Signature validation
        msg.signature
            .authenticate_msg(eth_chain_id, msg, &key_addr)
            .map_err(|e| TipsetRangeSyncerError::MessageSignatureInvalid(e.to_string()))?;
    }

    // Validate message root from header matches message root
    let msg_root = TipsetValidator::compute_msg_root(
        state_manager.blockstore(),
        block.bls_msgs(),
        block.secp_msgs(),
    )
    .map_err(|err| TipsetRangeSyncerError::ComputingMessageRoot(err.to_string()))?;
    if block.header().messages != msg_root {
        return Err(TipsetRangeSyncerError::BlockMessageRootInvalid(
            format!("{:?}", block.header().messages),
            format!("{msg_root:?}"),
        ));
    }

    Ok(())
}

/// Checks optional values in header.
///
/// It only looks for fields which are common to all consensus types.
fn block_sanity_checks(header: &CachingBlockHeader) -> Result<(), TipsetRangeSyncerError> {
    if header.signature.is_none() {
        return Err(TipsetRangeSyncerError::BlockWithoutSignature);
    }
    if header.bls_aggregate.is_none() {
        return Err(TipsetRangeSyncerError::BlockWithoutBlsAggregate);
    }
    Ok(())
}

/// Check the clock drift.
fn block_timestamp_checks(header: &CachingBlockHeader) -> Result<(), TipsetRangeSyncerError> {
    let time_now = chrono::Utc::now().timestamp() as u64;
    if header.timestamp > time_now.saturating_add(ALLOWABLE_CLOCK_DRIFT) {
        return Err(TipsetRangeSyncerError::TimeTravellingBlock(
            time_now,
            header.timestamp,
        ));
    } else if header.timestamp > time_now {
        warn!(
            "Got block from the future, but within clock drift threshold, {} > {}",
            header.timestamp, time_now
        );
    }
    Ok(())
}
