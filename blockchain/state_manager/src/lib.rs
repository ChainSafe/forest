// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod errors;

pub use self::errors::*;
use actor::{miner, ActorState};
use address::Address;
use blockstore::BlockStore;
use blockstore::BufferedBlockStore;
use cid::Cid;
use encoding::de::DeserializeOwned;
use forest_blocks::{FullTipset, Tipset};
use interpreter::VM;
use ipld_amt::Amt;
use runtime::DefaultSyscalls;
use state_tree::StateTree;
use std::error::Error as StdError;
use std::sync::Arc;
use vm::SectorSize;

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
    fn load_actor_state<D>(&self, addr: &Address, ts: &Tipset) -> Result<D, Error>
    where
        D: DeserializeOwned,
    {
        let actor = self
            .get_actor(addr, ts)?
            .ok_or_else(|| Error::ActorNotFound(addr.to_string()))?;
        let act: D = self
            .bs
            .get(&actor.state)
            .map_err(|e| Error::State(e.to_string()))?
            .ok_or_else(|| Error::ActorStateNotFound(actor.state.to_string()))?;
        Ok(act)
    }
    /// Returns the epoch at which the miner was slashed at
    pub fn miner_slashed(&self, _addr: &Address, _ts: &Tipset) -> Result<u64, Error> {
        // TODO update to use power actor if needed
        todo!()
    }
    /// Returns the amount of space in each sector committed to the network by this miner
    pub fn miner_sector_size(&self, addr: &Address, ts: &Tipset) -> Result<SectorSize, Error> {
        let act: miner::State = self.load_actor_state(addr, ts)?;
        // * Switch back to retrieving from Cid if/when changed in actors
        // let info: miner::MinerInfo = self.bs.get(&act.info)?.ok_or_else(|| {
        //     Error::State("Could not retrieve miner info from IPLD store".to_owned())
        // })?;
        Ok(act.info.sector_size)
    }
    pub fn get_actor(&self, addr: &Address, ts: &Tipset) -> Result<Option<ActorState>, Error> {
        let state =
            StateTree::new_from_root(self.bs.as_ref(), ts.parent_state()).map_err(Error::State)?;
        state.get_actor(addr).map_err(Error::State)
    }

    pub fn apply_blocks(&self, ts: &FullTipset) -> Result<(Cid, Cid), Box<dyn StdError>> {
        let mut buf_store = BufferedBlockStore::new(self.bs.as_ref());
        // TODO possibly switch out syscalls to be saved at state manager level
        let mut vm = VM::new(ts.parent_state(), &buf_store, ts.epoch(), DefaultSyscalls)?;

        // Apply tipset messages
        let receipts = vm.apply_tip_set_messages(ts).map_err(|e| Error::VM(e))?;

        // Construct receipt root from receipts
        let rect_root = Amt::new_from_slice(self.bs.as_ref(), &receipts)?;

        // Flush changes to blockstore
        let state_root = vm.flush()?;
        // Persist changes connected to root
        buf_store.flush(&state_root)?;

        Ok((state_root, rect_root))
    }
}
