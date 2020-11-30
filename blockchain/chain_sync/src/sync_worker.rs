// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(test)]
mod full_sync_test;
#[cfg(test)]
mod validate_block_test;

use super::bad_block_cache::BadBlockCache;
use super::sync_state::{SyncStage, SyncState};
use super::{Error, SyncNetworkContext};
use actor::{is_account_actor, make_map_with_root, power, STORAGE_POWER_ACTOR_ADDR};
use address::Address;
use amt::Amt;
use async_std::sync::{Receiver, RwLock};
use async_std::task::{self, JoinHandle};
use beacon::{Beacon, BeaconEntry, IGNORE_DRAND_VAR};
use blocks::{Block, BlockHeader, FullTipset, Tipset, TipsetKeys, TxMeta};
use chain::{persist_objects, ChainStore};
use cid::{Cid, Code::Blake2b256};
use crypto::{verify_bls_aggregate, DomainSeparationTag};
use encoding::{Cbor, Error as EncodingError};
use fil_types::{
    verifier::ProofVerifier, Randomness, ALLOWABLE_CLOCK_DRIFT, BLOCK_DELAY_SECS, BLOCK_GAS_LIMIT,
    TICKET_RANDOMNESS_LOOKBACK, UPGRADE_SMOKE_HEIGHT,
};
use forest_libp2p::chain_exchange::TipsetBundle;
use futures::stream::{FuturesUnordered, StreamExt};
use interpreter::price_list_by_epoch;
use ipld_blockstore::BlockStore;
use log::{debug, info, warn};
use message::{Message, SignedMessage, UnsignedMessage};
use state_manager::StateManager;
use state_tree::StateTree;
use std::collections::HashMap;
use std::error::Error as StdError;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{cmp::min, convert::TryFrom};

/// Worker to handle syncing chain with the chain_exchange protocol.
pub(crate) struct SyncWorker<DB, TBeacon, V> {
    /// State of the sync worker.
    pub state: Arc<RwLock<SyncState>>,

    /// Drand randomness beacon.
    pub beacon: Arc<TBeacon>,

    /// manages retrieving and updates state objects.
    pub state_manager: Arc<StateManager<DB>>,

    /// Context to be able to send requests to p2p network.
    pub network: SyncNetworkContext<DB>,

    /// The known genesis tipset.
    pub genesis: Arc<Tipset>,

    /// Bad blocks cache, updates based on invalid state transitions.
    /// Will mark any invalid blocks and all childen as bad in this bounded cache.
    pub bad_blocks: Arc<BadBlockCache>,

    /// Proof verification implementation.
    pub verifier: PhantomData<V>,
}

