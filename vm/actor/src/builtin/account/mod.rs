// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;

pub use self::state::State;
use crate::check_empty_params;
use address::{Address, Protocol};
use ipld_blockstore::BlockStore;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use vm::{ActorError, ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};

/// Account actor methods available
#[derive(FromPrimitive)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    PubkeyAddress = 2,
}

impl Method {
    /// Converts a method number into a Method enum
    fn from_method_num(m: MethodNum) -> Option<Method> {
        FromPrimitive::from_u64(u64::from(m))
    }
}

/// Account Actor
pub struct Actor;
impl Actor {
    /// Constructor for Account actor
    pub fn constructor<BS, RT>(rt: &RT, address: Address) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&address));
        match address.protocol() {
            Protocol::Secp256k1 | Protocol::BLS => {}
            protocol => {
                return Err(rt.abort(
                    ExitCode::ErrIllegalArgument,
                    format!("address must use BLS or SECP protocol, got {}", protocol),
                ));
            }
        }
        rt.create(&State { address });
        Ok(())
    }

    // Fetches the pubkey-type address from this actor.
    pub fn pubkey_address<BS, RT>(rt: &RT) -> Result<Address, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any();
        let st: State = rt.state();
        Ok(st.address)
    }
}

impl ActorCode for Actor {
    fn invoke_method<BS, RT>(
        &self,
        rt: &RT,
        method: MethodNum,
        params: &Serialized,
    ) -> Result<Serialized, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        match Method::from_method_num(method) {
            Some(Method::Constructor) => {
                Self::constructor(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::PubkeyAddress) => {
                check_empty_params(params)?;
                Self::pubkey_address(rt)?;
                Ok(Serialized::default())
            }
            _ => Err(rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method")),
        }
    }
}
