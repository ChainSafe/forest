// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;

pub use self::state::{Reward, State};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use vm::{ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR, METHOD_PLACEHOLDER};

/// Reward actor methods available
#[derive(FromPrimitive)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    MintReward = METHOD_PLACEHOLDER,
    WithdrawReward = METHOD_PLACEHOLDER + 1,
}

impl Method {
    /// from_method_num converts a method number into an Method enum
    fn from_method_num(m: MethodNum) -> Option<Method> {
        FromPrimitive::from_u64(u64::from(m))
    }
}

/// Reward Actor
pub struct Actor;
impl Actor {
    /// Constructor for Reward actor
    fn constructor<RT: Runtime>(_rt: &RT) {
        // TODO
        unimplemented!();
    }
    /// Mints a reward and puts into state reward map
    fn mint_reward<RT: Runtime>(_rt: &RT) {
        // TODO
        unimplemented!();
    }
    /// Withdraw available funds from reward map
    fn withdraw_reward<RT: Runtime>(_rt: &RT) {
        // TODO
        unimplemented!();
    }
}

impl ActorCode for Actor {
    fn invoke_method<RT: Runtime>(&self, rt: &RT, method: MethodNum, _params: &Serialized) {
        match Method::from_method_num(method) {
            // TODO determine parameters for each method on finished spec
            Some(Method::Constructor) => Self::constructor(rt),
            Some(Method::MintReward) => Self::mint_reward(rt),
            Some(Method::WithdrawReward) => Self::withdraw_reward(rt),
            _ => {
                rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method".to_owned());
                unreachable!();
            }
        }
    }
}
