pub mod state;
pub mod types;
pub use self::types::*;
pub use self::state::State;
use address::{Address, Protocol};
use ipld_blockstore::BlockStore;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use vm::{ActorError, ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};
use crate::{StoragePower, HAMT_BIT_WIDTH,SYSTEM_ACTOR_ADDR};
use ipld_hamt::Hamt;

/// Account actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    AddVerifier = 2,
    RemoveVerifier =3,
    AddVerifiedClient =4,
    UseBytes =5,
    RestoreBytes =6
}


pub struct Actor;
impl Actor {
    /// Constructor for Registry Actor
    pub fn constructor<BS, RT>(rt: &mut RT,root_key:Address) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*SYSTEM_ACTOR_ADDR))?;
        let empty_root = Hamt::<String, _>::new_with_bit_width(rt.store(), HAMT_BIT_WIDTH)
            .flush()
            .map_err(|e| {
                rt.abort(
                    ExitCode::ErrIllegalState,
                    format!("failed to create storage power state: {}", e),
                )
            })?;
        let st = State::new(empty_root,root_key);
        rt.create(&st)?;
        Ok(())
    }

    pub fn add_verifier<BS, RT>(rt: &mut RT,params:AddVerifierParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let state : State = rt.state()?;;
        rt.validate_immediate_caller_is(std::iter::once(&state.root_key))?;

        rt.transaction::<_, Result<_, String>, _>(|st: &mut State, rt|{
            st.put_verified(rt.store(),params.address,params.allowance)?;
            Ok(())
        })?
        .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e))?;

        Ok(())
        
    }

    pub fn delete_verifier<BS, RT>(rt: &mut RT,params:AddVerifierParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let state : State = rt.state()?;;
        rt.validate_immediate_caller_is(std::iter::once(&state.root_key))?;

        rt.transaction::<_, Result<_, String>, _>(|st: &mut State, rt|{
            st.delete_verifier(rt.store(),params.address)?;
            Ok(())
        })?
        .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e))?;

        Ok(())
        
    }
}