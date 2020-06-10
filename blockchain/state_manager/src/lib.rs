// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod errors;

pub use self::errors::*;
use actor::{init, miner, power, ActorState, INIT_ACTOR_ADDR, STORAGE_POWER_ACTOR_ADDR};
use address::{Address, BLSPublicKey, Payload, BLS_PUB_LEN};
use async_log::span;
use async_std::sync::RwLock;
use blockstore::BlockStore;
use blockstore::BufferedBlockStore;
use chain::{block_messages, ChainStore};
use cid::Cid;
use encoding::de::DeserializeOwned;
use forest_blocks::{Block, BlockHeader, FullTipset, Tipset, TipsetKeys};
use interpreter::{resolve_to_key_addr, ChainRand, DefaultSyscalls, VM};
use ipld_amt::Amt;
use log::trace;
use num_bigint::BigUint;
use state_tree::StateTree;
use std::collections::HashMap;
use std::error::Error as StdError;
use std::sync::Arc;

/// Intermediary for retrieving state objects and updating actor states
pub type CidPair = (Cid, Cid);

pub struct StateManager<DB> {
    bs: Arc<DB>,
    cache: RwLock<HashMap<TipsetKeys, CidPair>>,
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
        }
    }
    /// Loads actor state from IPLD Store
    fn load_actor_state<D>(&self, addr: &Address, state_cid: &Cid) -> Result<D, Error>
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

    /// Returns the network name from the init actor state
    pub fn get_network_name(&self, st: &Cid) -> Result<String, Error> {
        let state: init::State = self.load_actor_state(&*INIT_ACTOR_ADDR, st)?;
        Ok(state.network_name)
    }
    /// Returns true if miner has been slashed or is considered invalid
    pub fn is_miner_slashed(&self, addr: &Address, state_cid: &Cid) -> Result<bool, Error> {
        let ms: miner::State = self.load_actor_state(addr, state_cid)?;
        if ms.post_state.has_failed_post() {
            return Ok(true);
        }

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
        let receipts = vm.apply_tip_set_messages(ts)?;

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
            let cid_pair = self.compute_tipset_state(&block_headers)?;
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
            self.apply_blocks(&full_tipset, &chain_rand)
        })
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
