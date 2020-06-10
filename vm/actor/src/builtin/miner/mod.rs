// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;

pub use self::state::{MinerInfo, State};
use ipld_blockstore::BlockStore;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use vm::{ActorError, ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};

/// Storage Miner actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    ControlAddresses = 2,
    ChangeWorkerAddress = 3,
    ChangePeerID = 4,
    SubmitWindowedPoSt = 5,
    PreCommitSector = 6,
    ProveCommitSector = 7,
    ExtendSectorExpiration = 8,
    TerminateSectors = 9,
    DeclareFaults = 10,
    DeclareFaultsRecovered = 11,
    OnDeferredCronEvent = 12,
    CheckSectorProven = 13,
    AddLockedFund = 14,
    ReportConsensusFault = 15,
    WithdrawBalance = 16,
    ConfirmSectorProofsValid = 17,
    ChangeMultiaddrs = 18,
}

/// Miner Actor
pub struct Actor;
// TODO implement functions on finished spec

impl ActorCode for Actor {
    fn invoke_method<BS, RT>(
        &self,
        rt: &mut RT,
        method: MethodNum,
        _params: &Serialized,
    ) -> Result<Serialized, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        match FromPrimitive::from_u64(method) {
            // TODO handle dispatching actor functions
            Some(Method::Constructor) => todo!(),
            _ => {
                // Method number does not match available, abort in runtime
                Err(rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method".to_owned()))
            }
        }
    }
}
