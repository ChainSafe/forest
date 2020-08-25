// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;
mod types;

use super::{ACCOUNT_ACTOR_CODE_ID, SYSTEM_ACTOR_ADDR};
use ipld_blockstore::BlockStore;
use num_bigint::BigInt;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
pub use state::*;
pub use types::*;
use vm::{actor_error, ActorError, ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};
// * Updated to specs-actors commit: 4784ddb8e54d53c118e63763e4efbcf0a419da28

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

    // CallerValidation violates VM call validation constraints.
    //
    //  CallerValidationBranchNone performs no validation.
    //  CallerValidationBranchTwice validates twice.
    //  CallerValidationBranchAddrNilSet validates against an empty caller
    //  address set.
    //  CallerValidationBranchTypeNilSet validates against an empty caller type set.
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
                rt.validate_immediate_caller_is(vec![])?;
            }
            x if x == *CALLER_VALIDATION_BRANCH_TYPE_NIL_SET => {
                rt.validate_immediate_caller_type(vec![])?;
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
        let actor_cid = if arg.undef_cid {
            &*ACCOUNT_ACTOR_CODE_ID
        } else {
            &arg.cid
        };
        let actor_address = if arg.undef_address {
            *SYSTEM_ACTOR_ADDR
        } else {
            arg.address
        };
        rt.create_actor(&actor_cid, &actor_address)
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
                Self::caller_validation(rt, Serialized::deserialize(&params)?)?;
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
