// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::error::ServerError;
use crate::rpc::types::ApiTipsetKey;
use crate::rpc::{ApiVersion, Ctx, Permission, RpcMethod};
use crate::shim::{address::Address, econ::TokenAmount};
use fil_actor_interface::multisig;
use fvm_ipld_blockstore::Blockstore;
use num::Zero;

macro_rules! for_each_method {
    ($callback:ident) => {
        $callback!(crate::rpc::msig::MsigGetVested);
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
    type Ok = TokenAmount;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (addr, ApiTipsetKey(start_tsk), ApiTipsetKey(end_tsk)): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let start_ts = ctx
            .chain_store
            .load_required_tipset_or_heaviest(&start_tsk)?;
        let end_ts = ctx.chain_store.load_required_tipset_or_heaviest(&end_tsk)?;

        if start_ts.epoch() > end_ts.epoch() {
            return Err(ServerError::internal_error(
                "start tipset is after end tipset",
                None,
            ));
        } else if start_ts.epoch() == end_ts.epoch() {
            return Ok(TokenAmount::zero());
        }

        let msig_actor = ctx
            .state_manager
            .get_required_actor(&addr, *end_ts.parent_state())?;
        let ms = multisig::State::load(ctx.store(), msig_actor.code, msig_actor.state)?;

        let start_lb: TokenAmount = ms.locked_balance(start_ts.epoch())?.into();
        let end_lb: TokenAmount = ms.locked_balance(end_ts.epoch())?.into();
        Ok(start_lb - &end_lb)
    }
}
