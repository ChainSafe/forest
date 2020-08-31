// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;
mod types;

pub use self::state::State;
pub use self::types::*;
use crate::{
    make_map, MINER_ACTOR_CODE_ID, MULTISIG_ACTOR_CODE_ID, PAYCH_ACTOR_CODE_ID,
    POWER_ACTOR_CODE_ID, SYSTEM_ACTOR_ADDR,
};
use address::Address;
use cid::Cid;
use ipld_blockstore::BlockStore;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use vm::{actor_error, ActorError, ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};

// * Updated to specs-actors commit: f4024efad09a66e32bfeef10a2845b2b35325297 (v0.9.3)

/// Init actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    Exec = 2,
}

/// Init actor
pub struct Actor;
impl Actor {
    /// Init actor constructor
    pub fn constructor<BS, RT>(rt: &mut RT, params: ConstructorParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let sys_ref: &Address = &SYSTEM_ACTOR_ADDR;
        rt.validate_immediate_caller_is(std::iter::once(sys_ref))?;
        let mut empty_map = make_map(rt.store());
        let root = empty_map
            .flush()
            .map_err(|err| actor_error!(ErrIllegalState; "failed to construct state: {}", err))?;

        rt.create(&State::new(root, params.network_name))?;

        Ok(())
    }

    /// Exec init actor
    pub fn exec<BS, RT>(rt: &mut RT, params: ExecParams) -> Result<ExecReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;
        let caller_code = rt
            .get_actor_code_cid(rt.message().caller())?
            .ok_or_else(|| actor_error!(fatal("No code for actor at {}", rt.message().caller())))?;
        if !can_exec(&caller_code, &params.code_cid) {
            return Err(actor_error!(ErrForbidden;
                    "called type {} cannot exec actor type {}",
                    &caller_code, &params.code_cid
            ));
        }

        // Compute a re-org-stable address.
        // This address exists for use by messages coming from outside the system, in order to
        // stably address the newly created actor even if a chain re-org causes it to end up with
        // a different ID.
        let robust_address = rt.new_actor_address()?;

        // Allocate an ID for this actor.
        // Store mapping of pubkey or actor address to actor ID
        let id_address: Address = rt.transaction(|s: &mut State, rt| {
            s.map_address_to_new_id(rt.store(), &robust_address)
                .map_err(|e| actor_error!(ErrIllegalState; "failed to allocate ID address: {}", e))
        })??;

        // Create an empty actor
        rt.create_actor(params.code_cid, &id_address)?;

        // Invoke constructor
        rt.send(
            id_address,
            METHOD_CONSTRUCTOR,
            params.constructor_params,
            rt.message().value_received().clone(),
        )
        .map_err(|err| err.wrap("constructor failed"))?;

        Ok(ExecReturn {
            id_address,
            robust_address,
        })
    }
}

impl ActorCode for Actor {
    fn invoke_method<BS, RT>(
        &self,
        rt: &mut RT,
        method: MethodNum,
        params: &Serialized,
    ) -> Result<Serialized, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        match FromPrimitive::from_u64(method) {
            Some(Method::Constructor) => {
                Self::constructor(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::Exec) => {
                let res = Self::exec(rt, params.deserialize()?)?;
                Ok(Serialized::serialize(res)?)
            }
            None => Err(actor_error!(SysErrInvalidMethod; "Invalid method")),
        }
    }
}

fn can_exec(caller: &Cid, exec: &Cid) -> bool {
    (exec == &*MINER_ACTOR_CODE_ID && caller == &*POWER_ACTOR_CODE_ID)
        || exec == &*MULTISIG_ACTOR_CODE_ID
        || exec == &*PAYCH_ACTOR_CODE_ID
}
