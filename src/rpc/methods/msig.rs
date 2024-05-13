// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::error::ServerError;
use crate::rpc::types::ApiTipsetKey;
use crate::rpc::types::*;
use crate::rpc::{ApiVersion, Ctx, Permission, RpcMethod};
use crate::shim::actors::multisig::MultisigExt;
use crate::shim::{address::Address, econ::TokenAmount};
use fil_actor_interface::multisig;
use fvm_ipld_blockstore::Blockstore;
use num_bigint::BigInt;

macro_rules! for_each_method {
    ($callback:ident) => {
        $callback!(crate::rpc::msig::MsigGetAvailableBalance);
        $callback!(crate::rpc::msig::MsigGetPending);
        $callback!(crate::rpc::msig::MsigGetVested);
        $callback!(crate::rpc::msig::MsigGetVestingSchedule);
    };
}
pub(crate) use for_each_method;

pub enum MsigGetAvailableBalance {}

impl RpcMethod<2> for MsigGetAvailableBalance {
    const NAME: &'static str = "Filecoin.MsigGetAvailableBalance";
    const PARAM_NAMES: [&'static str; 2] = ["address", "tipset_key"];
    const API_VERSION: ApiVersion = ApiVersion::V0;
    const PERMISSION: Permission = Permission::Read;

    type Params = (Address, ApiTipsetKey);
    type Ok = TokenAmount;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (address, ApiTipsetKey(tsk)): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store.load_required_tipset_or_heaviest(&tsk)?;
        let height = ts.epoch();
        let actor = ctx
            .state_manager
            .get_required_actor(&address, *ts.parent_state())?;
        let actor_balance = TokenAmount::from(&actor.balance);
        let ms = multisig::State::load(ctx.store(), actor.code, actor.state)?;
        let locked_balance = ms.locked_balance(height)?.into();
        let avail_balance = &actor_balance - locked_balance;
        Ok(avail_balance)
    }
}

pub enum MsigGetPending {}

impl RpcMethod<2> for MsigGetPending {
    const NAME: &'static str = "Filecoin.MsigGetPending";
    const PARAM_NAMES: [&'static str; 2] = ["address", "tipset_key"];
    const API_VERSION: ApiVersion = ApiVersion::V0;
    const PERMISSION: Permission = Permission::Read;

    type Params = (Address, ApiTipsetKey);
    type Ok = Vec<Transaction>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (address, ApiTipsetKey(tsk)): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store.load_required_tipset_or_heaviest(&tsk)?;
        let actor = ctx
            .state_manager
            .get_required_actor(&address, *ts.parent_state())?;
        let ms = multisig::State::load(ctx.store(), actor.code, actor.state)?;
        let txns = ms
            .get_pending_txn(ctx.store())?
            .iter()
            .map(|txn| Transaction {
                id: txn.id,
                to: txn.to.into(),
                value: txn.value.clone().into(),
                method: txn.method,
                params: txn.params.clone(),
                approved: txn.approved.iter().map(|item| item.into()).collect(),
            })
            .collect();
        Ok(txns)
    }
}

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
            std::cmp::Ordering::Greater => Err(ServerError::internal_error(
                "start tipset is after end tipset",
                None,
            )),
            std::cmp::Ordering::Equal => Ok(BigInt::from(0)),
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

        Ok(ms.get_vesting_schedule()?)
    }
}
