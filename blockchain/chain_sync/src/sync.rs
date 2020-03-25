// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(test)]
mod peer_test;

use super::bucket::{SyncBucket, SyncBucketSet};
use super::network_handler::NetworkHandler;
use super::peer_manager::PeerManager;
use super::{Error, SyncNetworkContext};
use address::Address;
use amt::Amt;
use async_std::prelude::*;
use async_std::sync::{channel, Receiver, Sender};
use async_std::task;
use blocks::{Block, BlockHeader, FullTipset, TipSetKeys, Tipset, TxMeta};
use chain::{ChainStore, TipSetMetadata};
use cid::{multihash::Blake2b256, Cid};
use core::time::Duration;
use crypto::is_valid_signature;
use encoding::{Cbor, Error as EncodingError};
use forest_libp2p::{NetworkEvent, NetworkMessage};
use ipld_blockstore::BlockStore;
use libp2p::core::PeerId;
use log::{debug, info, warn};
use lru::LruCache;
use message::Message;
use num_bigint::BigUint;
use state_manager::StateManager;
use state_tree::{HamtStateTree, StateTree};
use std::cmp::min;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(PartialEq, Debug, Clone)]
/// Current state of the ChainSyncer
pub enum SyncState {
    /// Initial state, validating data structures and local chain
    Init,

    /// Bootstrap to the network, and acquire a secure enough set of peers
    Bootstrap,

    /// Syncing to checkpoint (using BlockSync for now)
    Checkpoint,

    /// Receive new blocks from the network and sync toward heaviest tipset
    Catchup,

    /// Once all blocks are validated to the heaviest chain, follow network
    /// by receiving blocks over the network and validating them
    _Follow,
}

pub struct ChainSyncer<DB> {
    /// Syncing state of chain sync
    state: SyncState,

    /// manages retrieving and updates state objects
    state_manager: StateManager<DB>,

    /// Bucket queue for incoming tipsets
    sync_queue: SyncBucketSet,
    /// Represents next tipset to be synced
    next_sync_target: SyncBucket,

    /// access and store tipsets / blocks / messages
    chain_store: ChainStore<DB>,

    /// Context to be able to send requests to p2p network
    network: SyncNetworkContext,

    /// the known genesis tipset
    _genesis: Tipset,

    /// Bad blocks cache, updates based on invalid state transitions.
    /// Will mark any invalid blocks and all childen as bad in this bounded cache
    bad_blocks: LruCache<Cid, String>,

    ///  incoming network events to be handled by syncer
    net_handler: NetworkHandler,

    /// Peer manager to handle full peers to send ChainSync requests to
    pub peer_manager: Arc<PeerManager>,
}

/// Message data used to ensure valid state transition
struct MsgMetaData {
    balance: BigUint,
    sequence: u64,
}

impl<DB> ChainSyncer<DB>
where
    DB: BlockStore,
{
    pub fn new(
        db: Arc<DB>,
        network_send: Sender<NetworkMessage>,
        network_rx: Receiver<NetworkEvent>,
    ) -> Result<Self, Error> {
        let chain_store = ChainStore::new(db.clone());
        let _genesis = match chain_store.genesis()? {
            Some(gen) => Tipset::new(vec![gen])?,
            None => {
                // TODO change default logic for genesis or setup better initialization
                warn!("No genesis found in data storage, using a default");
                Tipset::new(vec![BlockHeader::default()])?
            }
        };

        let state_manager = StateManager::new(db);

        // Split incoming channel to handle blocksync requests
        let (rpc_send, rpc_rx) = channel(20);
        let (event_send, event_rx) = channel(30);

        let network = SyncNetworkContext::new(network_send, rpc_rx, event_rx);

        let peer_manager = Arc::new(PeerManager::default());

        let net_handler = NetworkHandler::new(network_rx, rpc_send, event_send);

        Ok(Self {
            state: SyncState::Init,
            state_manager,
            chain_store,
            network,
            _genesis,
            bad_blocks: LruCache::new(1 << 15),
            net_handler,
            peer_manager,
            sync_queue: SyncBucketSet::default(),
            next_sync_target: SyncBucket::default(),
        })
    }
}

