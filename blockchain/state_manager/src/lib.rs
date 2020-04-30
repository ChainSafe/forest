// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod errors;

pub use self::errors::*;
use actor::{miner, power, ActorState, STORAGE_POWER_ACTOR_ADDR};
use address::{Address, Protocol};
use blockstore::BlockStore;
use blockstore::BufferedBlockStore;
use cid::Cid;
use encoding::de::DeserializeOwned;
use forest_blocks::FullTipset;
use interpreter::{resolve_to_key_addr, DefaultSyscalls, VM};
use ipld_amt::Amt;
use num_bigint::BigUint;
use state_tree::StateTree;
use std::error::Error as StdError;
use std::sync::Arc;

/// Intermediary for retrieving state objects and updating actor states
pub struct StateManager<DB> {
    bs: Arc<DB>,
}

impl<DB> StateManager<DB>
where
    DB: BlockStore,
{
    /// constructor
    pub fn new(bs: Arc<DB>) -> Self {
        Self { bs }
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
            Ok((claim.power, ps.total_network_power))
        } else {
            Err(Error::State(
                "Failed to retrieve claimed power from actor state".to_owned(),
            ))
        }
    }

    /// Performs the state transition for the tipset and applies all unique messages in all blocks.
    /// This function returns the state root and receipt root of the transition.
    pub fn apply_blocks(&self, ts: &FullTipset) -> Result<(Cid, Cid), Box<dyn StdError>> {
        let mut buf_store = BufferedBlockStore::new(self.bs.as_ref());
        // TODO possibly switch out syscalls to be saved at state manager level
        let mut vm = VM::new(
            ts.parent_state(),
            &buf_store,
            ts.epoch(),
            DefaultSyscalls::new(&buf_store),
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

    /// Returns a bls public key from provided address
    pub fn get_bls_public_key(&self, addr: &Address, state_cid: &Cid) -> Result<Vec<u8>, Error> {
        let state = StateTree::new_from_root(self.bs.as_ref(), state_cid).map_err(Error::State)?;
        let kaddr = resolve_to_key_addr(&state, self.bs.as_ref(), addr)
            .map_err(|e| format!("Failed to resolve key address, error: {}", e))?;
        if kaddr.protocol() != Protocol::BLS {
            return Err("Address must be BLS address to load bls public key"
                .to_owned()
                .into());
        }
        Ok(kaddr.payload_bytes())
    }
}
