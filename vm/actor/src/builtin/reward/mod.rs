// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;

pub use self::state::{Reward, State};
use crate::{assert_empty_params, empty_return};
use address::Address;
use ipld_blockstore::BlockStore;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use vm::{ActorError, ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};

/// Reward actor methods available
#[derive(FromPrimitive)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    AwardBlockReward = 2,
    WithdrawReward = 3,
}

impl Method {
    /// Converts a method number into an Method enum
    fn from_method_num(m: MethodNum) -> Option<Method> {
        FromPrimitive::from_u64(u64::from(m))
    }
}

/// Reward Actor
pub struct Actor;
impl Actor {
    /// Constructor for Reward actor
    fn constructor<BS, RT>(_rt: &RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO
        todo!();
    }
    /// Mints a reward and puts into state reward map
    fn award_block_reward<BS, RT>(_rt: &RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO add params type and implement
        todo!();
    }
    /// Withdraw available funds from reward map
    fn withdraw_reward<BS, RT>(_rt: &RT, _miner_in: &Address) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO
        todo!();
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
                assert_empty_params(params);
                Self::constructor(rt)?;
                Ok(empty_return())
            }
            Some(Method::AwardBlockReward) => {
                assert_empty_params(params);
                Self::award_block_reward(rt)?;
                Ok(empty_return())
            }
            Some(Method::WithdrawReward) => {
                Self::withdraw_reward(rt, &params.deserialize().unwrap())?;
                Ok(empty_return())
            }
            _ => Err(rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method")),
        }
    }
}
