// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use crate::rpc_api::data_types::MiningBaseInfo;
use crate::{
    blocks::TipsetKeys,
    rpc_api::{
        data_types::{
            ApiActorState, ApiDeadline, ApiInvocResult, CirculatingSupply, MessageLookup,
            SectorOnChainInfo,
        },
        state_api::*,
    },
    shim::{
        address::Address, clock::ChainEpoch, econ::TokenAmount, message::Message,
        message::MethodNum, state_tree::ActorState, version::NetworkVersion,
    },
};
use cid::Cid;
use fil_actor_interface::miner::{DeadlineInfo, MinerInfo, MinerPower};
use fil_actors_shared::fvm_ipld_bitfield::BitField;
use fil_actors_shared::v10::runtime::DomainSeparationTag;
use libipld_core::ipld::Ipld;

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

    pub fn state_miner_info_req(miner: Address, tsk: TipsetKeys) -> RpcRequest<MinerInfo> {
        RpcRequest::new(STATE_MINER_INFO, (miner, tsk))
    }

    pub fn miner_get_base_info_req(
        miner: Address,
        epoch: ChainEpoch,
        tsk: TipsetKeys,
    ) -> RpcRequest<Option<MiningBaseInfo>> {
        RpcRequest::new(MINER_GET_BASE_INFO, (miner, epoch, tsk))
    }

    pub fn state_call_req(message: Message, tsk: TipsetKeys) -> RpcRequest<ApiInvocResult> {
        RpcRequest::new(STATE_CALL, (message, tsk))
    }

    pub fn state_miner_faults_req(miner: Address, tsk: TipsetKeys) -> RpcRequest<BitField> {
        RpcRequest::new(STATE_MINER_FAULTS, (miner, tsk))
    }

    pub fn state_miner_recoveries_req(miner: Address, tsk: TipsetKeys) -> RpcRequest<BitField> {
        RpcRequest::new(STATE_MINER_RECOVERIES, (miner, tsk))
    }

    pub fn state_miner_power_req(miner: Address, tsk: TipsetKeys) -> RpcRequest<MinerPower> {
        RpcRequest::new(STATE_MINER_POWER, (miner, tsk))
    }

    pub fn state_miner_deadlines_req(
        miner: Address,
        tsk: TipsetKeys,
    ) -> RpcRequest<Vec<ApiDeadline>> {
        RpcRequest::new(STATE_MINER_DEADLINES, (miner, tsk))
    }

    pub fn state_miner_proving_deadline_req(
        miner: Address,
        tsk: TipsetKeys,
    ) -> RpcRequest<DeadlineInfo> {
        RpcRequest::new(STATE_MINER_PROVING_DEADLINE, (miner, tsk))
    }

    pub fn state_get_randomness_from_tickets_req(
        tsk: TipsetKeys,
        personalization: DomainSeparationTag,
        rand_epoch: ChainEpoch,
        entropy: Vec<u8>,
    ) -> RpcRequest<Vec<u8>> {
        RpcRequest::new(
            STATE_GET_RANDOMNESS_FROM_TICKETS,
            (personalization as i64, rand_epoch, entropy, tsk),
        )
    }

    pub fn state_get_randomness_from_beacon_req(
        tsk: TipsetKeys,
        personalization: DomainSeparationTag,
        rand_epoch: ChainEpoch,
        entropy: Vec<u8>,
    ) -> RpcRequest<Vec<u8>> {
        RpcRequest::new(
            STATE_GET_RANDOMNESS_FROM_BEACON,
            (personalization as i64, rand_epoch, entropy, tsk),
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

    pub fn state_lookup_id_req(addr: Address, tsk: TipsetKeys) -> RpcRequest<Option<Address>> {
        RpcRequest::new(STATE_LOOKUP_ID, (addr, tsk))
    }

    pub fn state_network_version_req(tsk: TipsetKeys) -> RpcRequest<NetworkVersion> {
        RpcRequest::new(STATE_NETWORK_VERSION, (tsk,))
    }

    pub fn state_account_key_req(addr: Address, tsk: TipsetKeys) -> RpcRequest<Address> {
        RpcRequest::new(STATE_ACCOUNT_KEY, (addr, tsk))
    }

    pub fn state_circulating_supply_req(tsk: TipsetKeys) -> RpcRequest<TokenAmount> {
        RpcRequest::new(STATE_CIRCULATING_SUPPLY, (tsk,))
    }

    pub fn state_vm_circulating_supply_internal_req(
        tsk: TipsetKeys,
    ) -> RpcRequest<CirculatingSupply> {
        RpcRequest::new(STATE_VM_CIRCULATING_SUPPLY_INTERNAL, (tsk,))
    }

    pub fn state_decode_params_req(
        recipient: Address,
        method_number: MethodNum,
        params: Vec<u8>,
        tsk: TipsetKeys,
    ) -> RpcRequest<Ipld> {
        RpcRequest::new(STATE_DECODE_PARAMS, (recipient, method_number, params, tsk))
    }

    pub fn state_sector_get_info_req(
        addr: Address,
        sector_no: u64,
        tsk: TipsetKeys,
    ) -> RpcRequest<SectorOnChainInfo> {
        RpcRequest::new(STATE_SECTOR_GET_INFO, (addr, sector_no, tsk))
    }

    // FIXME: StateWaitMsg API gets stuck in forest
    // pub fn state_wait_msg_req(msg_cid: Cid, confidence: i64) -> RpcRequest<Option<MessageLookup>> {
    //     RpcRequest::new(STATE_WAIT_MSG, (msg_cid, confidence))
    // }

    pub fn state_search_msg_req(msg_cid: Cid) -> RpcRequest<Option<MessageLookup>> {
        RpcRequest::new(STATE_SEARCH_MSG, (msg_cid,))
    }

    pub fn state_search_msg_limited_req(
        msg_cid: Cid,
        limit_epoch: i64,
    ) -> RpcRequest<Option<MessageLookup>> {
        RpcRequest::new(STATE_SEARCH_MSG_LIMITED, (msg_cid, limit_epoch))
    }
}