impl<DB, TBeacon, V> SyncWorker<DB, TBeacon, V>
where
    TBeacon: Beacon + Sync + Send + 'static,
    DB: BlockStore + Sync + Send + 'static,
    V: ProofVerifier + Sync + Send + 'static,
{
    fn chain_store(&self) -> &Arc<ChainStore<DB>> {
        self.state_manager.chain_store()
    }

    pub async fn spawn(self, mut inbound_channel: Receiver<Arc<Tipset>>) -> JoinHandle<()> {
        task::spawn(async move {
            while let Some(ts) = inbound_channel.next().await {
                if let Err(e) = self.sync(ts).await {
                    let err = e.to_string();
                    warn!("failed to sync tipset: {}", &err);
                    self.state.write().await.error(err);
                }
            }
        })
    }

    /// Performs syncing process
    pub async fn sync(&self, head: Arc<Tipset>) -> Result<(), Error> {
        // Bootstrap peers before syncing
        // TODO increase bootstrap peer count before syncing
        const MIN_PEERS: usize = 1;
        loop {
            let peer_count = self.network.peer_manager().len().await;
            if peer_count < MIN_PEERS {
                debug!("bootstrapping peers, have {}", peer_count);
                task::sleep(Duration::from_secs(2)).await;
            } else {
                break;
            }
        }

        // Get heaviest tipset from storage to sync toward
        let heaviest = self.chain_store().heaviest_tipset().await.unwrap();

        info!("Starting block sync...");

        // Sync headers from network from head to heaviest from storage
        self.state
            .write()
            .await
            .init(heaviest.clone(), head.clone());
        let tipsets = match self.sync_headers_reverse(head.clone(), &heaviest).await {
            Ok(ts) => ts,
            Err(e) => {
                self.state.write().await.error(e.to_string());
                return Err(e);
            }
        };

        // Persist header chain pulled from network
        self.set_stage(SyncStage::PersistHeaders).await;
        let headers: Vec<&BlockHeader> = tipsets.iter().flat_map(|t| t.blocks()).collect();
        if let Err(e) = persist_objects(self.chain_store().blockstore(), &headers) {
            self.state.write().await.error(e.to_string());
            return Err(e.into());
        }
        // Sync and validate messages from fetched tipsets
        self.set_stage(SyncStage::Messages).await;
        if let Err(e) = self.sync_messages_check_state(tipsets).await {
            self.state.write().await.error(e.to_string());
            return Err(e);
        }
        self.set_stage(SyncStage::Complete).await;

        // At this point the head is synced and the head can be set as the heaviest.
        self.chain_store().put_tipset(&head).await?;

        Ok(())
    }

    /// Sets the managed sync status
    pub async fn set_stage(&self, new_stage: SyncStage) {
        debug!("Sync stage set to: {}", new_stage);
        self.state.write().await.set_stage(new_stage);
    }

    /// Syncs chain data and persists it to blockstore
    async fn sync_headers_reverse(
        &self,
        head: Arc<Tipset>,
        to: &Tipset,
    ) -> Result<Vec<Arc<Tipset>>, Error> {
        info!("Syncing headers from: {:?}", head.key());
        self.state.write().await.set_epoch(to.epoch());

        let mut accepted_blocks: Vec<Cid> = Vec::new();

        let sync_len = head.epoch() - to.epoch();
        if !sync_len.is_positive() {
            return Err(Error::Other(
                "Target tipset must be after heaviest".to_string(),
            ));
        }

        // invariant: never empty, only appended to
        let mut return_set = Vec::with_capacity(sync_len as usize + 1);
        return_set.push(head);

        // Loop until most recent tipset height is less than or equal to tipset height
        'sync: loop {
            let cur_ts = return_set.last().unwrap();

            // Check if parent cids exist in bad block caches
            self.validate_tipset_against_cache(cur_ts.parents(), &accepted_blocks)
                .await?;

            if cur_ts.epoch() <= to.epoch() {
                // Current tipset is less than epoch of tipset syncing toward
                break;
            }

            // Try to load parent tipset from local storage
            if let Ok(ts) = self.chain_store().tipset_from_keys(cur_ts.parents()).await {
                // Add blocks in tipset to accepted chain and push the tipset to return set
                accepted_blocks.extend_from_slice(ts.cids());
                return_set.push(ts);
                continue;
            }

            // TODO tweak request window when socket frame is tested
            const REQUEST_WINDOW: i64 = 200;
            let epoch_diff = cur_ts.epoch() - to.epoch();
            debug!("ChainExchange from: {} to {}", cur_ts.epoch(), to.epoch());
            let window = min(epoch_diff, REQUEST_WINDOW);

            // Load blocks from network using chain_exchange
            // TODO consider altering window size before returning error for failed sync.
            let tipsets = self
                .network
                .chain_exchange_headers(None, cur_ts.parents(), window as u64)
                .await?;

            info!(
                "Got tipsets: Height: {}, Len: {}",
                tipsets[0].epoch(),
                tipsets.len()
            );

            // Loop through each tipset received from network
            for ts in tipsets {
                if ts.epoch() < to.epoch() {
                    // Break out of sync loop if epoch lower than to tipset
                    // This should not be hit if response from server is correct
                    break 'sync;
                }
                // Check Cids of blocks against bad block cache
                self.validate_tipset_against_cache(&ts.key(), &accepted_blocks)
                    .await?;

                accepted_blocks.extend_from_slice(ts.cids());
                self.state.write().await.set_epoch(ts.epoch());
                // Add tipset to vector of tipsets to return
                return_set.push(ts);
            }
        }

        let last_ts = return_set.last().unwrap();

        // Check if local chain was fork
        if last_ts.key() != to.key() {
            info!("Local chain was fork. Syncing fork...");
            if last_ts.parents() == to.parents() {
                // block received part of same tipset as best block
                // This removes need to sync fork
                return Ok(return_set);
            }
            // add fork into return set
            let fork = self.sync_fork(&last_ts, &to).await?;
            info!("Fork Synced");
            return_set.extend(fork);
        }
        info!("Sync Header reverse complete");
        Ok(return_set)
    }

    /// checks to see if tipset is included in bad clocks cache
    async fn validate_tipset_against_cache(
        &self,
        ts: &TipsetKeys,
        accepted_blocks: &[Cid],
    ) -> Result<(), Error> {
        for cid in ts.cids() {
            if let Some(reason) = self.bad_blocks.get(cid).await {
                for bh in accepted_blocks {
                    self.bad_blocks
                        .put(*bh, format!("chain contained {}", cid))
                        .await;
                }

                return Err(Error::Other(format!(
                    "Chain contained block marked as bad: {}, {}",
                    cid, reason
                )));
            }
        }
        Ok(())
    }

    /// fork detected, collect tipsets to be included in return_set sync_headers_reverse
    async fn sync_fork(&self, head: &Tipset, to: &Tipset) -> Result<Vec<Arc<Tipset>>, Error> {
        // TODO move to shared parameter (from actors crate most likely)
        const FORK_LENGTH_THRESHOLD: u64 = 500;

        // TODO make this request more flexible with the window size, shouldn't require a node
        // to have to request all fork length headers at once.
        let tips = self
            .network
            .chain_exchange_headers(None, head.parents(), FORK_LENGTH_THRESHOLD)
            .await?;

        let mut ts = self.chain_store().tipset_from_keys(to.parents()).await?;

        for (i, tip) in tips.iter().enumerate() {
            while ts.epoch() > tip.epoch() {
                if ts.epoch() == 0 {
                    return Err(Error::Other(
                        "Synced chain forked at genesis, refusing to sync".to_string(),
                    ));
                }
                ts = self.chain_store().tipset_from_keys(ts.parents()).await?;
            }
            if ts == *tip {
                let mut tips = tips;
                tips.drain((i + 1)..);
                return Ok(tips);
            }
        }

        Err(Error::Other(
            "Fork longer than threshold finality of 500".to_string(),
        ))
    }

    /// Syncs messages by first checking state for message existence otherwise fetches messages from
    /// chain exchange.
    async fn sync_messages_check_state(&self, tipsets: Vec<Arc<Tipset>>) -> Result<(), Error> {
        let mut ts_iter = tipsets.into_iter().rev();
        // Currently syncing 1 height at a time, no reason for us to sync more
        const REQUEST_WINDOW: usize = 1;

        while let Some(ts) = ts_iter.next() {
            // check storage first to see if we have full tipset
            match self.chain_store().fill_tipset(&ts) {
                Some(fts) => {
                    // full tipset found in storage; validate and continue
                    let curr_epoch = fts.epoch();
                    self.validate_tipset(fts).await?;
                    self.state.write().await.set_epoch(curr_epoch);
                }
                None => {
                    // no full tipset in storage; request messages via chain_exchange

                    let batch_size = REQUEST_WINDOW;
                    debug!(
                        "ChainExchange message sync tipsets: epoch: {}, len: {}",
                        ts.epoch(),
                        batch_size
                    );

                    // receive tipset bundle from block sync
                    let compacted_messages = self
                        .network
                        .chain_exchange_messages(None, ts.key(), batch_size as u64)
                        .await?;

                    // Chain current tipset with iterator
                    let mut bs_iter = std::iter::once(ts).chain(&mut ts_iter);

                    // since the bundle only has messages, we have to put the headers in them
                    for messages in compacted_messages {
                        let t = bs_iter.next().ok_or_else(|| {
                            Error::Other("Messages returned exceeded tipsets in chain".to_string())
                        })?;

                        let bundle = TipsetBundle {
                            blocks: t.blocks().to_vec(),
                            messages: Some(messages),
                        };
                        // construct full tipsets from fetched messages
                        let fts = FullTipset::try_from(&bundle).map_err(Error::Other)?;

                        // validate tipset and messages
                        let curr_epoch = fts.epoch();
                        self.validate_tipset(fts).await?;
                        self.state.write().await.set_epoch(curr_epoch);

                        // store messages
                        if let Some(m) = bundle.messages {
                            let bs = self.state_manager.blockstore();
                            chain::persist_objects(bs, &m.bls_msgs)?;
                            chain::persist_objects(bs, &m.secp_msgs)?;
                        } else {
                            warn!("Chain Exchange request for messages returned null messages");
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// validates tipsets and adds header data to tipset tracker
    async fn validate_tipset(&self, fts: FullTipset) -> Result<(), Error> {
        if &fts.to_tipset() == self.genesis.as_ref() {
            debug!("Skipping tipset validation for genesis");
            return Ok(());
        }

        let epoch = fts.epoch();

        let mut validations = FuturesUnordered::new();
        for b in fts.into_blocks() {
            let sm = self.state_manager.clone();
            let bc = self.beacon.clone();
            let v = task::spawn(async move { Self::validate_block(sm, bc, Arc::new(b)).await });
            validations.push(v);
        }

        while let Some(result) = validations.next().await {
            match result {
                Ok(block) => {
                    self.chain_store()
                        .add_to_tipset_tracker(block.header())
                        .await;
                }
                Err((cid, e)) => {
                    // If the error is temporally invalidated, don't add to bad blocks cache.
                    if !matches!(e, Error::Temporal(_, _)) {
                        self.bad_blocks.put(cid, e.to_string()).await;
                    }
                    return Err(Error::Other(format!("Invalid block detected: {}", e)));
                }
            }
        }
        info!("Successfully validated tipset at epoch: {}", epoch);
        Ok(())
    }

    /// Validates block semantically according to https://github.com/filecoin-project/specs/blob/6ab401c0b92efb6420c6e198ec387cf56dc86057/validation.md
    /// Returns the validated block if `Ok`.
    /// Returns the block cid (for marking bad) and `Error` if invalid (`Err`).
    async fn validate_block(
        sm: Arc<StateManager<DB>>,
        bc: Arc<TBeacon>,
        block: Arc<Block>,
    ) -> Result<Arc<Block>, (Cid, Error)> {
        debug!(
            "Validating block at epoch: {} with weight: {}",
            block.header().epoch(),
            block.header().weight()
        );

        let cs = sm.chain_store().clone();
        let block_cid = block.cid();

        // Check block validation cache in store.
        let is_validated = cs
            .is_block_validated(block_cid)
            .map_err(|e| (*block_cid, e.into()))?;
        if is_validated {
            return Ok(block);
        }

        let mut error_vec: Vec<String> = Vec::new();
        let mut validations = FuturesUnordered::new();
        let header = block.header();

        // Check to ensure all optional values exist
        block_sanity_checks(header).map_err(|e| (*block_cid, e.into()))?;

        let base_ts = cs
            .tipset_from_keys(header.parents())
            .await
            .map_err(|e| (*block_cid, e.into()))?;

        // Retrieve lookback tipset for validation.
        let (lbts, lbst) = sm
            .get_lookback_tipset_for_round::<V>(base_ts.clone(), block.header().epoch())
            .await
            .map_err(|e| (*block_cid, e.into()))?;

        let lbst = Arc::new(lbst);

        let prev_beacon = cs
            .latest_beacon_entry(&base_ts)
            .await
            .map_err(|e| (*block_cid, e.into()))?;
        let prev_beacon = Arc::new(prev_beacon);

        // Timestamp checks
        let nulls = (header.epoch() - (base_ts.epoch() + 1)) as u64;
        let target_timestamp = base_ts.min_timestamp() + BLOCK_DELAY_SECS * (nulls + 1);
        if target_timestamp != header.timestamp() {
            return Err((
                *block_cid,
                Error::Validation(format!(
                    "block had the wrong timestamp: {} != {}",
                    header.timestamp(),
                    target_timestamp
                )),
            ));
        }
        let time_now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Retrieved system time before UNIX epoch")
            .as_secs();
        if header.timestamp() > time_now + ALLOWABLE_CLOCK_DRIFT {
            return Err((*block_cid, Error::Temporal(time_now, header.timestamp())));
        } else if header.timestamp() > time_now {
            warn!(
                "Got block from the future, but within clock drift threshold, {} > {}",
                header.timestamp(),
                time_now
            );
        }

        // Work address needed for async validations, so necessary to do sync to avoid duplication.
        let work_addr = sm
            .get_miner_work_addr(&lbst, header.miner_address())
            .map_err(|e| (*block_cid, e.into()))?;

        // Async validations

        // * Check block messages and their signatures as well as message root
        let b = Arc::clone(&block);
        let base_ts_clone = Arc::clone(&base_ts);
        let sm_c = Arc::clone(&sm);
        validations.push(task::spawn_blocking(move || {
            Self::check_block_msgs(sm_c, &b, &base_ts_clone)
                .map_err(|e| Error::Validation(e.to_string()))
        }));

        // * Miner validations
        let sm_c = Arc::clone(&sm);
        let b_cloned = Arc::clone(&block);
        let base_ts_clone = Arc::clone(&base_ts);
        validations.push(task::spawn_blocking(move || {
            let h = b_cloned.header();
            Self::validate_miner(&sm_c, h.miner_address(), base_ts_clone.parent_state())
        }));

        // * base fee check
        let base_ts_clone = Arc::clone(&base_ts);
        let bs_cloned = sm.blockstore_cloned();
        let b_cloned = Arc::clone(&block);
        validations.push(task::spawn_blocking(move || {
            let base_fee =
                chain::compute_base_fee(bs_cloned.as_ref(), &base_ts_clone).map_err(|e| {
                    Error::Validation(format!("Could not compute base fee: {}", e.to_string()))
                })?;

            let parent_base_fee = b_cloned.header().parent_base_fee();
            if &base_fee != parent_base_fee {
                return Err(Error::Validation(format!(
                    "base fee doesn't match: {} (header), {} (computed)",
                    parent_base_fee, base_fee
                )));
            }
            Ok(())
        }));

        // * Parent weight calculation check
        let bs_cloned = sm.blockstore_cloned();
        let base_ts_clone = Arc::clone(&base_ts);
        let weight = header.weight().clone();
        validations.push(task::spawn_blocking(move || {
            let calc_weight = chain::weight(bs_cloned.as_ref(), &base_ts_clone)
                .map_err(|e| Error::Other(format!("Error calculating weight: {}", e)))?;
            if weight != calc_weight {
                return Err(Error::Validation(format!(
                    "Parent weight doesn't match: {} (header), {} (computed)",
                    weight, calc_weight
                )));
            }
            Ok(())
        }));

        // * State root and receipt root validations
        let sm_cloned = Arc::clone(&sm);
        let base_ts_clone = Arc::clone(&base_ts);
        let b_cloned = Arc::clone(&block);
        validations.push(task::spawn(async move {
            let h = b_cloned.header();
            let (state_root, rec_root) = sm_cloned
                .tipset_state::<V>(&base_ts_clone)
                .await
                .map_err(|e| Error::Other(format!("Failed to calculate state: {}", e)))?;
            if &state_root != h.state_root() {
                return Err(Error::Validation(format!(
                    "Parent state root did not match computed state: {} (header), {} (computed)",
                    h.state_root(),
                    state_root,
                )));
            }
            if &rec_root != h.message_receipts() {
                return Err(Error::Validation(format!(
                    "Parent receipt root did not match computed root: {} (header), {} (computed)",
                    h.message_receipts(),
                    rec_root,
                )));
            }
            Ok(())
        }));

        // * Winner election PoSt validations
        let b_clone = Arc::clone(&block);
        let p_beacon = Arc::clone(&prev_beacon);
        let base_ts_clone = Arc::clone(&base_ts);
        let sm_c = Arc::clone(&sm);
        let lbst_clone = Arc::clone(&lbst);
        validations.push(task::spawn_blocking(move || {
            let h = b_clone.header();
            // Safe to unwrap because checked to be `Some` in sanity check.
            let election_proof = h.election_proof().as_ref().unwrap();

            if election_proof.win_count < 1 {
                return Err(Error::Validation(
                    "Block is not claiming to be a winner".to_string(),
                ));
            }

            let hp = sm_c.eligible_to_mine(h.miner_address(), &base_ts_clone, &lbts)?;
            if !hp {
                return Err(Error::Validation(
                    "Block's miner is ineligible to mine".to_string(),
                ));
            }

            let r_beacon = h.beacon_entries().last().unwrap_or(&p_beacon);

            let buf = h.miner_address().marshal_cbor()?;

            let vrf_base = chain::draw_randomness(
                r_beacon.data(),
                DomainSeparationTag::ElectionProofProduction,
                h.epoch(),
                &buf,
            )
            .map_err(|e| Error::Other(format!("failed to draw randomness: {}", e)))?;

            verify_election_post_vrf(&work_addr, &vrf_base, election_proof.vrfproof.as_bytes())
                .map_err(|e| format!("Winner election proof failed: {}", e))?;

            let slashed =
                sm_c.is_miner_slashed(h.miner_address(), &base_ts_clone.parent_state())?;
            if slashed {
                return Err(Error::Validation(
                    "Received block was from slashed or invalid miner".to_owned(),
                ));
            }

            let (mpow, tpow) = sm_c.get_power(&lbst_clone, h.miner_address())?;

            let j =
                election_proof.compute_win_count(&mpow.quality_adj_power, &tpow.quality_adj_power);
            if election_proof.win_count != j {
                return Err(Error::Validation(format!(
                    "miner claims wrong number of wins: miner: {}, computed {}",
                    election_proof.win_count, j
                )));
            }

            Ok(())
        }));

        // * Block signature check
        let b_cloned = Arc::clone(&block);
        let p_beacon = Arc::clone(&prev_beacon);
        validations.push(task::spawn_blocking(move || {
            // TODO switch logic to function attached to block header
            let block_sig_bytes = b_cloned.header().to_signing_bytes()?;

            // Can unwrap here because verified to be `Some` in the sanity checks.
            let block_sig = b_cloned.header().signature().as_ref().unwrap();
            block_sig
                .verify(&block_sig_bytes, &work_addr)
                .map_err(|e| Error::Blockchain(blocks::Error::InvalidSignature(e)))
        }));

        // * Beacon values check
        if std::env::var(IGNORE_DRAND_VAR) != Ok("1".to_owned()) {
            let block_cloned = Arc::clone(&block);
            validations.push(task::spawn(async move {
                block_cloned
                    .header()
                    .validate_block_drand(bc.as_ref(), &p_beacon)
                    .await
                    .map_err(|e| {
                        Error::Validation(format!(
                            "Failed to validate blocks random beacon values: {}",
                            e
                        ))
                    })
            }));
        }

        // * Ticket election proof validations
        let b_cloned = Arc::clone(&block);
        let p_beacon = Arc::clone(&prev_beacon);
        validations.push(task::spawn_blocking(move || {
            let h = b_cloned.header();
            let mut buf = h.miner_address().marshal_cbor()?;

            if h.epoch() > UPGRADE_SMOKE_HEIGHT {
                let vrf_proof = base_ts
                    .min_ticket()
                    .ok_or("Base tipset did not have ticket to verify")?
                    .vrfproof
                    .as_bytes();
                buf.extend_from_slice(vrf_proof);
            }

            let beacon_base = h.beacon_entries().last().unwrap_or(&p_beacon);

            let vrf_base = chain::draw_randomness(
                beacon_base.data(),
                DomainSeparationTag::TicketProduction,
                h.epoch() - TICKET_RANDOMNESS_LOOKBACK,
                &buf,
            )
            .map_err(|e| format!("failed to draw randomness: {}", e))?;

            verify_election_post_vrf(
                &work_addr,
                &vrf_base,
                // Safe to unwrap here because of block sanity checks
                h.ticket().as_ref().unwrap().vrfproof.as_bytes(),
            )
            .map_err(|e| format!("Ticket election proof failed: {}", e))?;

            Ok(())
        }));

        // * Winning PoSt proof validation
        let b_clone = block.clone();
        validations.push(task::spawn_blocking(move || {
            Self::verify_winning_post_proof(&sm, b_clone.header(), &prev_beacon, &lbst)
                .map_err(|e| format!("Verify winning PoSt failed: {}", e))?;

            Ok(())
        }));

        // collect the errors from the async validations
        while let Some(result) = validations.next().await {
            if let Err(e) = result {
                error_vec.push(e.to_string());
            }
        }

        // combine vec of error strings and return Validation error with this resultant string
        if !error_vec.is_empty() {
            let error_string = error_vec.join(", ");
            return Err((*block_cid, Error::Validation(error_string)));
        }

        cs.mark_block_as_validated(block_cid).map_err(|e| {
            (
                *block_cid,
                Error::Validation(format!(
                    "failed to mark block {} as validated: {}",
                    block_cid, e
                )),
            )
        })?;

        Ok(block)
    }

    // Block message validation checks
    fn check_block_msgs(
        state_manager: Arc<StateManager<DB>>,
        block: &Block,
        base_ts: &Tipset,
    ) -> Result<(), Box<dyn StdError>> {
        // do the initial loop here
        // Check Block Message and Signatures in them
        let mut pub_keys = Vec::new();
        let mut cids = Vec::new();
        for m in block.bls_msgs() {
            let pk = StateManager::get_bls_public_key(
                state_manager.blockstore(),
                m.from(),
                base_ts.parent_state(),
            )?;
            pub_keys.push(pk);
            cids.push(m.to_signing_bytes());
        }

        if let Some(sig) = block.header().bls_aggregate() {
            if !verify_bls_aggregate(
                cids.iter()
                    .map(|x| x.as_slice())
                    .collect::<Vec<&[u8]>>()
                    .as_slice(),
                pub_keys
                    .iter()
                    .map(|x| &x[..])
                    .collect::<Vec<&[u8]>>()
                    .as_slice(),
                &sig,
            ) {
                return Err(
                    format!("Bls aggregate signature {:?} was invalid: {:?}", sig, cids).into(),
                );
            }
        } else {
            return Err("No bls signature included in the block header".into());
        }

        let pl = price_list_by_epoch(base_ts.epoch());
        let mut sum_gas_limit = 0;

        // check msgs for validity
        let mut check_msg = |msg: &UnsignedMessage,
                             account_sequences: &mut HashMap<Address, u64>,
                             tree: &StateTree<DB>|
         -> Result<(), Box<dyn StdError>> {
            // Phase 1: syntactic validation
            let min_gas = pl.on_chain_message(msg.marshal_cbor().unwrap().len());
            msg.valid_for_block_inclusion(min_gas.total())?;

            sum_gas_limit += msg.gas_limit();
            if sum_gas_limit > BLOCK_GAS_LIMIT {
                return Err("block gas limit exceeded".into());
            }

            // Phase 2: (Partial) semantic validation
            // Sender exists and is account actor, and sequence is correct
            let sequence: u64 = match account_sequences.get(msg.from()) {
                Some(sequence) => *sequence,
                // Sequence does not exist in map, get actor from state
                None => {
                    let act = tree.get_actor(msg.from())?.ok_or({
                        "Failed to retrieve nonce for addr: Actor does not exist in state"
                    })?;

                    if !is_account_actor(&act.code) {
                        return Err("Sender must be an account actor".into());
                    }
                    act.sequence
                }
            };

            // sequence equality check
            if sequence != msg.sequence() {
                return Err(format!(
                    "Message has incorrect sequence (exp: {} got: {})",
                    sequence,
                    msg.sequence()
                )
                .into());
            }

            // Update account sequence
            account_sequences.insert(*msg.from(), sequence + 1);
            Ok(())
        };

        let mut account_sequences: HashMap<Address, u64> = HashMap::default();
        let db = state_manager.blockstore();
        let (state_root, _) = task::block_on(state_manager.tipset_state::<V>(&base_ts))
            .map_err(|e| format!("Could not update state: {}", e))?;
        let tree = StateTree::new_from_root(db, &state_root)
            .map_err(|e| format!("Could not load from new state root in state manager: {}", e))?;

        // loop through bls messages and check msg validity
        for (i, m) in block.bls_msgs().iter().enumerate() {
            check_msg(m, &mut account_sequences, &tree)
                .map_err(|e| format!("block had invalid bls message at index {}: {}", i, e))?;
        }
        // loop through secp messages and check msg validity and signature
        for (i, m) in block.secp_msgs().iter().enumerate() {
            check_msg(m.message(), &mut account_sequences, &tree)
                .map_err(|e| format!("block had invalid secp message at index {}: {}", i, e))?;

            // Resolve key address for signature verification
            let k_addr = task::block_on(state_manager.resolve_to_key_addr::<V>(m.from(), base_ts))
                .map_err(|e| e.to_string())?;

            // Secp256k1 signature validation
            m.signature()
                .verify(&m.message().to_signing_bytes(), &k_addr)
                .map_err(|e| format!("Message signature invalid: {}", e))?;
        }
        // validate message root from header matches message root
        let sm_root = compute_msg_meta(db, block.bls_msgs(), block.secp_msgs())?;
        if block.header().messages() != &sm_root {
            return Err(format!(
                "Invalid message root expected {} calculated {}",
                block.header().messages(),
                sm_root
            )
            .into());
        }

        Ok(())
    }

    fn verify_winning_post_proof(
        sm: &StateManager<DB>,
        header: &BlockHeader,
        prev_entry: &BeaconEntry,
        lbst: &Cid,
    ) -> Result<(), Error>
    where
        V: ProofVerifier,
    {
        // TODO allow for insecure validation to skip these checks

        let buf = header.miner_address().marshal_cbor()?;

        let rbase = header.beacon_entries().iter().last().unwrap_or(prev_entry);

        let rand = chain::draw_randomness(
            rbase.data(),
            DomainSeparationTag::WinningPoStChallengeSeed,
            header.epoch(),
            &buf,
        )
        .map_err(|err| {
            Error::Validation(format!(
                "failed to get randomness for verifying winningPost proof: {:}",
                err
            ))
        })?;

        let id = header.miner_address().id().map_err(|e| {
            Error::Validation(format!(
                "failed to get ID from miner address {}: {}",
                header.miner_address(),
                e
            ))
        })?;

        let sectors = sm
            .get_sectors_for_winning_post::<V>(&lbst, &header.miner_address(), Randomness(rand))
            .map_err(|e| {
                Error::Validation(format!("Failed to get sectors for post: {}", e.to_string()))
            })?;

        V::verify_winning_post(Randomness(rand), header.winning_post_proof(), &sectors, id).map_err(
            |e| Error::Validation(format!("Failed to verify winning PoSt: {}", e.to_string())),
        )
    }

    fn validate_miner(sm: &StateManager<DB>, maddr: &Address, ts_state: &Cid) -> Result<(), Error> {
        let spast: power::State = sm
            .load_actor_state(&*STORAGE_POWER_ACTOR_ADDR, ts_state)
            .map_err(|e| format!("Could not load power state: {}", e))?;

        let cm = make_map_with_root::<_, power::Claim>(&spast.claims, sm.blockstore())?;

        if cm.contains_key(&maddr.to_bytes())? {
            Ok(())
        } else {
            Err(Error::Validation(
                "Miner isn't valid from power state".to_string(),
            ))
        }
    }
}

/// Helper function to verify VRF proofs.
fn verify_election_post_vrf(worker: &Address, rand: &[u8], evrf: &[u8]) -> Result<(), String> {
    // TODO allow for insecure post validation to skip checks
    crypto::verify_vrf(worker, rand, evrf)
}

/// Checks optional values in header and returns reference to the values.
fn block_sanity_checks(header: &BlockHeader) -> Result<(), &'static str> {
    if header.election_proof().is_none() {
        return Err("Block cannot have no election proof");
    }
    if header.signature().is_none() {
        return Err("Block had no signature");
    }
    if header.bls_aggregate().is_none() {
        return Err("Block had no bls aggregate signature");
    }
    if header.ticket().is_none() {
        return Err("Block had no ticket");
    }
    Ok(())
}

/// Returns message root CID from bls and secp message contained in the param Block
pub fn compute_msg_meta<DB: BlockStore>(
    blockstore: &DB,
    bls_msgs: &[UnsignedMessage],
    secp_msgs: &[SignedMessage],
) -> Result<Cid, Error> {
    // collect bls and secp cids
    let bls_cids = cids_from_messages(bls_msgs)?;
    let secp_cids = cids_from_messages(secp_msgs)?;

    // generate Amt and batch set message values
    // TODO avoid having to clone all cids (from iter function on Amt)
    let bls_root = Amt::new_from_slice(blockstore, &bls_cids)?;
    let secp_root = Amt::new_from_slice(blockstore, &secp_cids)?;

    let meta = TxMeta {
        bls_message_root: bls_root,
        secp_message_root: secp_root,
    };

    // store message roots and receive meta_root cid
    let meta_root = blockstore
        .put(&meta, Blake2b256)
        .map_err(|e| Error::Other(e.to_string()))?;

    Ok(meta_root)
}

fn cids_from_messages<T: Cbor>(messages: &[T]) -> Result<Vec<Cid>, EncodingError> {
    messages.iter().map(Cbor::cid).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_std::sync::channel;
    use beacon::MockBeacon;
    use db::MemoryDB;
    use fil_types::verifier::MockVerifier;
    use forest_libp2p::NetworkMessage;
    use libp2p::PeerId;
    use std::sync::Arc;
    use std::time::Duration;
    use test_utils::{construct_chain_exchange_response, construct_dummy_header, construct_tipset};

    fn sync_worker_setup(
        db: Arc<MemoryDB>,
    ) -> (
        SyncWorker<MemoryDB, MockBeacon, MockVerifier>,
        Receiver<NetworkMessage>,
    ) {
        let chain_store = Arc::new(ChainStore::new(db.clone()));

        let (local_sender, test_receiver) = channel(20);

        let gen = construct_dummy_header();
        chain_store.set_genesis(&gen).unwrap();

        let beacon = Arc::new(MockBeacon::new(Duration::from_secs(1)));

        let genesis_ts = Arc::new(Tipset::new(vec![gen]).unwrap());
        (
            SyncWorker {
                state: Default::default(),
                beacon,
                state_manager: Arc::new(StateManager::new(chain_store)),
                network: SyncNetworkContext::new(local_sender, Default::default(), db),
                genesis: genesis_ts,
                bad_blocks: Default::default(),
                verifier: Default::default(),
            },
            test_receiver,
        )
    }

    fn send_chain_exchange_response(chain_exchange_message: Receiver<NetworkMessage>) {
        let rpc_response = construct_chain_exchange_response();

        task::block_on(async {
            match chain_exchange_message.recv().await.unwrap() {
                NetworkMessage::ChainExchangeRequest {
                    response_channel, ..
                } => {
                    response_channel.send(rpc_response).unwrap();
                }
                _ => unreachable!(),
            }
        });
    }

    #[test]
    fn sync_headers_reverse_given_tipsets_test() {
        let db = Arc::new(MemoryDB::default());
        let (sw, network_receiver) = sync_worker_setup(db);

        // params for sync_headers_reverse
        let source = PeerId::random();
        let head = Arc::new(construct_tipset(4, 10));
        let to = construct_tipset(1, 10);

        task::block_on(async move {
            sw.network
                .peer_manager()
                .update_peer_head(source.clone(), Some(head.clone()))
                .await;
            assert_eq!(sw.network.peer_manager().len().await, 1);
            // make chain_exchange request
            let return_set = task::spawn(async move { sw.sync_headers_reverse(head, &to).await });
            // send chain_exchange response to channel
            send_chain_exchange_response(network_receiver);
            assert_eq!(return_set.await.unwrap().len(), 4);
        });
    }
}
