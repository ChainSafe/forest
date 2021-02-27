// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;
mod types;

pub use self::state::State;
pub use self::types::*;
use crate::{
    ActorDowncast, MINER_ACTOR_CODE_ID, MULTISIG_ACTOR_CODE_ID, PAYCH_ACTOR_CODE_ID,
    POWER_ACTOR_CODE_ID, SYSTEM_ACTOR_ADDR,
};
use address::Address;
use cid::Cid;
use ipld_blockstore::BlockStore;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use vm::{actor_error, ActorError, ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};

// * Updated to specs-actors commit: 999e57a151cc7ada020ca2844b651499ab8c0dec (v3.0.1)

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
        let state = State::new(rt.store(), params.network_name).map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalState,
                "failed to construct init actor state",
            )
        })?;

        rt.create(&state)?;

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
            .ok_or_else(|| {
                actor_error!(
                    ErrIllegalState,
                    "no code for caller as {}",
                    rt.message().caller()
                )
            })?;
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
                .map_err(|e| {
                    e.downcast_default(ExitCode::ErrIllegalState, "failed to allocate ID address")
                })
        })?;

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
                Self::constructor(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::Exec) => {
                let res = Self::exec(rt, rt.deserialize_params(params)?)?;
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
