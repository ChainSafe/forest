// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;

pub use self::state::State;
use address::Address;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use vm::{ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};

/// Account actor methods available
#[derive(FromPrimitive)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    PubkeyAddress = 2,
}

impl Method {
    /// from_method_num converts a method number into a Method enum
    fn from_method_num(m: MethodNum) -> Option<Method> {
        FromPrimitive::from_u64(u64::from(m))
    }
}

/// Account Actor
pub struct Actor;
impl Actor {
    /// Constructor for Account actor
    pub fn constructor<RT: Runtime>(_rt: &RT, _address: Address) {
        // TODO now updated spec
        todo!()
    }

    // Fetches the pubkey-type address from this actor.
    pub fn pubkey_address<RT: Runtime>(_rt: &RT) -> Address {
        // TODO
        todo!()
    }
}

impl ActorCode for Actor {
    fn invoke_method<RT: Runtime>(&self, rt: &RT, method: MethodNum, _params: &Serialized) {
        match Method::from_method_num(method) {
            Some(Method::Constructor) => {
                // TODO deserialize address from params
                Self::constructor(rt, Address::default())
            }
            Some(Method::PubkeyAddress) => {
                // TODO assert that no params and handle return
                let _ = Self::pubkey_address(rt);
            }
            _ => {
                rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method".to_owned());
                unreachable!();
            }
        }
    }
}
