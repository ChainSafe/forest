// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::RpcState;
use beacon::Beacon;
use blockstore::BlockStore;
use forest_libp2p::{Multiaddr, NetRPCMethods, NetRPCResponse, NetworkMessage};
use futures::channel::oneshot;
use jsonrpc_v2::{Data, Error as JsonRpcError};
use serde::Serialize;
use wallet::KeyStore;

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct AddrInfo {
    #[serde(rename = "ID")]
    id: String,
    addrs: Vec<Multiaddr>,
}
pub(crate) async fn net_addrs_listen<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
) -> Result<AddrInfo, JsonRpcError> {
    let (tx, rx) = oneshot::channel();
    let req = NetworkMessage::JSONRPCRequest {
        method: NetRPCMethods::NetAddrsListen,
        response_channel: tx,
    };
    data.network_send.send(req).await;
    let resp = match rx.await? {
        NetRPCResponse::NetAddrsListen(id, addrs) => AddrInfo {
            id: id.to_string(),
            addrs,
        },
    };
    Ok(resp)
}
