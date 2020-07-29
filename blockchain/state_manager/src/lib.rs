// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod errors;
pub mod utils;
pub use self::errors::*;
use actor::{
    init, market, miner, power, ActorState, BalanceTable, INIT_ACTOR_ADDR,
    STORAGE_MARKET_ACTOR_ADDR, STORAGE_POWER_ACTOR_ADDR,
};
use address::{Address, BLSPublicKey, Payload, BLS_PUB_LEN};
use async_log::span;
use async_std::{sync::RwLock, task};
use blockstore::BlockStore;
use blockstore::BufferedBlockStore;
use chain::{block_messages, get_heaviest_tipset, ChainStore, HeadChange};
use cid::Cid;
use clock::ChainEpoch;
use encoding::de::DeserializeOwned;
use encoding::Cbor;
use fil_types::DevnetParams;
use flo_stream::Subscriber;
use forest_blocks::{Block, BlockHeader, FullTipset, Tipset, TipsetKeys};
use futures::channel::oneshot;
use futures::stream::{FuturesUnordered, StreamExt};
use interpreter::{resolve_to_key_addr, ApplyRet, ChainRand, DefaultSyscalls, VM};
use ipld_amt::Amt;
use log::{trace, warn};
use message::{Message, MessageReceipt, UnsignedMessage};
use num_bigint::BigInt;
use state_tree::StateTree;
use std::collections::HashMap;
use std::error::Error as StdError;
use std::sync::Arc;

/// Intermediary for retrieving state objects and updating actor states
pub type CidPair = (Cid, Cid);

/// Type to represent invocation of state call results
pub struct InvocResult<Msg>
where
    Msg: Message,
{
    pub msg: Msg,
    pub msg_rct: Option<MessageReceipt>,
    pub actor_error: Option<String>,
}

// An alias Result that represents an InvocResult and an Error
pub type StateCallResult<T> = Result<InvocResult<T>, Error>;

#[allow(dead_code)]
#[derive(Default)]
pub struct MarketBalance {
    escrow: BigInt,
    locked: BigInt,
}

