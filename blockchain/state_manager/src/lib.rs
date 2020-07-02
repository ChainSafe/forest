// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod call;
mod errors;
pub mod utils;
pub use self::errors::*;
use actor::{
    init, market, market::MarketBalance, miner, power, ActorState, BalanceTable, INIT_ACTOR_ADDR,
    STORAGE_POWER_ACTOR_ADDR,
};
use address::{Address, BLSPublicKey, Payload, BLS_PUB_LEN};
use async_log::span;
use async_std::sync::RwLock;
use blockstore::BlockStore;
use blockstore::BufferedBlockStore;
use chain::{block_messages, ChainMessage, ChainStore};
use cid::Cid;
use encoding::de::DeserializeOwned;
use flo_stream::Subscriber;
use forest_blocks::{Block, BlockHeader, FullTipset, Tipset, TipsetKeys};
use futures::stream;
use futures::*;
use interpreter::{resolve_to_key_addr, ApplyRet, ChainRand, DefaultSyscalls, VM};
use ipld_amt::Amt;
use log::trace;
use message::{Message, MessageReceipt, UnsignedMessage};
use num_bigint::BigUint;
use num_traits::Zero;
use state_tree::StateTree;
use std::collections::HashMap;
use std::error::Error as StdError;
use std::sync::Arc;

/// Intermediary for retrieving state objects and updating actor states
pub type CidPair = (Cid, Cid);

pub struct StateManager<DB> {
    bs: Arc<DB>,
    cache: RwLock<HashMap<TipsetKeys, CidPair>>,
    subscriber: Option<Subscriber<ChainMessage>>,
    back_search_wait: Option<Subscriber<()>>,
}

