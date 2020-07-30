// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(test)]
mod peer_test;

use super::bad_block_cache::BadBlockCache;
use super::bucket::{SyncBucket, SyncBucketSet};
use super::network_handler::NetworkHandler;
use super::peer_manager::PeerManager;
use super::sync_state::{SyncStage, SyncState};
use super::{Error, SyncNetworkContext};
use address::{Address, Protocol};
use amt::Amt;
use async_std::sync::{Receiver, RwLock, Sender};
use async_std::task;
use beacon::{Beacon, BeaconEntry};
use blocks::{Block, BlockHeader, FullTipset, Tipset, TipsetKeys, TxMeta};
use chain::{persist_objects, ChainStore};
use cid::{multihash::Blake2b256, Cid};
use commcid::cid_to_replica_commitment_v1;
use core::time::Duration;
use crypto::verify_bls_aggregate;
use crypto::DomainSeparationTag;
use encoding::{Cbor, Error as EncodingError};
use fil_types::SectorInfo;
use filecoin_proofs_api::{post::verify_winning_post, ProverId, PublicReplicaInfo, SectorId};
use flo_stream::{MessagePublisher, Publisher};
use forest_libp2p::{
    hello::HelloRequest, BlockSyncRequest, NetworkEvent, NetworkMessage, MESSAGES,
};
use futures::{
    executor::block_on,
    stream::{FuturesUnordered, StreamExt},
};
use ipld_blockstore::BlockStore;
use libp2p::core::PeerId;
use log::error;
use log::{debug, info, warn};
use message::{Message, SignedMessage, UnsignedMessage};
use num_traits::Zero;
use state_manager::{utils, StateManager};
use state_tree::StateTree;
use std::cmp::min;
use std::collections::{BTreeMap, HashMap};
use std::convert::{TryFrom, TryInto};
use std::sync::Arc;
use vm::TokenAmount;

/// Struct that handles the ChainSync logic. This handles incoming network events such as
/// gossipsub messages, Hello protocol requests, as well as sending and receiving BlockSync
/// messages to be able to do the initial sync.
pub struct ChainSyncer<DB, TBeacon> {
    /// Syncing state of chain sync
    // TODO should be a vector once syncing done async and ideally not wrap each state in mutex.
    state: Arc<RwLock<SyncState>>,

    /// Drand randomness beacon
    beacon: Arc<TBeacon>,

    /// manages retrieving and updates state objects
    state_manager: Arc<StateManager<DB>>,

    /// Bucket queue for incoming tipsets
    sync_queue: SyncBucketSet,

    /// Represents next tipset to be synced
    next_sync_target: SyncBucket,

    /// access and store tipsets / blocks / messages
    chain_store: ChainStore<DB>,

    /// Context to be able to send requests to p2p network
    network: SyncNetworkContext,

    /// the known genesis tipset
    genesis: Tipset,

    /// Bad blocks cache, updates based on invalid state transitions.
    /// Will mark any invalid blocks and all childen as bad in this bounded cache
    bad_blocks: Arc<BadBlockCache>,

    ///  incoming network events to be handled by syncer
    net_handler: NetworkHandler,

    /// Peer manager to handle full peers to send ChainSync requests to
    peer_manager: Arc<PeerManager>,
}

/// Message data used to ensure valid state transition
struct MsgMetaData {
    balance: TokenAmount,
    sequence: u64,
}