pub struct StateManager<DB> {
    bs: Arc<DB>,
    cache: RwLock<HashMap<TipsetKeys, CidPair>>,
    subscriber: Option<Subscriber<HeadChange>>,
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
        }
    }

    // Creates a constructor that passes in a HeadChange subscriber and a back_search subscriber
    pub fn new_with_subscribers(bs: Arc<DB>, chain_subs: Subscriber<HeadChange>) -> Self {
        Self {
            bs,
            cache: RwLock::new(HashMap::new()),
            subscriber: Some(chain_subs),
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
    pub fn get_power(&self, state_cid: &Cid, addr: &Address) -> Result<(BigInt, BigInt), Error> {
        let ps: power::State = self.load_actor_state(&*STORAGE_POWER_ACTOR_ADDR, state_cid)?;

        if let Some(claim) = ps.get_claim(self.bs.as_ref(), addr)? {
            Ok((claim.raw_byte_power, claim.quality_adj_power))
        } else {
            Err(Error::State(
                "Failed to retrieve claimed power from actor state".to_owned(),
            ))
        }
    }

    pub fn get_subscriber(&self) -> Option<Subscriber<HeadChange>> {
        self.subscriber.clone()
    }

    /// Performs the state transition for the tipset and applies all unique messages in all blocks.
    /// This function returns the state root and receipt root of the transition.
    pub fn apply_blocks(
        &self,
        ts: &FullTipset,
        rand: &ChainRand,
        callback: Option<impl FnMut(Cid, UnsignedMessage, ApplyRet) -> Result<(), String>>,
    ) -> Result<(Cid, Cid), Box<dyn StdError>> {
        let mut buf_store = BufferedBlockStore::new(self.bs.as_ref());
        // TODO possibly switch out syscalls to be saved at state manager level
        // TODO change from statically using devnet params when needed
        let mut vm = VM::<_, _, DevnetParams>::new(
            ts.parent_state(),
            &buf_store,
            ts.epoch(),
            DefaultSyscalls::new(&buf_store),
            rand,
        )?;

        // Apply tipset messages
        let receipts = vm.apply_tipset_messages(ts, callback)?;

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
            let no_func = None::<fn(Cid, UnsignedMessage, ApplyRet) -> Result<(), String>>;
            let cid_pair = self.compute_tipset_state(&block_headers, no_func)?;
            self.cache
                .write()
                .await
                .insert(tipset.key().clone(), cid_pair.clone());
            Ok(cid_pair)
        })
    }

    fn call_raw(
        &self,
        msg: &mut UnsignedMessage,
        bstate: &Cid,
        rand: &ChainRand,
        bheight: &ChainEpoch,
    ) -> StateCallResult<UnsignedMessage>
    where
        DB: BlockStore,
    {
        span!("state_call_raw", {
            let block_store = self.get_block_store_ref();
            let buf_store = BufferedBlockStore::new(block_store);
            let mut vm = VM::<_, _, DevnetParams>::new(
                bstate,
                &buf_store,
                *bheight,
                DefaultSyscalls::new(&buf_store),
                rand,
            )?;

            if msg.gas_limit() == 0 {
                msg.set_gas_limit(10000000000)
            }

            let actor = self
                .get_actor(msg.from(), bstate)?
                .ok_or_else(|| Error::Other("Could not get actor".to_string()))?;
            msg.set_sequence(actor.sequence);
            let apply_ret = vm.apply_implicit_message(msg);
            trace!(
                "gas limit {:},gas price {:?},value {:?}",
                msg.gas_limit(),
                msg.gas_price(),
                msg.value()
            );
            if let Some(err) = &apply_ret.act_error {
                warn!("chain call failed: {:?}", err);
            }

            Ok(InvocResult {
                msg: msg.clone(),
                msg_rct: Some(apply_ret.msg_receipt.clone()),
                actor_error: apply_ret.act_error.map(|e| e.to_string()),
            })
        })
    }

    /// runs the given message and returns its result without any persisted changes.
    pub fn call(
        &self,
        message: &mut UnsignedMessage,
        tipset: Option<Tipset>,
    ) -> StateCallResult<UnsignedMessage>
    where
        DB: BlockStore,
    {
        let ts = if let Some(t_set) = tipset {
            t_set
        } else {
            chain::get_heaviest_tipset(self.get_block_store_ref())
                .map_err(|_| Error::Other("Could not get heaviest tipset".to_string()))?
                .ok_or_else(|| Error::Other("Empty Tipset given".to_string()))?
        };
        let state = ts.parent_state();
        let chain_rand = ChainRand::new(ts.key().to_owned());
        self.call_raw(message, state, &chain_rand, &ts.epoch())
    }

    /// returns the result of executing the indicated message, assuming it was executed in the indicated tipset.
    pub fn replay(
        &self,
        ts: &Tipset,
        mcid: &Cid,
    ) -> Result<(UnsignedMessage, Option<ApplyRet>), Error>
    where
        DB: BlockStore,
    {
        let mut outm: Option<UnsignedMessage> = None;
        let mut outr: Option<ApplyRet> = None;
        let callback = |cid: Cid, unsigned: UnsignedMessage, apply_ret: ApplyRet| {
            if cid == mcid.clone() {
                outm = Some(unsigned);
                outr = Some(apply_ret);
                return Err("halt".to_string());
            }

            Ok(())
        };
        let result = self.compute_tipset_state(ts.blocks(), Some(callback));

        if let Err(error_message) = result {
            if error_message.to_string() != "halt" {
                return Err(Error::Other(format!(
                    "unexpected error during execution : {:}",
                    error_message
                )));
            }
        }

        let out_mes =
            outm.ok_or_else(|| Error::Other("given message not found in tipset".to_string()))?;
        Ok((out_mes, outr))
    }

    pub fn compute_tipset_state<'a>(
        &'a self,
        blocks_headers: &[BlockHeader],
        callback: Option<impl FnMut(Cid, UnsignedMessage, ApplyRet) -> Result<(), String>>,
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
            self.apply_blocks(&full_tipset, &chain_rand, callback)
        })
    }

    fn search_back_for_message(
        block_store: Arc<DB>,
        current: &Tipset,
        (message_from_address, message_cid, message_sequence): (&Address, &Cid, &u64),
    ) -> Result<Option<(Tipset, MessageReceipt)>, Error>
    where
        DB: BlockStore,
    {
        if current.epoch() == 0 {
            return Ok(None);
        }
        let state = StateTree::new_from_root(&*block_store, current.parent_state())
            .map_err(Error::State)?;

        if let Some(actor_state) = state
            .get_actor(message_from_address)
            .map_err(Error::State)?
        {
            if actor_state.sequence == 0 || actor_state.sequence < *message_sequence {
                return Ok(None);
            }
        }

        let tipset = chain::tipset_from_keys(&*block_store, current.parents()).map_err(|err| {
            Error::Other(format!(
                "failed to load tipset during msg wait searchback: {:}",
                err
            ))
        })?;
        let r = Self::tipset_executed_message(
            &*block_store,
            &tipset,
            message_cid,
            (message_from_address, message_sequence),
        )?;

        if let Some(receipt) = r {
            return Ok(Some((tipset, receipt)));
        }
        Self::search_back_for_message(
            block_store,
            &tipset,
            (message_from_address, message_cid, message_sequence),
        )
    }
    /// returns a message receipt from a given tipset and message cid
    pub fn get_receipt(&self, tipset: &Tipset, msg: &Cid) -> Result<MessageReceipt, Error> {
        let m = chain::get_chain_message(self.get_block_store_ref(), msg)
            .map_err(|e| Error::Other(e.to_string()))?;
        let message_var = (m.from(), &m.sequence());
        let message_receipt =
            Self::tipset_executed_message(self.get_block_store_ref(), tipset, msg, message_var)?;

        if let Some(receipt) = message_receipt {
            return Ok(receipt);
        }
        let cid = m
            .cid()
            .map_err(|e| Error::Other(format!("Could not convert message to cid {:?}", e)))?;
        let message_var = (m.from(), &cid, &m.sequence());
        let maybe_tuple =
            Self::search_back_for_message(self.get_block_store(), tipset, message_var)?;
        let message_receipt = maybe_tuple
            .ok_or_else(|| {
                Error::Other("Could not get receipt from search back message".to_string())
            })?
            .1;
        Ok(message_receipt)
    }

    fn tipset_executed_message(
        block_store: &DB,
        tipset: &Tipset,
        cid: &Cid,
        (message_from_address, message_sequence): (&Address, &u64),
    ) -> Result<Option<MessageReceipt>, Error>
    where
        DB: BlockStore,
    {
        if tipset.epoch() == 0 {
            return Ok(None);
        }
        let tipset = chain::tipset_from_keys(block_store, tipset.parents())
            .map_err(|err| Error::Other(err.to_string()))?;
        let messages = chain::messages_for_tipset(block_store, &tipset)
            .map_err(|err| Error::Other(err.to_string()))?;
        messages
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, s)| s.from() == message_from_address)
            .filter_map(|(index,s)| {
                if s.sequence() == *message_sequence {
                    if s.cid().map(|s| &s == cid).unwrap_or_default() {
                        return Some(
                            chain::get_parent_reciept(
                                block_store,
                                tipset.blocks().first().unwrap(),
                                index as u64,
                            )
                            .map_err(|err| {
                                Error::Other(err.to_string())
                            }),
                        );
                    }
                    let error_msg = format!("found message with equal nonce as the one we are looking for (F:{:} n {:}, TS: `Error Converting message to Cid` n{:})", cid, message_sequence, s.sequence());
                    return Some(Err(Error::Other(error_msg)))
                }
                if s.sequence() < *message_sequence {
                    return Some(Ok(None));
                }

                None
            })
            .next()
            .unwrap_or_else(|| Ok(None))
    }

    /// WaitForMessage blocks until a message appears on chain. It looks backwards in the chain to see if this has already
    /// happened. It guarantees that the message has been on chain for at least confidence epochs without being reverted
    /// before returning.
    pub async fn wait_for_message<'a>(
        block_store: Arc<DB>,
        subscriber: Option<Subscriber<HeadChange>>,
        cid: &Cid,
        confidence: i64,
    ) -> Result<(Option<Arc<Tipset>>, Option<MessageReceipt>), Error>
    where
        DB: BlockStore + Send + Sync + 'static,
    {
        let mut subscribers = subscriber.clone().ok_or_else(|| {
            Error::Other("State Manager not subscribed to tipset head changes".to_string())
        })?;
        let (sender, mut receiver) = oneshot::channel::<()>();
        let message = chain::get_chain_message(&*block_store, cid)
            .map_err(|err| Error::Other(format!("failed to load message {:}", err)))?;

        let maybe_subscriber: Option<HeadChange> = subscribers.next().await;
        let first_subscriber = maybe_subscriber.ok_or_else(|| {
            Error::Other("SubHeadChanges first entry should have been one item".to_string())
        })?;

        let tipset = match first_subscriber {
            HeadChange::Current(tipset) => tipset,
            _ => {
                return Err(Error::Other(format!(
                    "expected current head on SHC stream (got {:?})",
                    first_subscriber
                )))
            }
        };
        let message_var = (message.from(), &message.sequence());
        let maybe_message_reciept =
            Self::tipset_executed_message(&*block_store, &tipset, cid, message_var)?;
        if let Some(r) = maybe_message_reciept {
            return Ok((Some(tipset.clone()), Some(r)));
        }

        let mut candidate_tipset: Option<Arc<Tipset>> = None;
        let mut candidate_receipt: Option<MessageReceipt> = None;

        let block_store_for_message = block_store.clone();
        let cid = message
            .cid()
            .map_err(|e| Error::Other(format!("Could not get cid from message {:?}", e)))?;

        let cid_for_task = cid.clone();
        let address_for_task = *message.from();
        let sequence_for_task = message.sequence();
        let height_of_head = tipset.epoch();
        let task = task::spawn(async move {
            let (back_t, back_r) = Self::search_back_for_message(
                block_store_for_message,
                &tipset,
                (&address_for_task, &cid_for_task, &sequence_for_task),
            )?
            .ok_or_else(|| {
                Error::Other("State manager not subscribed to back search wait".to_string())
            })?;
            let back_tuple = (back_t, back_r);
            sender
                .send(())
                .map_err(|e| Error::Other(format!("Could not send to channel {:?}", e)))?;
            Ok::<_, Error>(back_tuple)
        });

        let reverts: Arc<RwLock<HashMap<TipsetKeys, bool>>> = Arc::new(RwLock::new(HashMap::new()));
        let block_revert = reverts.clone();
        let mut futures = FuturesUnordered::new();
        let subscriber_poll = task::spawn(async move {
            while let Some(subscriber) = subscriber
                .clone()
                .ok_or_else(|| {
                    Error::Other("State Manager not subscribed to tipset head changes".to_string())
                })?
                .next()
                .await
            {
                match subscriber {
                    HeadChange::Revert(_tipset) => {
                        if candidate_tipset.is_some() {
                            candidate_tipset = None;
                            candidate_receipt = None;
                        }
                    }
                    HeadChange::Apply(tipset) => {
                        if candidate_tipset
                            .as_ref()
                            .map(|s| s.epoch() >= s.epoch() + tipset.epoch())
                            .unwrap_or_default()
                        {
                            return Ok((candidate_tipset, candidate_receipt));
                        }
                        let poll_receiver = receiver.try_recv().map_err(|e| {
                            Error::Other(format!("Could not receieve from channel {:?}", e))
                        })?;
                        if poll_receiver.is_some() {
                            block_revert
                                .write()
                                .await
                                .insert(tipset.key().to_owned(), true);
                        }

                        let message_var = (message.from(), &message.sequence());
                        let maybe_receipt = Self::tipset_executed_message(
                            &*block_store,
                            &tipset,
                            &cid,
                            message_var,
                        )?;
                        if let Some(receipt) = maybe_receipt {
                            if confidence == 0 {
                                return Ok((Some(tipset), Some(receipt)));
                            }
                            candidate_tipset = Some(tipset);
                            candidate_receipt = Some(receipt)
                        }
                    }
                    _ => (),
                }
            }

            Ok((None, None))
        });

        futures.push(subscriber_poll);
        let search_back_poll = task::spawn(async move {
            let (back_tipset, back_receipt) = task.await?;
            let should_revert = *reverts
                .read()
                .await
                .get(back_tipset.key())
                .unwrap_or(&false);
            let larger_height_of_head = height_of_head >= back_tipset.epoch() + confidence;
            if !should_revert && larger_height_of_head {
                return Ok((Some(Arc::new(back_tipset)), Some(back_receipt)));
            }

            Ok((None, None))
        });
        futures.push(search_back_poll);

        futures.next().await.ok_or_else(|| Error::Other("wait_for_message could not be completed due to failure of subscriber poll or search_back functionality".to_string()))?
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

    /// Return the heaviest tipset's balance from self.db for a given address
    pub fn get_heaviest_balance(&self, addr: &Address) -> Result<BigInt, Error> {
        let ts = get_heaviest_tipset(self.bs.as_ref())
            .map_err(|err| Error::Other(err.to_string()))?
            .ok_or_else(|| Error::Other("could not get bs heaviest ts".to_owned()))?;
        let cid = ts.parent_state();
        self.get_balance(addr, cid)
    }

    /// Return the balance of a given address and state_cid
    pub fn get_balance(&self, addr: &Address, cid: &Cid) -> Result<BigInt, Error> {
        let act = self.get_actor(addr, cid)?;
        let actor = act.ok_or_else(|| "could not find actor".to_owned())?;
        Ok(actor.balance)
    }

    pub fn lookup_id(&self, addr: &Address, ts: &Tipset) -> Result<Option<Address>, Error> {
        let state_tree = StateTree::new_from_root(self.bs.as_ref(), ts.parent_state())?;
        state_tree.lookup_id(addr).map_err(Error::State)
    }

    pub fn market_balance(&mut self, addr: &Address, ts: &Tipset) -> Result<MarketBalance, Error> {
        let market_state: market::State =
            self.load_actor_state(&*STORAGE_MARKET_ACTOR_ADDR, ts.parent_state())?;

        let new_addr = self
            .lookup_id(addr, ts)?
            .ok_or_else(|| Error::State(format!("Failed to resolve address {}", addr)))?;

        let out = MarketBalance {
            escrow: {
                let et = BalanceTable::from_root(self.bs.as_ref(), &market_state.escrow_table)
                    .map_err(|_x| Error::State("Failed to build Escrow Table".to_string()))?;
                et.get(&new_addr).unwrap_or_default()
            },
            locked: {
                let lt = BalanceTable::from_root(self.bs.as_ref(), &market_state.locked_table)
                    .map_err(|_x| Error::State("Failed to build Locked Table".to_string()))?;
                lt.get(&new_addr).unwrap_or_default()
            },
        };

        Ok(out)
    }
}