impl<DB> StateManager<DB>
where
    DB: BlockStore,
{
    /// constructor
    pub fn new(bs: Arc<DB>) -> Self {
        Self {
            bs,
            cache: RwLock::new(HashMap::new()),
            subscriber: None,
            back_search_wait: None,
        }
    }

    pub fn new_with_chain_meessage_subscriber(
        bs: Arc<DB>,
        chain_sub: Subscriber<ChainMessage>,
    ) -> Self {
        Self {
            bs,
            cache: RwLock::new(HashMap::new()),
            subscriber: Some(chain_sub),
            back_search_wait: None,
        }
    }

    pub fn new_with_subscribers(
        bs: Arc<DB>,
        chain_subs: Subscriber<ChainMessage>,
        back_search_sub: Subscriber<()>,
    ) -> Self {
        Self {
            bs,
            cache: RwLock::new(HashMap::new()),
            subscriber: Some(chain_subs),
            back_search_wait: Some(back_search_sub),
        }
    }
    /// Loads actor state from IPLD Store
    pub fn load_actor_state<D>(&self, addr: &Address, state_cid: &Cid) -> Result<D, Error>
    where
        D: DeserializeOwned,
    {
        let actor = self
            .get_actor(addr, state_cid)?
            .ok_or_else(|| Error::ActorNotFound(addr.to_string()))?;
        let act: D = self
            .bs
            .get(&actor.state)
            .map_err(|e| Error::State(e.to_string()))?
            .ok_or_else(|| Error::ActorStateNotFound(actor.state.to_string()))?;
        Ok(act)
    }
    fn get_actor(&self, addr: &Address, state_cid: &Cid) -> Result<Option<ActorState>, Error> {
        let state = StateTree::new_from_root(self.bs.as_ref(), state_cid).map_err(Error::State)?;
        state.get_actor(addr).map_err(Error::State)
    }

    pub fn get_block_store(&self) -> Arc<DB> {
        self.bs.clone()
    }

    pub fn get_block_store_ref(&self) -> &DB {
        &self.bs
    }

    /// Returns the network name from the init actor state
    pub fn get_network_name(&self, st: &Cid) -> Result<String, Error> {
        let state: init::State = self.load_actor_state(&*INIT_ACTOR_ADDR, st)?;
        Ok(state.network_name)
    }
    /// Returns true if miner has been slashed or is considered invalid
    // TODO update
    pub fn is_miner_slashed(&self, addr: &Address, state_cid: &Cid) -> Result<bool, Error> {
        let _ms: miner::State = self.load_actor_state(addr, state_cid)?;

        let ps: power::State = self.load_actor_state(&*STORAGE_POWER_ACTOR_ADDR, state_cid)?;
        match ps.get_claim(self.bs.as_ref(), addr)? {
            Some(_) => Ok(false),
            None => Ok(true),
        }
    }
    /// Returns raw work address of a miner
    pub fn get_miner_work_addr(&self, state_cid: &Cid, addr: &Address) -> Result<Address, Error> {
        let ms: miner::State = self.load_actor_state(addr, state_cid)?;

        let state = StateTree::new_from_root(self.bs.as_ref(), state_cid).map_err(Error::State)?;
        // Note: miner::State info likely to be changed to CID
        let addr = resolve_to_key_addr(&state, self.bs.as_ref(), &ms.info.worker)
            .map_err(|e| Error::Other(format!("Failed to resolve key address; error: {}", e)))?;
        Ok(addr)
    }
    /// Returns specified actor's claimed power and total network power as a tuple
    pub fn get_power(&self, state_cid: &Cid, addr: &Address) -> Result<(BigUint, BigUint), Error> {
        let ps: power::State = self.load_actor_state(&*STORAGE_POWER_ACTOR_ADDR, state_cid)?;

        if let Some(claim) = ps.get_claim(self.bs.as_ref(), addr)? {
            Ok((claim.raw_byte_power, claim.quality_adj_power))
        } else {
            Err(Error::State(
                "Failed to retrieve claimed power from actor state".to_owned(),
            ))
        }
    }

    /// Performs the state transition for the tipset and applies all unique messages in all blocks.
    /// This function returns the state root and receipt root of the transition.
    pub fn apply_blocks(
        &self,
        ts: &FullTipset,
        rand: &ChainRand,
        mut call_back: Option<impl FnMut(Cid, UnsignedMessage, ApplyRet) -> Result<(), String>>,
    ) -> Result<(Cid, Cid), Box<dyn StdError>> {
        let mut buf_store = BufferedBlockStore::new(self.bs.as_ref());
        // TODO possibly switch out syscalls to be saved at state manager level
        let mut vm = VM::new(
            ts.parent_state(),
            &buf_store,
            ts.epoch(),
            DefaultSyscalls::new(&buf_store),
            rand,
        )?;

        // Apply tipset messages
        let receipts = vm.apply_tip_set_messages(ts, call_back)?;

        // Construct receipt root from receipts
        let rect_root = Amt::new_from_slice(self.bs.as_ref(), &receipts)?;

        // Flush changes to blockstore
        let state_root = vm.flush()?;
        // Persist changes connected to root
        buf_store.flush(&state_root)?;

        Ok((state_root, rect_root))
    }

    pub async fn tipset_state(&self, tipset: &Tipset) -> Result<(Cid, Cid), Box<dyn StdError>> {
        span!("tipset_state", {
            trace!("tipset {:?}", tipset.cids());
            // if exists in cache return
            if let Some(cid_pair) = self.cache.read().await.get(&tipset.key()) {
                return Ok(cid_pair.clone());
            }

            if tipset.epoch() == 0 {
                // NB: This is here because the process that executes blocks requires that the
                // block miner reference a valid miner in the state tree. Unless we create some
                // magical genesis miner, this won't work properly, so we short circuit here
                // This avoids the question of 'who gets paid the genesis block reward'
                let message_receipts = tipset
                    .blocks()
                    .first()
                    .ok_or_else(|| Error::Other("Could not get message receipts".to_string()))?;
                let cid_pair = (
                    tipset.parent_state().clone(),
                    message_receipts.message_receipts().clone(),
                );
                self.cache
                    .write()
                    .await
                    .insert(tipset.key().clone(), cid_pair.clone());
                return Ok(cid_pair);
            }

            let block_headers = tipset.blocks();
            // generic constants are not implemented yet this is a lowcost method for now
            let cid_pair = self.compute_tipset_state(
                &block_headers,
                Some(|Cid, UnsignedMessage, ApplyRet| Ok(())),
            )?;
            self.cache
                .write()
                .await
                .insert(tipset.key().clone(), cid_pair.clone());
            Ok(cid_pair)
        })
    }

    pub fn compute_tipset_state<'a>(
        &'a self,
        blocks_headers: &[BlockHeader],
        mut call_back: Option<impl FnMut(Cid, UnsignedMessage, ApplyRet) -> Result<(), String>>,
    ) -> Result<(Cid, Cid), Box<dyn StdError>> {
        span!("compute_tipset_state", {
            let check_for_duplicates = |s: &BlockHeader| {
                blocks_headers
                    .iter()
                    .filter(|val| val.miner_address() == s.miner_address())
                    .take(2)
                    .count()
            };
            if blocks_headers.iter().any(|s| check_for_duplicates(s) > 1) {
                // Duplicate Miner found
                return Err(Box::new(Error::Other(
                    "Could not get message receipts".to_string(),
                )));
            }

            let chain_store = ChainStore::new(self.bs.clone());
            let tipset_keys =
                TipsetKeys::new(blocks_headers.iter().map(|s| s.cid()).cloned().collect());
            let chain_rand = ChainRand::new(tipset_keys);

            let blocks = blocks_headers
                .iter()
                .map::<Result<Block, Box<dyn StdError>>, _>(|s: &BlockHeader| {
                    let (bls_messages, secp_messages) =
                        block_messages(chain_store.blockstore(), &s)?;
                    Ok(Block {
                        header: s.clone(),
                        bls_messages,
                        secp_messages,
                    })
                })
                .collect::<Result<Vec<Block>, _>>()?;
            // convert tipset to fulltipset
            let full_tipset = FullTipset::new(blocks)?;
            self.apply_blocks(&full_tipset, &chain_rand, call_back)
        })
    }

    pub fn look_up_id(&self, addr: &Address, tipset: &Tipset) -> Result<Address, Error> {
        let tipset = ChainStore::new(self.get_block_store())
            .tipset_from_keys(tipset.key())
            .map_err(|e| {
                Error::Other(format!(
                    "Could not get chain store from root {:}",
                    e.to_string()
                ))
            })?;
        let state_tree =
            StateTree::new_from_root(self.get_block_store_ref(), tipset.parent_state())?;
        state_tree.lookup_id(addr).map_err(Error::State)
    }

    
    pub fn market_balance(&self, _addr: &Address, _tipset: &Tipset) -> Result<MarketBalance, Error> {
        //will merge rupan's implementation
        unimplemented!("market balance fn not implemented in current build")
    }

    fn search_back_for_message(
        &self,
        tipset: &Tipset,
        message: &Message,
    ) -> Result<Option<(Tipset, MessageReceipt)>, Error> {
        let current = tipset.to_owned();

        if current.epoch() == 0 {
            return Ok(None);
        }

        if let Some(actor_state) = self.get_actor(message.from(), tipset.parent_state())? {
            if actor_state.sequence == 0 || actor_state.sequence < message.sequence() {
                return Ok(None);
            }
        }

        let tipset = chain::tipset_from_keys(self.get_block_store_ref(), current.parents())
            .map_err(|err| {
                Error::Other(format!(
                    "failed to load tipset during msg wait searchback: {:}",
                    err
                ))
            })?;
        let cid = message.to_cid()?;
        let r = self.tipset_executed_message(&tipset, &cid, message)?;

        if let Some(receipt) = r {
            return Ok(Some((tipset, receipt)));
        }
        self.search_back_for_message(&tipset, message)
    }

    pub fn get_receipt(&self,tipset : &Tipset,msg:&Cid) -> Result<MessageReceipt, Error>
    {
        let m = chain::get_chain_message(self.get_block_store_ref(),msg).map_err(|e|Error::Other(e.to_string()))?;
        let message_receipt = self.tipset_executed_message(tipset,msg,&*m)?;

        if let Some(receipt) = message_receipt
        {
            return Ok(receipt)
        }

        let maybe_tuple = self.search_back_for_message(tipset,&*m)?;
        let message_receipt = maybe_tuple.ok_or_else(||Error::Other("Could not get receipt from search back message".to_string()))?.1;
        Ok(message_receipt)
    }

    fn tipset_executed_message(
        &self,
        tipset: &Tipset,
        cid: &Cid,
        message: &Message,
    ) -> Result<Option<MessageReceipt>, Error> {
        if tipset.epoch() == 0 {
            return Ok(None);
        }

        let tipset = chain::tipset_from_keys(self.get_block_store_ref(), tipset.parents())
            .map_err(|err| Error::Other(err.to_string()))?;
        let messages = chain::message_for_tipset(self.get_block_store_ref(), &tipset)
            .map_err(|err| Error::Other(err.to_string()))?;
        messages
            .iter()
            .zip(0..messages.len())
            .rev()
            .filter(|(s, index)| s.from() == message.from())
            .filter_map(|(s, index)| {
                if s.sequence() == message.sequence() {
                    if s.to_cid().map(|s| &s == cid).unwrap_or_default() {
                        return Some(
                            chain::get_parent_reciept(
                                self.get_block_store_ref(),
                                tipset.blocks().first().unwrap(),
                                index as u64,
                            )
                            .map_err(|err| {
                                Error::Other(err.to_string())
                            }),
                        );
                    }
                    if let Ok(m_cid) = s.to_cid()
                    {
                        return Some(Err(Error::Other(format!("found message with equal nonce as the one we are looking for (F:{:} n {:}, TS: {:} n{:})",cid,message.sequence(),m_cid,s.sequence()))))
                    }
                    else
                    {
                        return Some(Err(Error::Other(format!("found message with equal nonce as the one we are looking for (F:{:} n {:}, TS: `Error Converting message to Cid` n{:})",cid,message.sequence(),s.sequence()))))
                    }
                    
                }
                if s.sequence() < message.sequence() {
                    return Some(Ok(None));
                }

                None
            })
            .next()
            .unwrap_or_else(|| Ok(None))
    }

    pub async fn wait_for_message(
        &self,
        cid: &Cid,
        confidence: u64,
    ) -> Result<Option<(Arc<Tipset>, MessageReceipt)>, Error> {
        let mut subscribers = self.subscriber.clone().ok_or_else(|| {
            Error::Other("State Manager not subscribed to tipset head changes".to_string())
        })?;
        let mut back_search_wait = self.back_search_wait.clone().ok_or_else(|| {
            Error::Other("State manager not subscribed to back search wait".to_string())
        })?;
        let message = chain::get_chain_message(self.get_block_store_ref(), cid).map_err(|err| {
            Error::Other(format!("failed to load message {:}",err))
        })?;

        let subscribers: Vec<ChainMessage> = subscribers.collect().await;
        let first_subscriber = subscribers.iter().nth(0).ok_or_else(|| {
            Error::Other("SubHeadChanges first entry should have been one item".to_string())
        })?;

     
        let tipset = match first_subscriber {
            ChainMessage::HcCurrent(tipset) => tipset,
            _ => {
                return Err(Error::Other(
                    format!("expected current head on SHC stream (got {:?})", first_subscriber)
                ))
            }
        };
        let maybe_message_reciept = self.tipset_executed_message(&tipset, cid, &*message)?;
        if let Some(r) = maybe_message_reciept {
            if let ChainMessage::HcCurrent(current) = first_subscriber {
                return Ok(Some((current.clone(), r)));
            }
        }

        let (back_tipset, back_receipt) = self
            .search_back_for_message(&tipset, &*message)?
            .ok_or_else(|| {
                Error::Other("State manager not subscribed to back search wait".to_string())
            })?;

        let mut candidate_tipset: Option<Arc<Tipset>> = None;
        let mut candidate_receipt: Option<MessageReceipt> = None;
        let mut height_of_head = tipset.epoch();
        let mut reverts: HashMap<TipsetKeys, bool> = HashMap::new();
        loop {
            while let Some(subscriber) = self
                .subscriber
                .clone()
                .ok_or_else(|| {
                    Error::Other("State Manager not subscribed to tipset head changes".to_string())
                })?
                .next()
                .await
            {
                match subscriber {
                    ChainMessage::HcRevert(tipset) => {
                        if let Some(_) = candidate_tipset {
                            candidate_tipset = None;
                            candidate_receipt = None;
                        }
                    }
                    ChainMessage::HcApply(tipset) => {
                        if candidate_tipset
                            .as_ref()
                            .map(|s| s.epoch() >= s.epoch() + tipset.epoch())
                            .unwrap_or_default()
                        {
                            let ts = candidate_tipset.ok_or_else(|| {
                                Error::Other(
                                    "Candidate Tipset not".to_string(),
                                )
                            })?;

                            let rs = candidate_receipt.ok_or_else(|| {
                                Error::Other(
                                    "Candidate Receipt not set".to_string(),
                                )
                            })?;

                            return Ok(Some((ts, rs)));
                        }

                        reverts.insert(tipset.key().to_owned(), true);

                        let maybe_receipt =
                            self.tipset_executed_message(&tipset, cid, &*message)?;
                        if let Some(receipt) = maybe_receipt {
                            if confidence == 0 {
                                return Ok(Some((tipset.clone(), receipt)));
                            }
                            candidate_tipset = Some(tipset);
                            candidate_receipt = Some(receipt)
                        }
                    }
                    _ => continue,
                }
            }

            match back_search_wait.next().await {
                Some(_) => {
                    if !reverts.get(back_tipset.key()).unwrap_or(&false) {
                        if height_of_head > back_tipset.epoch() + confidence {
                            let ts = candidate_tipset.ok_or_else(|| {
                                Error::Other(
                                    "Candidate Tipset not".to_string(),
                                )
                            })?;

                            let rs = candidate_receipt.ok_or_else(|| {
                                Error::Other(
                                    "Candidate Receipt not set".to_string(),
                                )
                            })?;

                            return Ok(Some((ts, rs)));
                        }
                    }
                }
                _ => continue,
            }
        }
    }

    /// Returns a bls public key from provided address
    pub fn get_bls_public_key(
        db: &Arc<DB>,
        addr: &Address,
        state_cid: &Cid,
    ) -> Result<[u8; BLS_PUB_LEN], Error> {
        let state = StateTree::new_from_root(db.as_ref(), state_cid).map_err(Error::State)?;
        let kaddr = resolve_to_key_addr(&state, db.as_ref(), addr)
            .map_err(|e| format!("Failed to resolve key address, error: {}", e))?;

        match kaddr.into_payload() {
            Payload::BLS(BLSPublicKey(key)) => Ok(key),
            _ => Err(Error::State(
                "Address must be BLS address to load bls public key".to_owned(),
            )),
        }
    }
}
