// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::error::ServerError;
use crate::rpc::types::ApiTipsetKey;
use crate::rpc::types::MsigVesting;
use crate::rpc::{ApiVersion, Ctx, Permission, RpcMethod};
use crate::shim::{address::Address, econ::TokenAmount};
use fil_actor_interface::multisig;
use fvm_ipld_blockstore::Blockstore;
use num_bigint::BigInt;

macro_rules! for_each_method {
    ($callback:ident) => {
        $callback!(crate::rpc::msig::MsigGetVested);
        $callback!(crate::rpc::msig::MsigGetVestingSchedule);
    };
}
pub(crate) use for_each_method;

pub enum MsigGetVested {}
impl RpcMethod<3> for MsigGetVested {
    const NAME: &'static str = "Filecoin.MsigGetVested";
    const PARAM_NAMES: [&'static str; 3] = ["address", "start_tsk", "end_tsk"];
    const API_VERSION: ApiVersion = ApiVersion::V0;
    const PERMISSION: Permission = Permission::Read;

    type Params = (Address, ApiTipsetKey, ApiTipsetKey);
    type Ok = BigInt;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (addr, ApiTipsetKey(start_tsk), ApiTipsetKey(end_tsk)): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let start_ts = ctx
            .chain_store
            .load_required_tipset_or_heaviest(&start_tsk)?;
        let end_ts = ctx.chain_store.load_required_tipset_or_heaviest(&end_tsk)?;

        match start_ts.epoch().cmp(&end_ts.epoch()) {
            std::cmp::Ordering::Greater => {
                return Err(ServerError::internal_error(
                    "start tipset is after end tipset",
                    None,
                ));
            }
            std::cmp::Ordering::Equal => {
                return Ok(BigInt::from(0));
            }
            std::cmp::Ordering::Less => {
                let msig_actor = ctx
                    .state_manager
                    .get_required_actor(&addr, *end_ts.parent_state())?;
                let ms = multisig::State::load(ctx.store(), msig_actor.code, msig_actor.state)?;
                let start_lb: TokenAmount = ms.locked_balance(start_ts.epoch())?.into();
                let end_lb: TokenAmount = ms.locked_balance(end_ts.epoch())?.into();
                Ok(start_lb.atto() - end_lb.atto())
            }
        }
    }
}

pub enum MsigGetVestingSchedule {}
impl RpcMethod<2> for MsigGetVestingSchedule {
    const NAME: &'static str = "Filecoin.MsigGetVestingSchedule";
    const PARAM_NAMES: [&'static str; 2] = ["address", "tsk"];
    const API_VERSION: ApiVersion = ApiVersion::V0;
    const PERMISSION: Permission = Permission::Read;

    type Params = (Address, ApiTipsetKey);
    type Ok = MsigVesting;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (addr, ApiTipsetKey(tsk)): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store.load_required_tipset_or_heaviest(&tsk)?;

        let msig_actor = ctx
            .state_manager
            .get_required_actor(&addr, *ts.parent_state())?;
        let ms = multisig::State::load(ctx.store(), msig_actor.code, msig_actor.state)?;

        let msig_vesting = match &ms {
            multisig::State::V8(st) => MsigVesting {
                initial_balance: st.initial_balance.atto().clone(),
                start_epoch: st.start_epoch,
                unlock_duration: st.unlock_duration,
            },
            multisig::State::V9(st) => MsigVesting {
                initial_balance: st.initial_balance.atto().clone(),
                start_epoch: st.start_epoch,
                unlock_duration: st.unlock_duration,
            },
            multisig::State::V10(st) => MsigVesting {
                initial_balance: st.initial_balance.atto().clone(),
                start_epoch: st.start_epoch,
                unlock_duration: st.unlock_duration,
            },
            multisig::State::V11(st) => MsigVesting {
                initial_balance: st.initial_balance.atto().clone(),
                start_epoch: st.start_epoch,
                unlock_duration: st.unlock_duration,
            },
            multisig::State::V12(st) => MsigVesting {
                initial_balance: st.initial_balance.atto().clone(),
                start_epoch: st.start_epoch,
                unlock_duration: st.unlock_duration,
            },
            multisig::State::V13(st) => MsigVesting {
                initial_balance: st.initial_balance.atto().clone(),
                start_epoch: st.start_epoch,
                unlock_duration: st.unlock_duration,
            },
        };

        Ok(msig_vesting)
    }
}
