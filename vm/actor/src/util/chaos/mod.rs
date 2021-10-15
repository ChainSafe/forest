// Copyright 2019-2022 ChainSafe Systems
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
const CALLER_VALIDATION_BRANCH_IS_ADDRESS: i64 = 2;
const CALLER_VALIDATION_BRANCH_IS_TYPE: i64 = 3;

// Mutate State Branch Methods
const MUTATE_IN_TRANSACTION: i64 = 0;

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
    AbortWith = 8,
    InspectRuntime = 9,
}

/// Chaos Actor
pub struct Actor;

impl Actor {
    pub fn send<BS, RT>(rt: &mut RT, arg: SendArgs) -> Result<SendReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;

        let result = rt.send(arg.to, arg.method, arg.params, arg.value);
        if let Err(e) = result {
            Ok(SendReturn {
                return_value: Serialized::default(),
                code: e.exit_code(),
            })
        } else {
            Ok(SendReturn {
                return_value: result.unwrap(),
                code: ExitCode::Ok,
            })
        }
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
    ///  CALLER_VALIDATION_BRANCH_IS_ADDRESS validates against an empty caller
    ///  address set.
    ///  CALLER_VALIDATION_BRANCH_IS_TYPE validates against an empty caller type set.
    pub fn caller_validation<BS, RT>(
        rt: &mut RT,
        args: CallerValidationArgs,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        match args.branch {
            x if x == CALLER_VALIDATION_BRANCH_NONE => {}
            x if x == CALLER_VALIDATION_BRANCH_TWICE => {
                rt.validate_immediate_caller_accept_any()?;
                rt.validate_immediate_caller_accept_any()?;
            }
            x if x == CALLER_VALIDATION_BRANCH_IS_ADDRESS => {
                rt.validate_immediate_caller_is(&args.addrs)?;
            }
            x if x == CALLER_VALIDATION_BRANCH_IS_TYPE => {
                rt.validate_immediate_caller_type(&args.types)?;
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

            _ => Err(actor_error!(ErrIllegalArgument; "Invalid mutate state command given" )),
        }
    }

    pub fn abort_with(arg: AbortWithArgs) -> Result<(), ActorError> {
        if arg.uncontrolled {
            panic!("Uncontrolled abort/error");
        }
        Err(ActorError::new(arg.code, arg.message))
    }

    pub fn inspect_runtime<BS, RT>(rt: &mut RT) -> Result<InspectRuntimeReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;
        Ok(InspectRuntimeReturn {
            caller: *rt.message().caller(),
            receiver: *rt.message().receiver(),
            value_received: rt.message().value_received().clone(),
            curr_epoch: rt.curr_epoch(),
            current_balance: rt.current_balance()?,
            state: rt.state()?,
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
                Self::constructor(rt);
                Ok(Serialized::default())
            }
            Some(Method::CallerValidation) => {
                Self::caller_validation(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }

            Some(Method::CreateActor) => {
                Self::create_actor(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::ResolveAddress) => {
                let res = Self::resolve_address(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::serialize(res)?)
            }

            Some(Method::Send) => {
                let res: SendReturn = Self::send(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::serialize(res)?)
            }

            Some(Method::DeleteActor) => {
                Self::delete_actor(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }

            Some(Method::MutateState) => {
                Self::mutate_state(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }

            Some(Method::AbortWith) => {
                Self::abort_with(rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }

            Some(Method::InspectRuntime) => {
                let inspect = Self::inspect_runtime(rt)?;
                Ok(Serialized::serialize(inspect)?)
            }

            None => Err(actor_error!(SysErrInvalidMethod; "Invalid method")),
        }
    }
}
