// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;

pub use self::state::State;
use crate::{builtin::singletons::SYSTEM_ACTOR_ADDR, check_empty_params};
use address::{Address, Protocol};
use ipld_blockstore::BlockStore;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use vm::{actor_error, ActorError, ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};

// * Updated to specs-actors commit: 845089a6d2580e46055c24415a6c32ee688e5186 (v3.0.0)

/// Account actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    PubkeyAddress = 2,
}

/// Account Actor
pub struct Actor;
impl Actor {
    /// Constructor for Account actor
    pub fn constructor<BS, RT>(rt: &mut RT, address: Address) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*SYSTEM_ACTOR_ADDR))?;
        match address.protocol() {
            Protocol::Secp256k1 | Protocol::BLS => {}
            protocol => {
                return Err(actor_error!(ErrIllegalArgument;
                    "address must use BLS or SECP protocol, got {}", protocol));
            }
        }
        rt.create(&State { address })?;
        Ok(())
    }

    // Fetches the pubkey-type address from this actor.
    pub fn pubkey_address<BS, RT>(rt: &mut RT) -> Result<Address, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;
        let st: State = rt.state()?;
        Ok(st.address)
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
            Some(Method::PubkeyAddress) => {
                check_empty_params(params)?;
                let addr = Self::pubkey_address(rt)?;
                Ok(Serialized::serialize(addr)?)
            }
            None => Err(actor_error!(SysErrInvalidMethod; "Invalid method")),
        }
    }
}
