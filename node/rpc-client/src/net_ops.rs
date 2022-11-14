// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::call;
use forest_rpc_api::net_api::*;
use jsonrpc_v2::Error;

pub async fn net_addrs_listen(
    params: NetAddrsListenParams,
    auth_token: &Option<String>,
) -> Result<NetAddrsListenResult, Error> {
    call(NET_ADDRS_LISTEN, params, auth_token).await
}

pub async fn net_peers(
    params: NetPeersParams,
    auth_token: &Option<String>,
) -> Result<NetPeersResult, Error> {
    call(NET_PEERS, params, auth_token).await
}

pub async fn net_connect(
    params: NetConnectParams,
    auth_token: &Option<String>,
) -> Result<NetConnectResult, Error> {
    call(NET_CONNECT, params, auth_token).await
}

pub async fn net_disconnect(
    params: NetDisconnectParams,
    auth_token: &Option<String>,
) -> Result<NetDisconnectResult, Error> {
    call(NET_DISCONNECT, params, auth_token).await
}
