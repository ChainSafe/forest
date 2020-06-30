use actor::{
    market::MarketBalance,
    miner::{
        compute_proving_period_deadline, ChainSectorInfo, DeadlineInfo, Deadlines, Fault,
        MinerInfo, SectorOnChainInfo, SectorPreCommitOnChainInfo, State,
    },
    power::Claim,
};
use address::Address;
use async_std::task;
use bitfield::BitField;
use blocks::{Tipset, TipsetKeys};
use chain::ChainStore;
use cid::Cid;
use clock::ChainEpoch;
use fil_types::SectorNumber;
use message::UnsignedMessage;
use num_bigint::BigUint;
use num_traits::identities::Zero;
use state_manager::{call, call::InvocResult, StateManager};
use state_tree::StateTree;
use std::error::Error;

type BoxError = Box<dyn Error + 'static>;
struct StateApi<DB>
where
    DB: blockstore::BlockStore,
{
    state_manager: StateManager<DB>,
}

impl<DB> StateApi<DB>
where
    DB: blockstore::BlockStore,
{
    fn get_network_name(&self) -> Result<String, BoxError> {
        let maybe_heaviest_tipset: Option<Tipset> =
            chain::get_heaviest_tipset(&*self.state_manager.get_block_store()).map_err(|e| {
                let box_err: Box<Error> = e.into();
                box_err
            })?;
        let heaviest_tipset: Tipset = maybe_heaviest_tipset.unwrap();
        self.state_manager
            .get_network_name(heaviest_tipset.parent_state())
            .map_err(|e| e.into())
    }

    fn state_miner_sector(
        &self,
        address: &Address,
        filter: &mut BitField,
        filter_out: bool,
        key: &TipsetKeys,
    ) -> Result<Vec<ChainSectorInfo>, BoxError> {
        let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
        let mut filter = Some(filter);
        state_manager::utils::get_miner_sector_set(
            &self.state_manager,
            &tipset,
            address,
            &mut filter,
            filter_out,
        )
        .map_err(|e| e.into())
    }

    fn state_miner_proving_set(
        &self,
        address: &Address,
        key: &TipsetKeys,
    ) -> Result<Vec<SectorOnChainInfo>, BoxError> {
        let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
        let miner_actor_state: State = self
            .state_manager
            .load_actor_state(&address, &tipset.parent_state())?;
        state_manager::utils::get_proving_set_raw(&self.state_manager, &miner_actor_state)
            .map_err(|e| e.into())
    }

    fn state_miner_info(&self, actor: &Address, key: &TipsetKeys) -> Result<MinerInfo, BoxError> {
        let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
        state_manager::utils::get_miner_info(&self.state_manager, &tipset, actor)
            .map_err(|e| e.into())
    }

    fn state_sector_info(
        &self,
        address: &Address,
        sector_number: &SectorNumber,
        key: &TipsetKeys,
    ) -> Result<Option<SectorOnChainInfo>, BoxError> {
        let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
        state_manager::utils::miner_sector_info(
            &self.state_manager,
            address,
            sector_number,
            &tipset,
        )
        .map_err(|e| e.into())
    }

    fn state_sector_precommit_info(
        &self,
        address: &Address,
        sector_number: &SectorNumber,
        key: &TipsetKeys,
    ) -> Result<Option<SectorPreCommitOnChainInfo>, BoxError> {
        let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
        state_manager::utils::precommit_info(&self.state_manager, address, sector_number, &tipset)
            .map_err(|e| e.into())
    }

    fn state_miner_deadlines(
        &self,
        actor: &Address,
        key: &TipsetKeys,
    ) -> Result<Deadlines, BoxError> {
        let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
        state_manager::utils::get_miner_deadlines(&self.state_manager, &tipset, actor)
            .map_err(|e| e.into())
    }

    fn state_miner_proving_deadline(
        &self,
        actor: &Address,
        key: &TipsetKeys,
    ) -> Result<DeadlineInfo, BoxError> {
        let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
        let miner_actor_state: State = self
            .state_manager
            .load_actor_state(&actor, &tipset.parent_state())?;
        Ok(compute_proving_period_deadline(
            miner_actor_state.proving_period_start,
            tipset.epoch(),
        ))
    }

    fn state_miner_faults(&self, actor: &Address, key: &TipsetKeys) -> Result<BitField, BoxError> {
        let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
        let miner_actor_state: State = self
            .state_manager
            .load_actor_state(&actor, &tipset.parent_state())?;
        state_manager::utils::get_miner_faults(&self.state_manager, &tipset, actor)
            .map_err(|e| e.into())
    }

    fn state_all_miner_faults(
        &self,
        look_back: ChainEpoch,
        end_tsk: &TipsetKeys,
    ) -> Result<Vec<Fault>, BoxError> {
        let tipset =
            ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(end_tsk)?;
        let cut_off = tipset.epoch() - look_back;
        let miners = state_manager::utils::list_all_actors(&self.state_manager, &tipset)?;
        let mut all_faults = Vec::new();
        miners
            .iter()
            .map(|m| {
                let miner_actor_state: State = self
                    .state_manager
                    .load_actor_state(&m, &tipset.parent_state())
                    .map_err(|e| e.to_string())?;
                let block_store = &*self.state_manager.get_block_store();
                miner_actor_state.for_each_fault_epoch(
                    block_store,
                    |fault_start: u64, _| -> Result<(), String> {
                        if fault_start >= cut_off {
                            all_faults.push(Fault {
                                miner: *m,
                                fault: fault_start,
                            })
                        }
                        Ok(())
                    },
                )
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(all_faults)
    }

    fn state_miner_recoveries(
        &self,
        actor: &Address,
        key: &TipsetKeys,
    ) -> Result<BitField, BoxError> {
        let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
        let miner_actor_state: State = self
            .state_manager
            .load_actor_state(&actor, &tipset.parent_state())?;
        state_manager::utils::get_miner_recoveries(&self.state_manager, &tipset, actor)
            .map_err(|e| e.into())
    }

    fn state_miner_power(
        &self,
        actor: &Address,
        key: &TipsetKeys,
    ) -> Result<(Option<Claim>, Claim), BoxError> {
        let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
        let miner_actor_state: State = self
            .state_manager
            .load_actor_state(&actor, &tipset.parent_state())?;
        state_manager::utils::get_power(&self.state_manager, &tipset, Some(actor))
            .map_err(|e| e.into())
    }

    fn state_page_collateral(&self, _: &TipsetKeys) -> Result<BigUint, BoxError> {
        Ok(BigUint::zero())
    }

    fn state_call(
        &self,
        message: &mut UnsignedMessage,
        key: &TipsetKeys,
    ) -> Result<InvocResult<UnsignedMessage>, BoxError> {
        let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
        call::state_call(&self.state_manager, message, Some(tipset)).map_err(|e| e.into())
    }

    fn state_reply(
        &self,
        key: &TipsetKeys,
        cid: &Cid,
    ) -> Result<InvocResult<UnsignedMessage>, BoxError> {
        let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
        let (msg, ret) = call::state_replay(&self.state_manager, &tipset, cid)?;

        Ok(InvocResult {
            msg,
            msg_rct: ret.msg_receipt().clone(),
            actor_error: ret.act_error().map(|e| e.to_string()),
        })
    }

    fn state_for_ts(&self, maybe_tipset: Option<Tipset>) -> Result<StateTree<DB>, BoxError> {
        let block_store = self.state_manager.get_block_store_ref();
        let tipset = if let None = maybe_tipset {
            chain::get_heaviest_tipset(block_store)?
        } else {
            maybe_tipset
        };

        let (st, _) = task::block_on(self.state_manager.tipset_state(&tipset.unwrap()))?;
        let state_tree = StateTree::new_from_root(block_store, &st)?;
        Ok(state_tree)
    }

    fn state_get_actor(
        &self,
        actor: &Address,
        key: &TipsetKeys,
    ) -> Result<Option<actor::ActorState>, BoxError> {
        let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
        let state = self.state_for_ts(Some(tipset))?;
        state.get_actor(actor).map_err(|e| e.into())
    }

    fn state_account_key(&self, actor: &Address, key: &TipsetKeys) -> Result<Address, BoxError> {
        let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
        let state = self.state_for_ts(Some(tipset))?;
        let address = interpreter::resolve_to_key_addr(
            &state,
            self.state_manager.get_block_store_ref(),
            actor,
        )?;
        Ok(address)
    }

    fn state_lookup_id(&self, address: &Address, key: &TipsetKeys) -> Result<Address, BoxError> {
        let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
        let state = self.state_for_ts(Some(tipset))?;
        state.lookup_id(address).map_err(|e| e.into())
    }

    fn state_list_actors(&self, key: &TipsetKeys) -> Result<Vec<Address>, BoxError> {
        let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
        state_manager::utils::list_all_actors(&self.state_manager, &tipset).map_err(|e| e.into())
    }

    fn state_market_balance(
        &self,
        address: &Address,
        key: &TipsetKeys,
    ) -> Result<MarketBalance, BoxError> {
        let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
        self.state_manager
            .market_balance(address, &tipset)
            .map_err(|e| e.into())
    }
}
