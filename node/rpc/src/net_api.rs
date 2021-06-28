// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use futures::channel::oneshot;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};

use beacon::Beacon;
use blockstore::BlockStore;
use forest_libp2p::{NetRPCMethods, NetworkMessage};
use rpc_api::{
    data_types::{AddrInfo, RPCState},
    net_api::*,
};

pub(crate) async fn net_addrs_listen<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
) -> Result<NetAddrsListenResult, JsonRpcError> {
    let (tx, rx) = oneshot::channel();
    let req = NetworkMessage::JSONRPCRequest {
        method: NetRPCMethods::NetAddrsListen(tx),
    };
    data.network_send.send(req).await?;
    let (id, addrs) = rx.await?;

    Ok(AddrInfo {
        id: id.to_string(),
        addrs,
    })
}

pub(crate) async fn net_peers<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
) -> Result<NetPeersResult, JsonRpcError> {
    let (tx, rx) = oneshot::channel();
    let req = NetworkMessage::JSONRPCRequest {
        method: NetRPCMethods::NetPeers(tx),
    };

    data.network_send.send(req).await?;
    let addrs = rx.await?;

    let connections = addrs
        .iter()
        .map(|(id, addr)| AddrInfo {
            id: id.to_string(),
            addrs: vec![addr.to_owned()],
        })
        .collect();

    Ok(connections)
}

pub(crate) async fn net_connect<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<NetConnectParams>,
) -> Result<NetConnectResult, JsonRpcError> {
    todo!();
}

pub(crate) async fn net_disconnect<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<NetDisconnectParams>,
) -> Result<NetDisconnectResult, JsonRpcError> {
    todo!();
}
