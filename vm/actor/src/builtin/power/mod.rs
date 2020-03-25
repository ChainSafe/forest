// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;
mod types;

pub use self::state::{Claim, CronEvent, State};
pub use self::types::*;
use crate::check_empty_params;
use ipld_blockstore::BlockStore;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use vm::{ActorError, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR};

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
    pub fn constructor<BS, RT>(_rt: &RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO
        todo!();
    }
    pub fn add_balance<BS, RT>(_rt: &RT, _params: AddBalanceParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO
        todo!();
    }
    pub fn withdraw_balance<BS, RT>(
        _rt: &RT,
        _params: WithdrawBalanceParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO
        todo!();
    }
    pub fn create_miner<BS, RT>(
        _rt: &RT,
        _params: CreateMinerParams,
    ) -> Result<CreateMinerReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO
        todo!();
    }
    pub fn delete_miner<BS, RT>(_rt: &RT, _params: DeleteMinerParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO
        todo!();
    }
    pub fn on_sector_prove_commit<BS, RT>(
        _rt: &RT,
        _params: OnSectorProveCommitParams,
    ) -> Result<TokenAmount, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO
        todo!();
    }
    pub fn on_sector_terminate<BS, RT>(
        _rt: &RT,
        _params: OnSectorTerminateParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO
        todo!();
    }
    pub fn on_sector_temporary_fault_effective_begin<BS, RT>(
        _rt: &RT,
        _params: OnSectorTemporaryFaultEffectiveBeginParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO
        todo!();
    }
    pub fn on_sector_temporary_fault_effective_end<BS, RT>(
        _rt: &RT,
        _params: OnSectorTemporaryFaultEffectiveEndParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO
        todo!();
    }
    pub fn on_sector_modify_weight_desc<BS, RT>(
        _rt: &RT,
        _params: OnSectorModifyWeightDescParams,
    ) -> Result<TokenAmount, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO
        todo!();
    }
    pub fn on_miner_windowed_post_success<BS, RT>(_rt: &RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO
        todo!();
    }
    pub fn on_miner_windowed_post_failure<BS, RT>(
        _rt: &RT,
        _params: OnMinerWindowedPoStFailureParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO
        todo!();
    }
    pub fn enroll_cron_event<BS, RT>(
        _rt: &RT,
        _params: EnrollCronEventParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO
        todo!();
    }
    pub fn report_consensus_fault<BS, RT>(
        _rt: &RT,
        _params: ReportConsensusFaultParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO
        todo!();
    }
    pub fn on_epoch_tick_end<BS, RT>(_rt: &RT) -> Result<(), ActorError>
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
                check_empty_params(params)?;
                Self::constructor(rt)?;
                Ok(Serialized::default())
            }
            Some(Method::AddBalance) => {
                Self::add_balance(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::WithdrawBalance) => {
                Self::withdraw_balance(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::CreateMiner) => {
                let res = Self::create_miner(rt, params.deserialize()?)?;
                Ok(Serialized::serialize(res)?)
            }
            Some(Method::DeleteMiner) => {
                Self::delete_miner(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::OnSectorProveCommit) => {
                let res = Self::on_sector_prove_commit(rt, params.deserialize()?)?;
                Ok(Serialized::serialize(res)?)
            }
            Some(Method::OnSectorTerminate) => {
                Self::on_sector_terminate(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::OnSectorTemporaryFaultEffectiveBegin) => {
                Self::on_sector_temporary_fault_effective_begin(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::OnSectorTemporaryFaultEffectiveEnd) => {
                Self::on_sector_temporary_fault_effective_end(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::OnSectorModifyWeightDesc) => {
                let res = Self::on_sector_modify_weight_desc(rt, params.deserialize()?)?;
                Ok(Serialized::serialize(res)?)
            }
            Some(Method::OnMinerWindowedPoStSuccess) => {
                check_empty_params(params)?;
                Self::on_miner_windowed_post_success(rt)?;
                Ok(Serialized::default())
            }
            Some(Method::OnMinerWindowedPoStFailure) => {
                Self::on_miner_windowed_post_failure(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::EnrollCronEvent) => {
                Self::enroll_cron_event(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::ReportConsensusFault) => {
                Self::report_consensus_fault(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::OnEpochTickEnd) => {
                check_empty_params(params)?;
                Self::on_epoch_tick_end(rt)?;
                Ok(Serialized::default())
            }
            _ => Err(rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method")),
        }
    }
}
