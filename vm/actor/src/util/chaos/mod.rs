// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;
mod types;

use address::Address;
use cid::Cid;
use ipld_blockstore::BlockStore;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
pub use state::*;
pub use types::*;
use vm::{actor_error, ActorError, ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};

// * Updated to test-vectors commit: 907892394dd83fe1f4bf1a82146bbbcc58963148

// Caller Validation methods
const CALLER_VALIDATION_BRANCH_NONE: i64 = 0;
const CALLER_VALIDATION_BRANCH_TWICE: i64 = 1;
const CALLER_VALIDATION_BRANCH_ADDR_NIL_SET: i64 = 2;
const CALLER_VALIDATION_BRANCH_TYPE_NIL_SET: i64 = 3;

// Mutate State Branch Methods
const MUTATE_IN_TRANSACTION: i64 = 0;
const MUTATE_READ_ONLY: i64 = 1;
const MUTATE_AFTER_TRANSACTION: i64 = 2;

/// Chaos actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    CallerValidation = 2,
    CreateActor = 3,
    ResolveAddress = 4,
    DeleteActor = 5,
    Send = 6,
    MutateState = 7,
}

/// Chaos Actor
pub struct Actor;

impl Actor {
    pub fn send<BS, RT>(rt: &mut RT, arg: SendArgs) -> Result<SendReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        if Serialized::default() != rt.send(arg.to, arg.method, arg.params, arg.value)? {
            return Err(actor_error!(ErrIllegalState; "Failed to unmarshal"));
        }

        Ok(SendReturn {
            return_value: Serialized::default(),
            code: ExitCode::Ok,
        })
    }

    /// Constructor for Account actor
    pub fn constructor<BS, RT>(_rt: &mut RT)
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        panic!("Constructor should not be called");
    }

    /// CallerValidation violates VM call validation constraints.
    ///
    ///  CALLER_VALIDATION_BRANCH_NONE performs no validation.
    ///  CALLER_VALIDATION_BRANCH_TWICE validates twice.
    ///  CALLER_VALIDATION_BRANCH_ADDR_NIL_SET validates against an empty caller
    ///  address set.
    ///  CALLER_VALIDATION_BRANCH_TYPE_NIL_SET validates against an empty caller type set.
    pub fn caller_validation<BS, RT>(rt: &mut RT, branch: i64) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        match branch {
            x if x == CALLER_VALIDATION_BRANCH_NONE => {}
            x if x == CALLER_VALIDATION_BRANCH_TWICE => {
                rt.validate_immediate_caller_accept_any()?;
                rt.validate_immediate_caller_accept_any()?;
            }
            x if x == CALLER_VALIDATION_BRANCH_ADDR_NIL_SET => {
                rt.validate_immediate_caller_is(&[])?;
            }
            x if x == CALLER_VALIDATION_BRANCH_TYPE_NIL_SET => {
                rt.validate_immediate_caller_type(&[])?;
            }
            _ => panic!("invalid branch passed to CallerValidation"),
        }
        Ok(())
    }

    // Creates an actor with the supplied CID and Address.
    pub fn create_actor<BS, RT>(rt: &mut RT, arg: CreateActorArgs) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;
        // TODO Temporarily fine to use default as Undefined Cid, but may need to change in the future
        let actor_cid = if arg.undef_cid {
            Cid::default()
        } else {
            arg.cid
        };

        // TODO This may need to change to match address.Undef in Spec-actors
        let actor_address = arg.address;

        rt.create_actor(actor_cid, &actor_address)
    }

    /// Resolves address, and returns the resolved address (defaulting to 0 ID) and success boolean.
    pub fn resolve_address<BS, RT>(
        rt: &mut RT,
        args: Address,
    ) -> Result<ResolveAddressResponse, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;
        let resolved = rt.resolve_address(&args)?;
        Ok(ResolveAddressResponse {
            address: resolved.unwrap_or_else(|| Address::new_id(0)),
            success: resolved.is_some(),
        })
    }

    pub fn delete_actor<BS, RT>(rt: &mut RT, beneficiary: Address) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;
        rt.delete_actor(&beneficiary)
    }

    pub fn mutate_state<BS, RT>(rt: &mut RT, arg: MutateStateArgs) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;

        match arg.branch {
            x if x == MUTATE_IN_TRANSACTION => rt.transaction(|s: &mut State, _| {
                s.value = arg.value;
                Ok(())
            }),
            x if x == MUTATE_READ_ONLY => {
                // Impossible to reach this step becuase its Rust, so just return
                Err(actor_error!(ErrForbidden; "Can not modify read only state" ))
            }
            x if x == MUTATE_AFTER_TRANSACTION => rt.transaction(|s: &mut State, _| {
                s.value = arg.value + "-in";
                Ok(())
            }),

            _ => Err(actor_error!(ErrIllegalArgument; "Invalid mutate state command given" )),
        }
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
                Self::constructor(rt);
                Ok(Serialized::default())
            }
            Some(Method::CallerValidation) => {
                let branch = Serialized::deserialize(&params)?;
                Self::caller_validation(rt, branch)?;
                Ok(Serialized::default())
            }

            Some(Method::CreateActor) => {
                Self::create_actor(rt, Serialized::deserialize(&params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::ResolveAddress) => {
                let res = Self::resolve_address(rt, params.deserialize()?)?;
                Ok(Serialized::serialize(res)?)
            }

            Some(Method::Send) => {
                let res: SendReturn = Self::send(rt, params.deserialize()?)?;
                Ok(Serialized::serialize(res)?)
            }

            Some(Method::DeleteActor) => {
                Self::delete_actor(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }

            Some(Method::MutateState) => {
                Self::mutate_state(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }

            None => Err(actor_error!(SysErrInvalidMethod; "Invalid method")),
        }
    }
}