impl<DB> ChainSyncer<DB>
where
    DB: BlockStore,
{
    pub async fn start(mut self) -> Result<(), Error> {
        self.net_handler.spawn(Arc::clone(&self.peer_manager));

        while let Some(event) = self.network.receiver.next().await {
            if let NetworkEvent::Hello { source, message } = &event {
                info!(
                    "Message inbound, heaviest tipset cid: {:?}",
                    message.heaviest_tip_set
                );
                if let Ok(fts) = self
                    .fetch_tipset(
                        source.clone(),
                        &TipSetKeys::new(message.heaviest_tip_set.clone()),
                    )
                    .await
                {
                    if self.inform_new_head(&source.clone(), &fts).await.is_err() {
                        warn!("Failed to sync with provided tipset",);
                    };
                } else {
                    warn!(
                        "Failed to fetch full tipset from peer: {} from storage or network",
                        source,
                    );
                }
            }
        }
        Ok(())
    }

    /// Performs syncing process
    async fn sync(&mut self, head: Tipset) -> Result<(), Error> {
        // Bootstrap peers before syncing
        // TODO increase bootstrap peer count before syncing
        const MIN_PEERS: usize = 1;
        loop {
            let peer_count = self.peer_manager.len().await;
            if peer_count < MIN_PEERS {
                debug!("bootstrapping peers, have {}", peer_count);
                task::sleep(Duration::from_secs(2)).await;
            } else {
                break;
            }
        }

        // Get heaviest tipset from storage to sync toward
        let heaviest = self.chain_store.heaviest_tipset();

        info!("Starting block sync...");

        // Sync headers from network from head to heaviest from storage
        let headers = self.sync_headers_reverse(head, &heaviest).await?;
        self.set_state(SyncState::Catchup);

        // Persist header chain pulled from network
        self.persist_headers(&headers)?;

        // TODO iterate over headers to fill out and include message, then validate message and state
        // TODO implement sync_messages_check_state see = https://github.com/filecoin-project/lotus/blob/master/chain/sync.go#L1032
        // TODO update SyncState::Follow

        Ok(())
    }

    // fn sync_messages_check_state(&self, ts: [Tipset]) -> Result<(), Error> {
    //     const REQUEST_WINDOW: u64 = 100;

    //     for i in 0..ts.len() {
    //         let fts = self.chain_store.try_fill_tipsets(ts[i])?;
    //         self.validate_tipsets(fts)?;
    //     }
    //     Ok(())
    // }

    /// informs the syncer about a new potential tipset
    /// This should be called when connecting to new peers, and additionally
    /// when receiving new blocks from the network
    pub async fn inform_new_head(&mut self, from: &PeerId, fts: &FullTipset) -> Result<(), Error> {
        // check if full block is nil and if so return error
        if fts.blocks().is_empty() {
            return Err(Error::NoBlocks);
        }

        for block in fts.blocks() {
            if let Some(bad) = self.bad_blocks.peek(block.cid()) {
                warn!("Bad block detected, cid: {:?}", bad);
                return Err(Error::Other("Block marked as bad".to_string()));
            }
            // validate message data
            self.validate_msg_data(block)?;
        }

        // compare target_weight to heaviest weight stored; ignore otherwise
        let heaviest_tipset = self.chain_store.heaviest_tipset();
        let best_weight = heaviest_tipset.blocks()[0].weight();
        let target_weight = fts.blocks()[0].header().weight();

        if target_weight.gt(&best_weight) {
            // Set peer head
            self.set_peer_head(from, fts.tipset()?.clone()).await?;
        }
        // incoming tipset from miners does not appear to be better than our best chain, ignoring for now
        Ok(())
    }
    pub async fn set_peer_head(&mut self, peer: &PeerId, ts: Tipset) -> Result<(), Error> {
        // update peer heads map
        self.peer_manager
            .insert_peer_head(peer.clone(), ts.clone())
            .await;

        // initial sync
        if self.get_state() == &SyncState::Init {
            if let Some(best_target) = self.select_sync_target().await {
                self.sync(best_target).await?;
                return Ok(());
            }
        }
        self.schedule_tipset(ts).await?;
        Ok(())
    }
    /// Retrieves the heaviest tipset in the sync queue; considered best target head
    pub async fn select_sync_target(&mut self) -> Option<Tipset> {
        let mut heads = Vec::new();
        for (_, ts) in self.peer_manager.peer_heads.read().await.iter() {
            heads.clone().push(ts);
        }
        // sort tipsets by epoch
        heads.sort_by_key(|header| (*header.epoch()));

        // insert tipsets into sync queue
        for ts in heads {
            self.sync_queue.insert(ts.clone());
        }

        if self.sync_queue.buckets().len() > 1 {
            warn!("Caution, multiple distinct chains seen during head selections");
        }
        // return heaviest tipset in queue
        self.sync_queue.heaviest()
    }
    /// Schedules a new tipset to be handled by the sync manager
    async fn schedule_tipset(&mut self, tipset: Tipset) -> Result<(), Error> {
        info!("Scheduling incoming tipset to sync: {:?}", tipset.cids());

        // check sync status if indicates tipsets are ready to be synced
        if self.get_state() == &SyncState::Catchup {
            // send tipsets to be synced
            self.sync(tipset).await?;
            return Ok(());
        }

        // TODO check for related tipsets

        // if next_sync_target is from same chain as incoming tipset add it to be synced next
        if !self.next_sync_target.is_empty() && self.next_sync_target.same_chain_as(&tipset) {
            self.next_sync_target.add(tipset);
        } else {
            // add incoming tipset to queue to by synced later
            self.sync_queue.insert(tipset);
            // update next sync target if empty
            if self.next_sync_target.is_empty() {
                if let Some(target_bucket) = self.sync_queue.pop() {
                    self.next_sync_target = target_bucket;
                    if let Some(best_target) = self.next_sync_target.heaviest_tipset() {
                        // send heaviest tipset from sync target to be synced
                        self.sync(best_target).await?;
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }
    /// Validates message root from header matches message root generated from the
    /// bls and secp messages contained in the passed in block and stores them in a key-value store
    fn validate_msg_data(&self, block: &Block) -> Result<(), Error> {
        let sm_root = self.compute_msg_data(block)?;
        if block.header().messages() != &sm_root {
            return Err(Error::InvalidRoots);
        }

        self.chain_store.put_messages(block.bls_msgs())?;
        self.chain_store.put_messages(block.secp_msgs())?;

        Ok(())
    }
    /// Returns message root CID from bls and secp message contained in the param Block
    fn compute_msg_data(&self, block: &Block) -> Result<Cid, Error> {
        // collect bls and secp cids
        let bls_cids = cids_from_messages(block.bls_msgs())?;
        let secp_cids = cids_from_messages(block.secp_msgs())?;
        // generate Amt and batch set message values
        let bls_root = Amt::new_from_slice(self.chain_store.blockstore(), &bls_cids)?;
        let secp_root = Amt::new_from_slice(self.chain_store.blockstore(), &secp_cids)?;

        let meta = TxMeta {
            bls_message_root: bls_root,
            secp_message_root: secp_root,
        };
        // store message roots and receive meta_root
        let meta_root = self.chain_store.blockstore().put(&meta, Blake2b256)?;

        Ok(meta_root)
    }
    /// Returns FullTipset from store if TipSetKeys exist in key-value store otherwise requests FullTipset
    /// from block sync
    async fn fetch_tipset(
        &mut self,
        peer_id: PeerId,
        tsk: &TipSetKeys,
    ) -> Result<FullTipset, String> {
        let fts = match self.load_fts(tsk) {
            Ok(fts) => fts,
            _ => return self.network.blocksync_fts(peer_id, tsk).await,
        };

        Ok(fts)
    }
    /// Returns a reconstructed FullTipset from store if keys exist
    fn load_fts(&self, keys: &TipSetKeys) -> Result<FullTipset, Error> {
        let mut blocks = Vec::new();
        // retrieve tipset from store based on passed in TipSetKeys
        let ts = self.chain_store.tipset_from_keys(keys)?;
        for header in ts.blocks() {
            // retrieve bls and secp messages from specified BlockHeader
            let (bls_msgs, secp_msgs) = self.chain_store.messages(&header)?;
            // construct a full block
            let full_block = Block {
                header: header.clone(),
                bls_messages: bls_msgs,
                secp_messages: secp_msgs,
            };
            // push vector of full blocks to build FullTipset
            blocks.push(full_block);
        }
        // construct FullTipset
        let fts = FullTipset::new(blocks);
        Ok(fts)
    }
    // Block message validation checks
    pub fn check_blk_msgs(&self, block: Block, _tip: &Tipset) -> Result<(), Error> {
        // TODO retrieve bls public keys for verify_bls_aggregate
        // for _m in block.bls_msgs() {
        // }
        // TODO verify_bls_aggregate

        // check msgs for validity
        fn check_msg<M, ST>(
            msg: &M,
            msg_meta_data: &mut HashMap<Address, MsgMetaData>,
            tree: &ST,
        ) -> Result<(), Error>
        where
            M: Message,
            ST: StateTree,
        {
            let updated_state: MsgMetaData = match msg_meta_data.get(msg.from()) {
                // address is present begin validity checks
                Some(MsgMetaData { sequence, balance }) => {
                    // sequence equality check
                    if *sequence != msg.sequence() {
                        return Err(Error::Validation("Sequences are not equal".to_string()));
                    }

                    // sufficient funds check
                    if *balance < msg.required_funds() {
                        return Err(Error::Validation(
                            "Insufficient funds for message execution".to_string(),
                        ));
                    }
                    // update balance and increment sequence by 1
                    MsgMetaData {
                        balance: balance - msg.required_funds(),
                        sequence: sequence + 1,
                    }
                }
                // MsgMetaData not found with provided address key, insert sequence and balance with address as key
                None => {
                    let actor = tree
                        .get_actor(msg.from())
                        .map_err(Error::Blockchain)?
                        .ok_or_else(|| {
                            Error::State("Could not retrieve actor from state tree".to_owned())
                        })?;

                    MsgMetaData {
                        sequence: actor.sequence,
                        balance: actor.balance,
                    }
                }
            };
            // update hash map with updated state
            msg_meta_data.insert(msg.from().clone(), updated_state);
            Ok(())
        }
        let mut msg_meta_data: HashMap<Address, MsgMetaData> = HashMap::default();
        // TODO retrieve tipset state and load state tree
        // temporary
        let tree = HamtStateTree::new(self.chain_store.db.as_ref());
        // loop through bls messages and check msg validity
        for m in block.bls_msgs() {
            check_msg(m, &mut msg_meta_data, &tree)?;
        }
        // loop through secp messages and check msg validity and signature
        for m in block.secp_msgs() {
            check_msg(m, &mut msg_meta_data, &tree)?;
            // signature validation
            if !is_valid_signature(&m.cid()?.to_bytes(), m.from(), m.signature()) {
                return Err(Error::Validation(
                    "Message signature is not valid".to_string(),
                ));
            }
        }
        // validate message root from header matches message root
        let sm_root = self.compute_msg_data(&block)?;
        if block.header().messages() != &sm_root {
            return Err(Error::InvalidRoots);
        }

        Ok(())
    }

    /// Validates block semantically according to https://github.com/filecoin-project/specs/blob/6ab401c0b92efb6420c6e198ec387cf56dc86057/validation.md
    pub fn validate(&self, block: &Block) -> Result<(), Error> {
        // get header from full block
        let header = block.header();

        // check if block has been signed
        if header.signature().bytes().is_empty() {
            return Err(Error::Validation("Signature is nil in header".to_string()));
        }

        let base_tipset = self.load_fts(&TipSetKeys::new(header.parents().cids.clone()))?;
        let parent_tipset = base_tipset.tipset()?;
        // time stamp checks
        header.validate_timestamps(&base_tipset)?;

        // check messages to ensure valid state transitions
        self.check_blk_msgs(block.clone(), &parent_tipset)?;

        // block signature check
        // TODO need to pass in raw miner address; temp using header miner address
        // see https://github.com/filecoin-project/lotus/blob/master/chain/sync.go#L611
        header.check_block_signature(header.miner_address())?;

        // TODO: incomplete, still need to retrieve power in order to ensure ticket is the winner
        let _slash = self
            .state_manager
            .miner_slashed(header.miner_address(), &parent_tipset)?;
        let _sector_size = self
            .state_manager
            .miner_sector_size(header.miner_address(), &parent_tipset)?;

        // TODO winner_check
        // TODO miner_check
        // TODO verify_ticket_vrf
        // TODO verify_election_proof_check

        Ok(())
    }

    fn validate_tipsets(&self, fts: FullTipset) -> Result<(), Error> {
        if fts.tipset()? == self._genesis {
            return Ok(());
        }

        for b in fts.blocks() {
            self.validate(b)?;
            // add bad blocks to bad_blocks
            let meta = TipSetMetadata {
                tipset_state_root: fts.tipset()?.blocks()[0].state_root().clone(),
                tipset_receipts_root: fts.tipset()?.blocks()[0].message_receipts().clone(),
                tipset: fts.tipset()?,
            };
            self.chain_store.tip_index.put(&meta)?;
        }
        Ok(())
    }

    /// Syncs chain data and persists it to blockstore
    async fn sync_headers_reverse(
        &mut self,
        head: Tipset,
        to: &Tipset,
    ) -> Result<Vec<Tipset>, Error> {
        info!("Syncing headers from: {:?}", head.key());

        let mut accepted_blocks: Vec<Cid> = Vec::new();

        let mut return_set = vec![head];

        let to_epoch = to
            .blocks()
            .get(0)
            .ok_or_else(|| Error::Blockchain("Tipset must not be empty".to_owned()))?
            .epoch();

        // Loop until most recent tipset height is less than to tipset height
        'sync: while let Some(cur_ts) = return_set.last() {
            // Check if parent cids exist in bad block cache
            self.validate_tipset_against_cache(cur_ts.parents(), &accepted_blocks)?;

            if cur_ts.epoch() < to_epoch {
                // Current tipset is less than epoch of tipset syncing toward
                break;
            }

            // Try to load parent tipset from local storage
            if let Ok(ts) = self.chain_store.tipset_from_keys(cur_ts.parents()) {
                // Add blocks in tipset to accepted chain and push the tipset to return set
                accepted_blocks.extend_from_slice(ts.cids());
                return_set.push(ts);
                continue;
            }

            const REQUEST_WINDOW: u64 = 100;
            let epoch_diff = u64::from(cur_ts.epoch() - to_epoch);
            let window = min(epoch_diff, REQUEST_WINDOW);

            // update sync state to Bootstrap indicating we are acquiring a 'secure enough' set of peers
            self.set_state(SyncState::Bootstrap);

            // TODO change from using random peerID to managed
            while self.peer_manager.is_empty().await {
                warn!("No valid peers to sync, waiting for other nodes");
                task::sleep(Duration::from_secs(5)).await;
            }
            let peer_id = self
                .peer_manager
                .get_peer()
                .await
                .expect("Peer set is not empty here");

            // checkpoint established
            self.set_state(SyncState::Checkpoint);

            // Load blocks from network using blocksync
            let tipsets: Vec<Tipset> = match self
                .network
                .blocksync_headers(peer_id.clone(), cur_ts.parents(), window)
                .await
            {
                Ok(ts) => ts,
                Err(e) => {
                    warn!("Failed blocksync request to peer {:?}: {}", peer_id, e);
                    self.peer_manager.remove_peer(&peer_id).await;
                    continue;
                }
            };

            // Loop through each tipset received from network
            for ts in tipsets {
                if ts.epoch() < to_epoch {
                    // Break out of sync loop if epoch lower than to tipset
                    // This should not be hit if response from server is correct
                    break 'sync;
                }
                // Check Cids of blocks against bad block cache
                self.validate_tipset_against_cache(&ts.key(), &accepted_blocks)?;

                accepted_blocks.extend_from_slice(ts.cids());
                // Add tipset to vector of tipsets to return
                return_set.push(ts);
            }
        }

        let last_ts = return_set
            .last()
            .ok_or_else(|| Error::Other("Return set should contain a tipset".to_owned()))?;

        // Check if local chain was fork
        if last_ts.key() != to.key() {
            if last_ts.parents() == to.parents() {
                // block received part of same tipset as best block
                // This removes need to sync fork
                return Ok(return_set);
            }
            // add fork into return set
            let fork = self.sync_fork(&last_ts, &to).await?;
            return_set.extend(fork);
        }

        Ok(return_set)
    }

    fn validate_tipset_against_cache(
        &mut self,
        ts: &TipSetKeys,
        accepted_blocks: &[Cid],
    ) -> Result<(), Error> {
        for cid in ts.cids() {
            if let Some(reason) = self.bad_blocks.get(cid).cloned() {
                for bh in accepted_blocks {
                    self.bad_blocks
                        .put(bh.clone(), format!("chain contained {}", cid));
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
    async fn sync_fork(&mut self, head: &Tipset, to: &Tipset) -> Result<Vec<Tipset>, Error> {
        // TODO change from using random peerID to managed
        let peer_id = PeerId::random();
        // pulled from Lotus: https://github.com/filecoin-project/lotus/blob/master/chain/sync.go#L996
        const FORK_LENGTH_THRESHOLD: u64 = 500;

        // Load blocks from network using blocksync
        let tips: Vec<Tipset> = self
            .network
            .blocksync_headers(peer_id.clone(), head.parents(), FORK_LENGTH_THRESHOLD)
            .await
            .map_err(|_| Error::Other("Could not retrieve tipset".to_string()))?;

        let mut ts = self.chain_store.tipset_from_keys(to.parents())?;

        for i in 0..tips.len() {
            while ts.epoch() > tips[i].epoch() {
                if *ts.epoch().as_u64() == 0 {
                    return Err(Error::Other(
                        "Synced chain forked at genesis, refusing to sync".to_string(),
                    ));
                }
                ts = self.chain_store.tipset_from_keys(ts.parents())?;
            }
            if ts == tips[i] {
                return Ok(tips[0..=i].to_vec());
            }
        }

        Err(Error::Other(
            "Fork longer than threshold finality of 500".to_string(),
        ))
    }

    /// Persists headers from tipset slice to chain store
    fn persist_headers(&self, tipsets: &[Tipset]) -> Result<(), Error> {
        Ok(tipsets
            .iter()
            .try_for_each(|ts| self.chain_store.persist_headers(ts))?)
    }
    /// Returns the managed sync status
    pub fn get_state(&self) -> &SyncState {
        &self.state
    }
    /// Sets the managed sync status
    pub fn set_state(&mut self, new_state: SyncState) {
        self.state = new_state
    }
}

fn cids_from_messages<T: Cbor>(messages: &[T]) -> Result<Vec<Cid>, EncodingError> {
    messages.iter().map(Cbor::cid).collect()
}
