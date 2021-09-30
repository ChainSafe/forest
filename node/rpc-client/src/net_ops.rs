// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::call;
use jsonrpc_v2::Error;
use rpc_api::net_api::*;

pub async fn net_addrs_listen(params: NetAddrsListenParams) -> Result<NetAddrsListenResult, Error> {
    call(NET_ADDRS_LISTEN, params).await
}

pub async fn net_peers(params: NetPeersParams) -> Result<NetPeersResult, Error> {
    call(NET_PEERS, params).await
}

pub async fn net_connect(params: NetConnectParams) -> Result<NetConnectResult, Error> {
    call(NET_CONNECT, params).await
}

pub async fn net_disconnect(params: NetDisconnectParams) -> Result<NetDisconnectResult, Error> {
    call(NET_DISCONNECT, params).await
}
