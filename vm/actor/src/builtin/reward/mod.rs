// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;

pub use self::state::{Reward, State};
use crate::{assert_empty_params, empty_return};
use address::Address;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use vm::{ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};

/// Reward actor methods available
#[derive(FromPrimitive)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    AwardBlockReward = 2,
    WithdrawReward = 3,
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
        todo!();
    }
    /// Mints a reward and puts into state reward map
    fn award_block_reward<RT: Runtime>(_rt: &RT) {
        // TODO add params type and implement
        todo!();
    }
    /// Withdraw available funds from reward map
    fn withdraw_reward<RT: Runtime>(_rt: &RT, _miner_in: &Address) {
        // TODO
        todo!();
    }
}

impl ActorCode for Actor {
    fn invoke_method<RT: Runtime>(
        &self,
        rt: &RT,
        method: MethodNum,
        params: &Serialized,
    ) -> Serialized {
        match Method::from_method_num(method) {
            Some(Method::Constructor) => {
                assert_empty_params(params);
                Self::constructor(rt);
                empty_return()
            }
            Some(Method::AwardBlockReward) => {
                Self::award_block_reward(rt);
                empty_return()
            }
            Some(Method::WithdrawReward) => {
                Self::withdraw_reward(rt, &params.deserialize().unwrap());
                empty_return()
            }
            _ => {
                rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method".to_owned());
                unreachable!();
            }
        }
    }
}