impl<DB, TBeacon: 'static> ChainSyncer<DB, TBeacon>
where
    TBeacon: Beacon + Send,
    DB: BlockStore + Sync + Send + 'static,
{
    pub fn new(
        chain_store: ChainStore<DB>,
        beacon: Arc<TBeacon>,
        network_send: Sender<NetworkMessage>,
        network_rx: Receiver<NetworkEvent>,
        genesis: Tipset,
    ) -> Result<Self, Error> {
        let state_manager = Arc::new(StateManager::new(chain_store.db.clone()));

        // Split incoming channel to handle blocksync requests
        let mut event_send = Publisher::new(30);
        let network = SyncNetworkContext::new(network_send, event_send.subscribe());

        let peer_manager = Arc::new(PeerManager::default());

        let net_handler = NetworkHandler::new(network_rx, event_send);

        Ok(Self {
            state: Arc::new(RwLock::new(SyncState::default())),
            beacon,
            state_manager,
            chain_store,
            network,
            genesis,
            bad_blocks: Arc::new(BadBlockCache::default()),
            net_handler,
            peer_manager,
            sync_queue: SyncBucketSet::default(),
            next_sync_target: SyncBucket::default(),
        })
    }

    /// Returns a clone of the bad blocks cache to be used outside of chain sync.
    pub fn bad_blocks_cloned(&self) -> Arc<BadBlockCache> {
        self.bad_blocks.clone()
    }

    /// Returns the atomic reference to the syncing state.
    pub fn sync_state_cloned(&self) -> Arc<RwLock<SyncState>> {
        self.state.clone()
    }

    /// Spawns a network handler and begins the syncing process.
    pub async fn start(mut self) -> Result<(), Error> {
        self.net_handler.spawn(Arc::clone(&self.peer_manager));

        while let Some(event) = self.network.receiver.next().await {
            match event {
                NetworkEvent::HelloRequest { request, channel } => {
                    let source = channel.peer.clone();
                    debug!(
                        "Message inbound, heaviest tipset cid: {:?}",
                        request.heaviest_tip_set
                    );
                    match self
                        .fetch_tipset(source.clone(), &TipsetKeys::new(request.heaviest_tip_set))
                        .await
                    {
                        Ok(fts) => {
                            if let Err(e) = self.inform_new_head(source.clone(), &fts).await {
                                warn!("Failed to sync with provided tipset: {}", e);
                            };
                        }
                        Err(e) => {
                            warn!("Failed to fetch full tipset from peer ({}): {}", source, e);
                        }
                    }
                }
                NetworkEvent::PeerDialed { peer_id } => {
                    let heaviest = self.chain_store.heaviest_tipset().unwrap();
                    self.network
                        .hello_request(
                            peer_id,
                            HelloRequest {
                                heaviest_tip_set: heaviest.cids().to_vec(),
                                heaviest_tipset_height: heaviest.epoch(),
                                heaviest_tipset_weight: heaviest.weight().clone(),
                                genesis_hash: self.genesis.blocks()[0].cid().clone(),
                            },
                        )
                        .await
                }
                _ => (),
            }
        }
        Ok(())
    }

    /// Performs syncing process
    async fn sync(&mut self, head: Arc<Tipset>) -> Result<(), Error> {
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
        let heaviest = self.chain_store.heaviest_tipset().unwrap();

        info!("Starting block sync...");

        // Sync headers from network from head to heaviest from storage
        self.state
            .write()
            .await
            .init(heaviest.clone(), head.clone());
        let tipsets = match self
            .sync_headers_reverse(head.as_ref().clone(), &heaviest)
            .await
        {
            Ok(ts) => ts,
            Err(e) => {
                self.state.write().await.error(e.to_string());
                return Err(e);
            }
        };

        // Persist header chain pulled from network
        self.set_stage(SyncStage::PersistHeaders).await;
        let headers: Vec<&BlockHeader> = tipsets.iter().map(|t| t.blocks()).flatten().collect();
        if let Err(e) = persist_objects(self.chain_store.blockstore(), &headers) {
            self.state.write().await.error(e.to_string());
            return Err(e.into());
        }

        // Sync and validate messages from fetched tipsets
        self.set_stage(SyncStage::Messages).await;
        if let Err(e) = self.sync_messages_check_state(&tipsets).await {
            self.state.write().await.error(e.to_string());
            return Err(e);
        }
        self.set_stage(SyncStage::Complete).await;

        // At this point the head is synced and the head can be set as the heaviest.
        self.chain_store.put_tipset(head.as_ref()).await?;

        Ok(())
    }

    /// Syncs messages by first checking state for message existence otherwise fetches messages from blocksync
    async fn sync_messages_check_state(&mut self, ts: &[Tipset]) -> Result<(), Error> {
        // see https://github.com/filecoin-project/lotus/blob/master/build/params_shared.go#L109 for request window size
        const REQUEST_WINDOW: i64 = 200;
        // TODO refactor type handling
        // set i to the length of provided tipsets
        let mut i: i64 = i64::try_from(ts.len())? - 1;

        while i >= 0 {
            // check storage first to see if we have full tipset
            let fts = match self.chain_store.fill_tipsets(ts[i as usize].clone()) {
                Ok(fts) => fts,
                Err(_) => {
                    // no full tipset in storage; request messages via blocksync

                    // retrieve peerId used for blocksync request
                    if let Some(peer_id) = self.peer_manager.get_peer().await {
                        let mut batch_size = REQUEST_WINDOW;
                        if i < batch_size {
                            batch_size = i;
                        }

                        // set params for blocksync request
                        let idx = i - batch_size;
                        let next = &ts[idx as usize];
                        let req_len = batch_size + 1;

                        // receive tipset bundle from block sync
                        let ts_bundle = self
                            .network
                            .blocksync_request(
                                peer_id,
                                BlockSyncRequest {
                                    start: next.cids().to_vec(),
                                    request_len: req_len as u64,
                                    options: MESSAGES,
                                },
                            )
                            .await?;

                        for b in ts_bundle.chain {
                            // construct full tipsets from fetched messages
                            let fts: FullTipset = (&b).try_into().map_err(Error::Other)?;

                            // validate tipset and messages
                            let curr_epoch = fts.epoch();
                            self.validate_tipset(fts).await?;
                            self.state.write().await.set_epoch(curr_epoch);

                            // store messages
                            self.chain_store.put_messages(&b.bls_msgs)?;
                            self.chain_store.put_messages(&b.secp_msgs)?;
                        }
                    }
                    i -= REQUEST_WINDOW;
                    continue;
                }
            };
            // full tipset found in storage; validate and continue
            let curr_epoch = fts.epoch();
            self.validate_tipset(fts).await?;
            self.state.write().await.set_epoch(curr_epoch);
            i -= 1;
            continue;
        }

        Ok(())
    }

    /// informs the syncer about a new potential tipset
    /// This should be called when connecting to new peers, and additionally
    /// when receiving new blocks from the network
    pub async fn inform_new_head(&mut self, peer: PeerId, fts: &FullTipset) -> Result<(), Error> {
        // check if full block is nil and if so return error
        if fts.blocks().is_empty() {
            return Err(Error::NoBlocks);
        }

        for block in fts.blocks() {
            if let Some(bad) = self.bad_blocks.peek(block.cid()).await {
                warn!("Bad block detected, cid: {:?}", bad);
                return Err(Error::Other("Block marked as bad".to_string()));
            }
            // validate message data
            self.validate_msg_meta(block)?;
        }

        // compare target_weight to heaviest weight stored; ignore otherwise
        let best_weight = match self.chain_store.heaviest_tipset() {
            Some(ts) => ts.weight().clone(),
            None => Zero::zero(),
        };
        let target_weight = fts.weight();

        if target_weight.gt(&best_weight) {
            self.set_peer_head(peer, Arc::new(fts.to_tipset())).await?;
        }
        // incoming tipset from miners does not appear to be better than our best chain, ignoring for now
        Ok(())
    }

    async fn set_peer_head(&mut self, peer: PeerId, ts: Arc<Tipset>) -> Result<(), Error> {
        self.peer_manager
            .add_peer(peer, Some(Arc::clone(&ts)))
            .await;

        // Only update target on initial sync
        if self.state.read().await.stage() == SyncStage::Headers {
            if let Some(best_target) = self.select_sync_target().await {
                // TODO revisit this if using for full node, shouldn't start syncing on first update
                self.sync(best_target).await?;
                return Ok(());
            }
        }
        self.schedule_tipset(ts).await?;

        Ok(())
    }

    /// Retrieves the heaviest tipset in the sync queue; considered best target head
    async fn select_sync_target(&mut self) -> Option<Arc<Tipset>> {
        // Retrieve all peer heads from peer manager
        let mut heads = self.peer_manager.get_peer_heads().await;
        heads.sort_by_key(|h| h.epoch());

        // insert tipsets into sync queue
        for tip in heads {
            self.sync_queue.insert(tip);
        }

        if self.sync_queue.buckets().len() > 1 {
            warn!("Caution, multiple distinct chains seen during head selections");
        }

        // return heaviest tipset in queue
        self.sync_queue.heaviest()
    }
    /// Schedules a new tipset to be handled by the sync manager
    async fn schedule_tipset(&mut self, tipset: Arc<Tipset>) -> Result<(), Error> {
        info!("Scheduling incoming tipset to sync: {:?}", tipset.cids());

        // check sync status if indicates tipsets are ready to be synced
        // TODO revisit this, seems wrong
        if self.state.read().await.stage() == SyncStage::Complete {
            // send tipsets to be synced
            self.sync(tipset).await?;
            return Ok(());
        }

        // TODO check for related tipsets

        // if next_sync_target is from same chain as incoming tipset add it to be synced next
        if self.next_sync_target.is_same_chain_as(&tipset) {
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
    fn validate_msg_meta(&self, block: &Block) -> Result<(), Error> {
        let sm_root = compute_msg_meta(
            self.chain_store.blockstore(),
            block.bls_msgs(),
            block.secp_msgs(),
        )?;
        if block.header().messages() != &sm_root {
            return Err(Error::InvalidRoots);
        }

        self.chain_store.put_messages(block.bls_msgs())?;
        self.chain_store.put_messages(block.secp_msgs())?;

        Ok(())
    }

    /// Returns FullTipset from store if TipsetKeys exist in key-value store otherwise requests FullTipset
    /// from block sync
    async fn fetch_tipset(
        &mut self,
        peer_id: PeerId,
        tsk: &TipsetKeys,
    ) -> Result<FullTipset, String> {
        let fts = match self.load_fts(tsk) {
            Ok(fts) => fts,
            _ => return self.network.blocksync_fts(peer_id, tsk).await,
        };

        Ok(fts)
    }
    /// Returns a reconstructed FullTipset from store if keys exist
    fn load_fts(&self, keys: &TipsetKeys) -> Result<FullTipset, Error> {
        let mut blocks = Vec::new();
        // retrieve tipset from store based on passed in TipsetKeys
        let ts = self.chain_store.tipset_from_keys(keys)?;
        for header in ts.blocks() {
            // retrieve bls and secp messages from specified BlockHeader
            let (bls_msgs, secp_msgs) =
                chain::block_messages(self.chain_store.blockstore(), &header)?;

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
        let fts = FullTipset::new(blocks)?;
        Ok(fts)
    }
    // Block message validation checks
    fn check_block_msgs(
        state_manager: Arc<StateManager<DB>>,
        block: Block,
        tip: Tipset,
    ) -> Result<(), Error> {
        // do the initial loop here
        // Check Block Message and Signatures in them
        let mut pub_keys = Vec::new();
        let mut cids = Vec::new();
        for m in block.bls_msgs() {
            let pk = StateManager::get_bls_public_key(
                &state_manager.get_block_store(),
                m.from(),
                tip.parent_state(),
            )?;
            pub_keys.push(pk);
            cids.push(m.cid()?.to_bytes());
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
                return Err(Error::Validation(
                    "Bls aggregate signature was invalid".to_owned(),
                ));
            }
        } else {
            return Err(Error::Validation(
                "No bls signature included in the block header".to_owned(),
            ));
        }

        // check msgs for validity
        fn check_msg<M, DB: BlockStore>(
            msg: &M,
            msg_meta_data: &mut HashMap<Address, MsgMetaData>,
            tree: &StateTree<DB>,
        ) -> Result<(), Error>
        where
            M: Message,
        {
            let updated_state: MsgMetaData = match msg_meta_data.get(msg.from()) {
                // address is present begin validity checks
                Some(MsgMetaData { sequence, balance }) => {
                    // sequence equality check
                    if *sequence != msg.sequence() {
                        return Err(Error::Validation("Sequences are not equal".to_owned()));
                    }

                    // sufficient funds check
                    if *balance < msg.required_funds() {
                        return Err(Error::Validation(
                            "Insufficient funds for message execution".to_owned(),
                        ));
                    }
                    // update balance and increment sequence by 1
                    MsgMetaData {
                        balance: balance - &msg.required_funds(),
                        sequence: sequence + 1,
                    }
                }
                // MsgMetaData not found with provided address key, insert sequence and balance with address as key
                None => {
                    let actor = tree
                        .get_actor(msg.from())
                        .map_err(Error::Other)?
                        .ok_or_else(|| {
                            Error::Other("Could not retrieve actor from state tree".to_owned())
                        })?;

                    MsgMetaData {
                        sequence: actor.sequence,
                        balance: actor.balance,
                    }
                }
            };
            // update hash map with updated state
            msg_meta_data.insert(*msg.from(), updated_state);
            Ok(())
        }
        let mut msg_meta_data: HashMap<Address, MsgMetaData> = HashMap::default();
        let db = state_manager.get_block_store();
        let (state_root, _) = block_on(state_manager.tipset_state(&tip))
            .map_err(|_| Error::Validation("Could not update state".to_owned()))?;
        let tree = StateTree::new_from_root(db.as_ref(), &state_root).map_err(|_| {
            Error::Validation("Could not load from new state root in state manager".to_owned())
        })?;
        // loop through bls messages and check msg validity
        for m in block.bls_msgs() {
            check_msg(m, &mut msg_meta_data, &tree)?;
        }
        // loop through secp messages and check msg validity and signature
        for m in block.secp_msgs() {
            check_msg(m, &mut msg_meta_data, &tree)?;
            // signature validation
            m.signature()
                .verify(&m.cid()?.to_bytes(), m.from())
                .map_err(|e| Error::Validation(format!("Message signature invalid: {}", e)))?;
        }
        // validate message root from header matches message root
        let sm_root = compute_msg_meta(db.as_ref(), block.bls_msgs(), block.secp_msgs())?;
        if block.header().messages() != &sm_root {
            return Err(Error::InvalidRoots);
        }

        Ok(())
    }

    /// Validates block semantically according to https://github.com/filecoin-project/specs/blob/6ab401c0b92efb6420c6e198ec387cf56dc86057/validation.md
    async fn validate(&self, block: &Block) -> Result<(), Error> {
        let mut error_vec: Vec<String> = Vec::new();
        let mut validations = FuturesUnordered::new();
        let header = block.header();

        // check if block has been signed
        if header.signature().is_none() {
            error_vec.push("Signature is nil in header".to_owned());
        }

        let parent_tipset = self.chain_store.tipset_from_keys(header.parents())?;

        // time stamp checks
        if let Err(err) = header.validate_timestamps(&parent_tipset) {
            error_vec.push(err.to_string());
        }

        let b = block.clone();

        let parent_clone = parent_tipset.clone();
        // check messages to ensure valid state transitions
        let sm = self.state_manager.clone();
        let x = task::spawn_blocking(move || Self::check_block_msgs(sm, b, parent_clone));
        validations.push(x);

        // block signature check
        let (state_root, _) = self
            .state_manager
            .tipset_state(&parent_tipset)
            .await
            .map_err(|_| Error::Validation("Could not update state".to_owned()))?;
        let work_addr_result = self
            .state_manager
            .get_miner_work_addr(&state_root, header.miner_address());

        // temp header needs to live long enough in static context returned by task::spawn
        let signature = block.header().signature().clone();
        let cid_bytes = block.header().cid().to_bytes().clone();
        match work_addr_result {
            Ok(_) => validations.push(task::spawn_blocking(move || {
                signature
                    .ok_or_else(|| {
                        Error::Blockchain(blocks::Error::InvalidSignature(
                            "Signature is nil in header".to_owned(),
                        ))
                    })?
                    .verify(&cid_bytes, &work_addr_result.unwrap())
                    .map_err(|e| Error::Blockchain(blocks::Error::InvalidSignature(e)))
            })),
            Err(err) => error_vec.push(err.to_string()),
        }

        let slash = self
            .state_manager
            .is_miner_slashed(header.miner_address(), &parent_tipset.parent_state())
            .unwrap_or_else(|err| {
                error_vec.push(err.to_string());
                false
            });
        if slash {
            error_vec.push("Received block was from slashed or invalid miner".to_owned())
        }

        let prev_beacon = self
            .chain_store
            .latest_beacon_entry(&self.chain_store.tipset_from_keys(header.parents())?)?;
        header
            .validate_block_drand(Arc::clone(&self.beacon), prev_beacon)
            .await?;

        let power_result = self
            .state_manager
            .get_power(&parent_tipset.parent_state(), header.miner_address());
        // ticket winner check
        match power_result {
            Ok(pow_tuple) => {
                let (c_pow, net_pow) = pow_tuple;
                if !header.is_ticket_winner(c_pow, net_pow) {
                    error_vec.push("Miner created a block but was not a winner".to_owned())
                }
            }
            Err(err) => error_vec.push(err.to_string()),
        }

        // TODO verify_ticket_vrf

        // collect the errors from the async validations
        while let Some(result) = validations.next().await {
            if result.is_err() {
                error_vec.push(result.err().unwrap().to_string());
            }
        }
        // combine vec of error strings and return Validation error with this resultant string
        if !error_vec.is_empty() {
            let error_string = error_vec.join(", ");
            return Err(Error::Validation(error_string));
        }

        Ok(())
    }
    /// validates tipsets and adds header data to tipset tracker
    async fn validate_tipset(&mut self, fts: FullTipset) -> Result<(), Error> {
        if fts.to_tipset() == self.genesis {
            return Ok(());
        }

        for b in fts.blocks() {
            if let Err(e) = self.validate(&b).await {
                self.bad_blocks.put(b.cid().clone(), e.to_string()).await;
                return Err(Error::Other("Invalid blocks detected".to_string()));
            }
            self.chain_store.set_tipset_tracker(b.header())?;
        }
        Ok(())
    }

    pub async fn verify_winning_post_proof(
        &self,
        block: BlockHeader,
        prev_entry: BeaconEntry,
        lbst: Cid,
    ) -> Result<(), Error> {
        let marshal_miner_work_addr = block.miner_address().marshal_cbor()?;
        let rbase = block.beacon_entries().iter().last().unwrap_or(&prev_entry);
        let rand = chain::draw_randomness(
            rbase.data(),
            DomainSeparationTag::WinningPoStChallengeSeed,
            block.epoch(),
            &marshal_miner_work_addr,
        )
        .map_err(|err| {
            Error::Validation(format!(
                "failed to get randomness for verifying winningPost proof: {:}",
                err
            ))
        })?;
        if block.miner_address().protocol() != Protocol::ID {
            return Err(Error::Validation(format!(
                "failed to get ID from miner address {:}",
                block.miner_address()
            )));
        };
        let sectors = utils::get_sectors_for_winning_post(
            &self.state_manager,
            &lbst,
            &block.miner_address(),
            &rand,
        )?;

        let proofs = block
            .win_post_proof()
            .iter()
            .fold(Vec::new(), |mut proof, p| {
                proof.extend_from_slice(&p.proof_bytes);
                proof
            });

        let replicas = sectors
            .iter()
            .map::<Result<(SectorId, PublicReplicaInfo), Error>, _>(|sector_info: &SectorInfo| {
                let commr =
                    cid_to_replica_commitment_v1(&sector_info.sealed_cid).map_err(|err| {
                        Error::Validation(format!("failed to get replica commitment: {:}", err))
                    })?;
                let replica = PublicReplicaInfo::new(
                    sector_info
                        .proof
                        .registered_winning_post_proof()
                        .map_err(|err| Error::Validation(format!("Invalid proof code: {:}", err)))?
                        .try_into()
                        .map_err(|err| {
                            Error::Validation(format!("failed to get registered proof: {:}", err))
                        })?,
                    commr,
                );
                Ok((SectorId::from(sector_info.sector_number), replica))
            })
            .collect::<Result<BTreeMap<SectorId, PublicReplicaInfo>, _>>()?;

        let mut prover_id = ProverId::default();
        let prover_bytes = block.miner_address().to_bytes();
        prover_id[..prover_bytes.len()].copy_from_slice(&prover_bytes);
        if !verify_winning_post(&rand, &proofs, &replicas, prover_id)
            .map_err(|err| Error::Validation(format!("failed to verify election post: {:}", err)))?
        {
            error!("invalid winning post ({:?}; {:?})", rand, sectors);
            Err(Error::Validation("Winning post was invalid".to_string()))
        } else {
            Ok(())
        }
    }

    /// Syncs chain data and persists it to blockstore
    async fn sync_headers_reverse(
        &mut self,
        head: Tipset,
        to: &Tipset,
    ) -> Result<Vec<Tipset>, Error> {
        info!("Syncing headers from: {:?}", head.key());
        self.state.write().await.set_epoch(to.epoch());

        let mut accepted_blocks: Vec<Cid> = Vec::new();

        let sync_len = head.epoch() - to.epoch();
        if !sync_len.is_positive() {
            return Err(Error::Other(
                "Target tipset must be after heaviest".to_string(),
            ));
        }
        let mut return_set = Vec::with_capacity(sync_len as usize);
        return_set.push(head);

        let to_epoch = to.blocks().get(0).expect("Tipset cannot be empty").epoch();

        // Loop until most recent tipset height is less than to tipset height
        'sync: while let Some(cur_ts) = return_set.last() {
            // Check if parent cids exist in bad block caches
            self.validate_tipset_against_cache(cur_ts.parents(), &accepted_blocks)
                .await?;

            if cur_ts.epoch() <= to_epoch {
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

            // TODO tweak request window when socket frame is tested
            const REQUEST_WINDOW: i64 = 5;
            let epoch_diff = cur_ts.epoch() - to_epoch;
            let window = min(epoch_diff, REQUEST_WINDOW);

            let peer_id = self.get_peer().await;

            // Load blocks from network using blocksync
            let tipsets: Vec<Tipset> = match self
                .network
                .blocksync_headers(peer_id.clone(), cur_ts.parents(), window as u64)
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
                self.validate_tipset_against_cache(&ts.key(), &accepted_blocks)
                    .await?;

                accepted_blocks.extend_from_slice(ts.cids());
                self.state.write().await.set_epoch(ts.epoch());
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
    /// checks to see if tipset is included in bad clocks cache
    async fn validate_tipset_against_cache(
        &mut self,
        ts: &TipsetKeys,
        accepted_blocks: &[Cid],
    ) -> Result<(), Error> {
        for cid in ts.cids() {
            if let Some(reason) = self.bad_blocks.get(cid).await {
                for bh in accepted_blocks {
                    self.bad_blocks
                        .put(bh.clone(), format!("chain contained {}", cid))
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
    async fn sync_fork(&mut self, head: &Tipset, to: &Tipset) -> Result<Vec<Tipset>, Error> {
        let peer_id = self.get_peer().await;
        // TODO move to shared parameter (from actors crate most likely)
        const FORK_LENGTH_THRESHOLD: u64 = 500;

        // Load blocks from network using blocksync
        let tips: Vec<Tipset> = self
            .network
            .blocksync_headers(peer_id, head.parents(), FORK_LENGTH_THRESHOLD)
            .await
            .map_err(|_| Error::Other("Could not retrieve tipset".to_string()))?;

        let mut ts = self.chain_store.tipset_from_keys(to.parents())?;

        for i in 0..tips.len() {
            while ts.epoch() > tips[i].epoch() {
                if ts.epoch() == 0 {
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

    /// Sets the managed sync status
    pub async fn set_stage(&mut self, new_stage: SyncStage) {
        debug!("Sync stage set to: {}", new_stage);
        self.state.write().await.set_stage(new_stage);
    }

    async fn get_peer(&self) -> PeerId {
        while self.peer_manager.is_empty().await {
            warn!("No valid peers to sync, waiting for other nodes");
            task::sleep(Duration::from_secs(5)).await;
        }

        self.peer_manager
            .get_peer()
            .await
            .expect("Peer set is not empty here")
    }
}

/// Returns message root CID from bls and secp message contained in the param Block
fn compute_msg_meta<DB: BlockStore>(
    blockstore: &DB,
    bls_msgs: &[UnsignedMessage],
    secp_msgs: &[SignedMessage],
) -> Result<Cid, Error> {
    // collect bls and secp cids
    let bls_cids = cids_from_messages(bls_msgs)?;
    let secp_cids = cids_from_messages(secp_msgs)?;

    // generate Amt and batch set message values
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
    use async_std::sync::Sender;
    use beacon::MockBeacon;
    use blocks::BlockHeader;
    use db::MemoryDB;
    use forest_libp2p::NetworkEvent;
    use std::sync::Arc;
    use test_utils::{construct_blocksync_response, construct_messages, construct_tipset};

    fn chain_syncer_setup(
        db: Arc<MemoryDB>,
    ) -> (
        ChainSyncer<MemoryDB, MockBeacon>,
        Sender<NetworkEvent>,
        Receiver<NetworkMessage>,
    ) {
        let chain_store = ChainStore::new(db);

        let (local_sender, test_receiver) = channel(20);
        let (event_sender, event_receiver) = channel(20);

        let gen = dummy_header();
        chain_store.set_genesis(gen.clone()).unwrap();

        let beacon = Arc::new(MockBeacon::new(Duration::from_secs(1)));

        let genesis_ts = Tipset::new(vec![gen]).unwrap();
        (
            ChainSyncer::new(
                chain_store,
                beacon,
                local_sender,
                event_receiver,
                genesis_ts,
            )
            .unwrap(),
            event_sender,
            test_receiver,
        )
    }

    fn send_blocksync_response(blocksync_message: Receiver<NetworkMessage>) {
        let rpc_response = construct_blocksync_response();

        task::block_on(async {
            match blocksync_message.recv().await.unwrap() {
                NetworkMessage::BlockSyncRequest {
                    peer_id: _,
                    request: _,
                    response_channel,
                } => {
                    response_channel.send(rpc_response).unwrap();
                }
                _ => unreachable!(),
            }
        });
    }

    fn dummy_header() -> BlockHeader {
        BlockHeader::builder()
            .miner_address(Address::new_id(1000))
            .messages(Cid::new_from_cbor(&[1, 2, 3], Blake2b256))
            .message_receipts(Cid::new_from_cbor(&[1, 2, 3], Blake2b256))
            .state_root(Cid::new_from_cbor(&[1, 2, 3], Blake2b256))
            .build()
            .unwrap()
    }
    #[test]
    fn chainsync_constructor() {
        let db = Arc::new(MemoryDB::default());

        // Test just makes sure that the chain syncer can be created without using a live database or
        // p2p network (local channels to simulate network messages and responses)
        let _chain_syncer = chain_syncer_setup(db);
    }

    #[test]
    fn sync_headers_reverse_given_tipsets_test() {
        let db = Arc::new(MemoryDB::default());
        let (mut cs, _event_sender, network_receiver) = chain_syncer_setup(db);

        cs.net_handler.spawn(Arc::clone(&cs.peer_manager));

        // params for sync_headers_reverse
        let source = PeerId::random();
        let head = construct_tipset(4, 10);
        let to = construct_tipset(1, 10);

        task::block_on(async move {
            cs.peer_manager.add_peer(source.clone(), None).await;
            assert_eq!(cs.peer_manager.len().await, 1);
            // make blocksync request
            let return_set = task::spawn(async move { cs.sync_headers_reverse(head, &to).await });
            // send blocksync response to channel
            send_blocksync_response(network_receiver);
            assert_eq!(return_set.await.unwrap().len(), 4);
        });
    }

    #[test]
    fn compute_msg_meta_given_msgs_test() {
        let db = Arc::new(MemoryDB::default());
        let (cs, _, _) = chain_syncer_setup(db);

        let (bls, secp) = construct_messages();

        let expected_root =
            Cid::from_raw_cid("bafy2bzacebx7t56l6urh4os4kzar5asc5hmbhl7so6sfkzcgpjforkwylmqxa")
                .unwrap();

        let root = compute_msg_meta(cs.chain_store.blockstore(), &[bls], &[secp]).unwrap();
        assert_eq!(root, expected_root);
    }

    #[test]
    fn empty_msg_meta_vector() {
        let blockstore = MemoryDB::default();
        let usm: Vec<UnsignedMessage> =
            encoding::from_slice(&base64::decode("gA==").unwrap()).unwrap();
        let sm: Vec<SignedMessage> =
            encoding::from_slice(&base64::decode("gA==").unwrap()).unwrap();

        assert_eq!(
            compute_msg_meta(&blockstore, &usm, &sm)
                .unwrap()
                .to_string(),
            "bafy2bzacecgw6dqj4bctnbnyqfujltkwu7xc7ttaaato4i5miroxr4bayhfea"
        );
    }
}
