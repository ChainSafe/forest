// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{ApiInfo, RpcRequest, ServerError};
use crate::rpc::net::*;

impl ApiInfo {
    pub async fn net_addrs_listen(&self) -> Result<AddrInfo, ServerError> {
        todo!()
    }

    pub fn net_addrs_listen_req() -> RpcRequest<AddrInfo> {
        todo!()
    }

    pub async fn net_peers(&self) -> Result<Vec<AddrInfo>, ServerError> {
        todo!()
    }

    pub fn net_peers_req() -> RpcRequest<Vec<AddrInfo>> {
        todo!()
    }

    pub fn net_listening_req() -> RpcRequest<bool> {
        todo!()
    }

    pub async fn net_info(&self) -> Result<NetInfoResult, ServerError> {
        todo!()
    }

    pub fn net_info_req() -> RpcRequest<NetInfoResult> {
        todo!()
    }

    pub async fn net_connect(&self, addr: AddrInfo) -> Result<(), ServerError> {
        todo!()
    }

    pub fn net_connect_req(addr: AddrInfo) -> RpcRequest<()> {
        todo!()
    }

    pub async fn net_disconnect(&self, peer: String) -> Result<(), ServerError> {
        todo!()
    }

    pub fn net_disconnect_req(peer: String) -> RpcRequest<()> {
        todo!()
    }

    pub async fn net_agent_version(&self, peer: String) -> Result<String, ServerError> {
        todo!()
    }

    pub fn net_agent_version_req(peer: String) -> RpcRequest<String> {
        todo!()
    }

    pub async fn net_auto_nat_status(&self) -> Result<NatStatusResult, ServerError> {
        todo!()
    }

    pub fn net_auto_nat_status_req() -> RpcRequest<NatStatusResult> {
        todo!()
    }

    pub fn net_version_req() -> RpcRequest<String> {
        todo!()
    }
}
