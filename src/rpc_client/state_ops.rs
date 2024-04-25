// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;
use std::time::Duration;

use crate::rpc::types::*;
use crate::state_manager::MarketBalance;
use crate::{
    rpc::state::*,
    shim::{address::Address, deal::DealID, message::MethodNum, version::NetworkVersion},
};
use cid::Cid;
use fvm_shared2::piece::PaddedPieceSize;
use libipld_core::ipld::Ipld;

use super::{ApiInfo, RpcRequest, ServerError};

impl ApiInfo {
    pub fn state_market_balance_req(
        miner: Address,
        tsk: ApiTipsetKey,
    ) -> RpcRequest<MarketBalance> {
        RpcRequest::new(STATE_MARKET_BALANCE, (miner, tsk))
    }

    pub async fn state_fetch_root(
        &self,
        root: Cid,
        opt_path: Option<PathBuf>,
    ) -> Result<String, ServerError> {
        self.call(Self::state_fetch_root_req(root, opt_path)).await
    }

    pub fn state_fetch_root_req(root: Cid, opt_path: Option<PathBuf>) -> RpcRequest<String> {
        RpcRequest::new(STATE_FETCH_ROOT, (root, opt_path))
    }

    pub fn state_miner_initial_pledge_collateral_req(
        miner: Address,
        info: SectorPreCommitInfo,
        tsk: ApiTipsetKey,
    ) -> RpcRequest<String> {
        RpcRequest::new(STATE_MINER_INITIAL_PLEDGE_COLLATERAL, (miner, info, tsk))
    }

    pub fn state_network_version_req(tsk: ApiTipsetKey) -> RpcRequest<NetworkVersion> {
        RpcRequest::new(STATE_NETWORK_VERSION, (tsk,))
    }

    pub fn state_decode_params_req(
        recipient: Address,
        method_number: MethodNum,
        params: Vec<u8>,
        tsk: ApiTipsetKey,
    ) -> RpcRequest<Ipld> {
        RpcRequest::new(STATE_DECODE_PARAMS, (recipient, method_number, params, tsk))
    }

    pub fn state_wait_msg_req(msg_cid: Cid, confidence: i64) -> RpcRequest<Option<MessageLookup>> {
        // This API is meant to be blocking when the message is missing from the blockstore
        RpcRequest::new(STATE_WAIT_MSG, (msg_cid, confidence)).with_timeout(Duration::MAX)
    }

    pub fn state_search_msg_req(msg_cid: Cid) -> RpcRequest<Option<MessageLookup>> {
        RpcRequest::new(STATE_SEARCH_MSG, (msg_cid,))
    }

    pub fn state_search_msg_limited_req(
        msg_cid: Cid,
        limit_epoch: i64,
    ) -> RpcRequest<Option<MessageLookup>> {
        RpcRequest::new(STATE_SEARCH_MSG_LIMITED, (msg_cid, limit_epoch))
    }

    pub fn state_deal_provider_collateral_bounds_req(
        size: PaddedPieceSize,
        verified: bool,
        tsk: ApiTipsetKey,
    ) -> RpcRequest<DealCollateralBounds> {
        RpcRequest::new(STATE_DEAL_PROVIDER_COLLATERAL_BOUNDS, (size, verified, tsk))
    }

    pub fn state_market_storage_deal_req(
        deal_id: DealID,
        tsk: ApiTipsetKey,
    ) -> RpcRequest<ApiMarketDeal> {
        RpcRequest::new(STATE_MARKET_STORAGE_DEAL, (deal_id, tsk))
    }
}
