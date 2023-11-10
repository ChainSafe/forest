// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use crate::{
    blocks::TipsetKeys,
    rpc_api::{
        data_types::{ApiActorState, SectorOnChainInfo},
        state_api::*,
    },
    shim::{address::Address, clock::ChainEpoch, state_tree::ActorState},
};
use cid::Cid;
use fil_actor_interface::miner::MinerPower;
use fil_actors_shared::v10::runtime::DomainSeparationTag;

use super::{ApiInfo, JsonRpcError, RpcRequest};

impl ApiInfo {
    pub async fn state_get_actor(
        &self,
        address: Address,
        head: TipsetKeys,
    ) -> Result<Option<ActorState>, JsonRpcError> {
        self.call(Self::state_get_actor_req(address, head)).await
    }

    pub fn state_get_actor_req(
        address: Address,
        head: TipsetKeys,
    ) -> RpcRequest<Option<ActorState>> {
        RpcRequest::new(STATE_GET_ACTOR, (address, head))
    }

    pub async fn state_fetch_root(
        &self,
        root: Cid,
        opt_path: Option<PathBuf>,
    ) -> Result<String, JsonRpcError> {
        self.call(Self::state_fetch_root_req(root, opt_path)).await
    }

    pub fn state_fetch_root_req(root: Cid, opt_path: Option<PathBuf>) -> RpcRequest<String> {
        RpcRequest::new(STATE_FETCH_ROOT, (root, opt_path))
    }

    pub async fn state_network_name(&self) -> Result<String, JsonRpcError> {
        self.call(Self::state_network_name_req()).await
    }

    pub fn state_network_name_req() -> RpcRequest<String> {
        RpcRequest::new(STATE_NETWORK_NAME, ())
    }

    pub fn state_miner_power_req(miner: Address, tsk: TipsetKeys) -> RpcRequest<MinerPower> {
        RpcRequest::new(STATE_MINER_POWER, (miner, tsk))
    }

    pub fn state_get_randomness_from_beacon_req(
        tsk: TipsetKeys,
        personalization: DomainSeparationTag,
        rand_epoch: ChainEpoch,
        entropy: Vec<u8>,
    ) -> RpcRequest<Vec<u8>> {
        RpcRequest::new(
            STATE_GET_RANDOMNESS_FROM_BEACON,
            (personalization as u32, rand_epoch, entropy, tsk),
        )
    }

    pub fn state_read_state_req(actor: Address, tsk: TipsetKeys) -> RpcRequest<ApiActorState> {
        RpcRequest::new(STATE_READ_STATE, (actor, tsk))
    }

    pub fn state_miner_active_sectors_req(
        actor: Address,
        tsk: TipsetKeys,
    ) -> RpcRequest<Vec<SectorOnChainInfo>> {
        RpcRequest::new(STATE_MINER_ACTIVE_SECTORS, (actor, tsk))
    }
}
