use std::error::Error;
use state_manager::StateManager;
use blocks::{Tipset,TipsetKeys};
use bitfield::BitField;
use address::Address;
use chain::ChainStore;
use actor::miner::{ChainSectorInfo,SectorOnChainInfo,State,MinerInfo,Deadlines,DeadlineInfo,compute_proving_period_deadline};
struct StateApi<DB> where DB : blockstore::BlockStore
{
    state_manager : StateManager<DB>,

}

impl<DB> StateApi<DB> where DB : blockstore::BlockStore
{
    fn get_network_name(&self) -> Result<String,Box<dyn Error + 'static>>
    {
        let maybe_heaviest_tipset : Option<Tipset> = chain::get_heaviest_tipset(&*self.state_manager.get_block_store()).map_err(|e|{
            let box_err : Box<Error> = e.into();
            box_err
        })?;
        let heaviest_tipset : Tipset = maybe_heaviest_tipset.unwrap();
        self.state_manager.get_network_name(heaviest_tipset.parent_state()).map_err(|e|e.into())
    }

    fn state_miner_sector(&self, address:&Address,filter:&mut BitField,filter_out:bool,key:&TipsetKeys) ->Result<Vec<ChainSectorInfo>,Box<dyn Error + 'static>>
    {
       let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
       let mut filter = Some(filter);
       state_manager::utils::get_miner_sector_set(&self.state_manager,&tipset,address,&mut filter,filter_out).map_err(|e|e.into())
    }

    fn state_miner_proving_set(&self, address:&Address,key:&TipsetKeys) ->Result<Vec<SectorOnChainInfo>,Box<dyn Error + 'static>>
    {
       let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
       let miner_actor_state: State = self.state_manager.load_actor_state(&address, &tipset.parent_state())?;
       state_manager::utils::get_proving_set_raw(&self.state_manager,&miner_actor_state).map_err(|e|e.into())
    }

    fn state_miner_info(&self,actor :&Address,key:&TipsetKeys) ->Result<MinerInfo,Box<dyn Error + 'static>>
    {
        let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
        state_manager::utils::get_miner_info(&self.state_manager,&tipset,actor).map_err(|e|e.into())
    }

    fn state_miner_deadlines(&self,actor :&Address,key:&TipsetKeys) ->Result<Deadlines,Box<dyn Error + 'static>>
    {
        let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
        state_manager::utils::get_miner_deadlines(&self.state_manager,&tipset,actor).map_err(|e|e.into())
    }

    fn state_miner_proving_deadline(&self,actor :&Address,key:&TipsetKeys) ->Result<DeadlineInfo,Box<dyn Error + 'static>>
    {
        let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
        let miner_actor_state: State = self.state_manager.load_actor_state(&actor, &tipset.parent_state())?;
        Ok(compute_proving_period_deadline(miner_actor_state.proving_period_start,tipset.epoch()))
    }

    fn state_miner_faults(&self,actor :&Address,key:&TipsetKeys) ->Result<BitField,Box<dyn Error + 'static>>
    {
        let tipset = ChainStore::new(self.state_manager.get_block_store()).tipset_from_keys(key)?;
        let miner_actor_state: State = self.state_manager.load_actor_state(&actor, &tipset.parent_state())?;
        state_manager::utils::get_miner_faults(&self.state_manager,&tipset,actor).map_err(|e|e.into())
    }
}