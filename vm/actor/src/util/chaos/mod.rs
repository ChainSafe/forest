// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;
mod types;

use cid::Cid;
use ipld_blockstore::BlockStore;
use num_bigint::bigint_ser::BigIntDe;
use num_bigint::BigInt;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
pub use state::*;
pub use types::*;
use vm::{actor_error, ActorError, ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};

// * Updated to test-vectors commit: a5d1c62f093bd44e487a35211031a5e5a0824936

lazy_static! {
    pub static ref CALLER_VALIDATION_BRANCH_NONE: BigInt = BigInt::from(0);
    pub static ref CALLER_VALIDATION_BRANCH_TWICE: BigInt = BigInt::from(1);
    pub static ref CALLER_VALIDATION_BRANCH_ADDR_NIL_SET: BigInt = BigInt::from(2);
    pub static ref CALLER_VALIDATION_BRANCH_TYPE_NIL_SET: BigInt = BigInt::from(3);
}

/// Chaos actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    CallerValidation = 2,
    CreateActor = 3,
}

/// Chaos Actor
pub struct Actor;

impl Actor {
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
    pub fn caller_validation<BS, RT>(rt: &mut RT, branch: BigInt) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        match branch {
            x if x == *CALLER_VALIDATION_BRANCH_NONE => {}
            x if x == *CALLER_VALIDATION_BRANCH_TWICE => {
                rt.validate_immediate_caller_accept_any()?;
                rt.validate_immediate_caller_accept_any()?;
            }
            x if x == *CALLER_VALIDATION_BRANCH_ADDR_NIL_SET => {
                rt.validate_immediate_caller_is(&[])?;
            }
            x if x == *CALLER_VALIDATION_BRANCH_TYPE_NIL_SET => {
                rt.validate_immediate_caller_type(&[])?;
            }
            _ => panic!("invalid branch passed to CallerValidation"),
        }
        Ok(())
    }

    // CreateActor creates an actor with the supplied CID and Address.
    pub fn create_actor<BS, RT>(rt: &mut RT, arg: CreateActorArgs) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
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
                let BigIntDe(branch) = Serialized::deserialize(&params)?;
                Self::caller_validation(rt, branch)?;
                Ok(Serialized::default())
            }

            Some(Method::CreateActor) => {
                Self::create_actor(rt, Serialized::deserialize(&params)?)?;
                Ok(Serialized::default())
            }
            None => Err(actor_error!(SysErrInvalidMethod; "Invalid method")),
        }
    }
}
