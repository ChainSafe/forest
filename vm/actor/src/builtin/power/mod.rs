// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;

pub use self::state::State;
use crate::{assert_empty_params, empty_return};
use ipld_blockstore::BlockStore;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use vm::{ActorError, ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};

/// Storage power actor methods available
#[derive(FromPrimitive)]
pub enum Method {
    /// Constructor for Storage Power Actor
    Constructor = METHOD_CONSTRUCTOR,
    AddBalance = 2,
    WithdrawBalance = 3,
    CreateMiner = 4,
    DeleteMiner = 5,
    OnSectorProveCommit = 6,
    OnSectorTerminate = 7,
    OnSectorTemporaryFaultEffectiveBegin = 8,
    OnSectorTemporaryFaultEffectiveEnd = 9,
    OnSectorModifyWeightDesc = 10,
    OnMinerWindowedPoStSuccess = 11,
    OnMinerWindowedPoStFailure = 12,
    EnrollCronEvent = 13,
    ReportConsensusFault = 14,
    OnEpochTickEnd = 15,
}

impl Method {
    /// Converts a method number into an Method enum
    fn from_method_num(m: MethodNum) -> Option<Method> {
        FromPrimitive::from_u64(u64::from(m))
    }
}

/// Storage Power Actor
pub struct Actor;
impl Actor {
    /// Constructor for StoragePower actor
    fn constructor<BS, RT>(_rt: &RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO
        todo!();
    }
    // TODO implement other actor methods based on methods available
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
            // TODO handle other methods available
            _ => Err(rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method")),
        }
    }
}
