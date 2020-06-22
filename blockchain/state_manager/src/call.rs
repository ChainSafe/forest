use crate::errors::*;
use crate::StateManager;
use actor::miner;
use address::{Address, Protocol};
use blockstore::BlockStore;
use cid::Cid;
use fil_types::{RegisteredProof, SectorInfo, SectorSize};
use filecoin_proofs_api::{post::generate_winning_post_sector_challenge, ProverId};
use interpreter::{resolve_to_key_addr, ChainRand, DefaultSyscalls,ApplyRet, VM};
use clock::ChainEpoch;
use blockstore::BufferedBlockStore;
use message::{Message, MessageReceipt,UnsignedMessage};
use vm::ActorError;
use log::warn;
use forest_blocks::Tipset;
use bitfield::BitField;


pub struct InvocResult<Msg> where Msg : Message
{
    msg : Msg,
    msg_rct : MessageReceipt,
    actor_error : String

}

type StateCallResult<T> = Result<InvocResult<T>,Error>;

fn state_call_raw<DB>(state_manager : &StateManager<DB>, msg : &mut UnsignedMessage,bstate : &Cid, rand : & ChainRand, bheight : &ChainEpoch) -> StateCallResult<UnsignedMessage> where DB : BlockStore
{
    let block_store = &*state_manager.get_block_store();
    let mut buf_store = BufferedBlockStore::new(block_store);
    let mut vm = VM::new(
        bstate,
        &buf_store,
        *bheight,
        DefaultSyscalls::new(&buf_store),
        rand,
    )?;

    if msg.gas_limit() == 0
    {
        msg.set_gas_limit(10000000000)
    }

    let actor = state_manager.get_actor(msg.from(),bstate)?;
    let apply_ret = vm.apply_implicit_message(msg);

    if let None = apply_ret.act_error()
    {
        warn!("chain call failed: {:?}",apply_ret.act_error());
    }

    Ok(InvocResult
    {
        msg : msg.clone(),
        msg_rct : apply_ret.msg_receipt().clone(),
        actor_error : apply_ret.act_error().map(|e|e.to_string()).unwrap_or_default()
    })

}

fn state_call<DB>(state_manager : &StateManager<DB>,message : &mut UnsignedMessage,tipset :Option<Tipset>) -> StateCallResult<UnsignedMessage> where DB : BlockStore
{
    let ts = if let Some(t_set) = tipset
    {
        t_set
    }
    else 
    {
        let ts = chain::get_heaviest_tipset(&*state_manager.get_block_store()).map_err(|_|Error::Other("Could not get tipset".to_string()))?;
        let t_set = ts.ok_or_else(||Error::Other("Tipset not available".to_string()))?;
        t_set
    };
    let state = ts.parent_state();
    let chain_rand = ChainRand::new(ts.key().to_owned());
    state_call_raw::<DB>(state_manager,message,state,&chain_rand,&ts.epoch())
}

fn state_replay<'a,DB>(state_manager : &'a StateManager<DB>,ts : &'a Tipset,mcid : &'a Cid) -> Result<(UnsignedMessage,ApplyRet),Error> where DB : BlockStore
{
    let mut outm  : Option<UnsignedMessage> = None;
    let mut outr : Option<ApplyRet> = None;
    let error_message_halt = "halt".to_string();
    let call_back = |cid:Cid,unsigned :UnsignedMessage,apply_ret : ApplyRet|{
        if cid ==mcid.clone()
        {
            outm = Some(unsigned.clone());
            outr = Some(apply_ret.clone());
            return Err("halt".to_string())
        }

        Ok(())

    };
    let result = state_manager.compute_tipset_state(ts.blocks(),Some(call_back));
    
    if let Err(error_message) = result
    {
        if error_message.to_string() == "halt"
        {
            return Err(Error::Other(format!("unexpected error during execution : {:}",error_message)))
        }
    }

    let out_ret = outr.ok_or_else(||Error::Other("given message not found in tipset".to_string()))?;
    let out_mes = outm.ok_or_else(||Error::Other("given message not found in tipset".to_string()))?;
    Ok((out_mes,out_ret))


    
}