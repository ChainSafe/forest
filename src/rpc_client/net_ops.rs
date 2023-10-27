// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_api::{data_types::AddrInfo, net_api::*};

use super::{ApiInfo, JsonRpcError, RpcRequest};

impl ApiInfo {
    pub async fn net_addrs_listen(&self) -> Result<AddrInfo, JsonRpcError> {
        self.call(Self::net_addrs_listen_req()).await
    }

    pub fn net_addrs_listen_req() -> RpcRequest<AddrInfo> {
        RpcRequest::new(NET_ADDRS_LISTEN, ())
    }

    pub async fn net_peers(&self) -> Result<Vec<AddrInfo>, JsonRpcError> {
        self.call(Self::net_peers_req()).await
    }

    pub fn net_peers_req() -> RpcRequest<Vec<AddrInfo>> {
        RpcRequest::new(NET_PEERS, ())
    }

    pub async fn net_info(&self) -> Result<NetInfoResult, JsonRpcError> {
        self.call(Self::net_info_req()).await
    }

    pub fn net_info_req() -> RpcRequest<NetInfoResult> {
        RpcRequest::new(NET_INFO, ())
    }

    pub async fn net_connect(&self, addr: AddrInfo) -> Result<(), JsonRpcError> {
        self.call(Self::net_connect_req(addr)).await
    }

    pub fn net_connect_req(addr: AddrInfo) -> RpcRequest<()> {
        RpcRequest::new(NET_CONNECT, (addr,))
    }

    pub async fn net_disconnect(&self, peer: String) -> Result<(), JsonRpcError> {
        self.call(Self::net_disconnect_req(peer)).await
    }

    pub fn net_disconnect_req(peer: String) -> RpcRequest<()> {
        RpcRequest::new(NET_DISCONNECT, (peer,))
    }
}
