// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::error::ServerError;
use crate::rpc::types::ApiTipsetKey;
use crate::rpc::types::*;
use crate::rpc::{ApiPaths, Ctx, Permission, RpcMethod};
use crate::shim::actors::MultisigActorStateLoad as _;
use crate::shim::actors::multisig;
use crate::shim::{address::Address, econ::TokenAmount};
use enumflags2::BitFlags;
use fvm_ipld_blockstore::Blockstore;
use num_bigint::BigInt;

pub enum MsigGetAvailableBalance {}

impl RpcMethod<2> for MsigGetAvailableBalance {
    const NAME: &'static str = "Filecoin.MsigGetAvailableBalance";
    const PARAM_NAMES: [&'static str; 2] = ["address", "tipset_key"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (Address, ApiTipsetKey);
    type Ok = TokenAmount;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (address, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let height = ts.epoch();
        let actor = ctx
            .state_manager
            .get_required_actor(&address, *ts.parent_state())?;
        let actor_balance = TokenAmount::from(&actor.balance);
        let ms = multisig::State::load(ctx.store(), actor.code, actor.state)?;
        let locked_balance = ms.locked_balance(height)?;
        let avail_balance = &actor_balance - locked_balance;
        Ok(avail_balance)
    }
}

pub enum MsigGetPending {}

impl RpcMethod<2> for MsigGetPending {
    const NAME: &'static str = "Filecoin.MsigGetPending";
    const PARAM_NAMES: [&'static str; 2] = ["address", "tipset_key"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (Address, ApiTipsetKey);
    type Ok = Vec<Transaction>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (address, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let ms: multisig::State = ctx
            .state_manager
            .get_actor_state_from_address(&ts, &address)?;
        let txns = ms
            .get_pending_txn(ctx.store())?
            .into_iter()
            .map(|txn| Transaction {
                id: txn.id,
                to: txn.to,
                value: txn.value,
                method: txn.method,
                params: txn.params,
                approved: txn.approved,
            })
            .collect();
        Ok(txns)
    }
}

pub enum MsigGetVested {}
impl RpcMethod<3> for MsigGetVested {
    const NAME: &'static str = "Filecoin.MsigGetVested";
    const PARAM_NAMES: [&'static str; 3] = ["address", "start_tsk", "end_tsk"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (Address, ApiTipsetKey, ApiTipsetKey);
    type Ok = BigInt;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (addr, ApiTipsetKey(start_tsk), ApiTipsetKey(end_tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let start_ts = ctx
            .chain_store()
            .load_required_tipset_or_heaviest(&start_tsk)?;
        let end_ts = ctx
            .chain_store()
            .load_required_tipset_or_heaviest(&end_tsk)?;

        match start_ts.epoch().cmp(&end_ts.epoch()) {
            std::cmp::Ordering::Greater => Err(ServerError::internal_error(
                "start tipset is after end tipset",
                None,
            )),
            std::cmp::Ordering::Equal => Ok(BigInt::from(0)),
            std::cmp::Ordering::Less => {
                let ms: multisig::State = ctx
                    .state_manager
                    .get_actor_state_from_address(&end_ts, &addr)?;
                let start_lb = ms.locked_balance(start_ts.epoch())?;
                let end_lb = ms.locked_balance(end_ts.epoch())?;
                Ok(start_lb.atto() - end_lb.atto())
            }
        }
    }
}

pub enum MsigGetVestingSchedule {}
impl RpcMethod<2> for MsigGetVestingSchedule {
    const NAME: &'static str = "Filecoin.MsigGetVestingSchedule";
    const PARAM_NAMES: [&'static str; 2] = ["address", "tsk"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (Address, ApiTipsetKey);
    type Ok = MsigVesting;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (addr, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let ms: multisig::State = ctx.state_manager.get_actor_state_from_address(&ts, &addr)?;
        Ok(ms.get_vesting_schedule()?)
    }
}
