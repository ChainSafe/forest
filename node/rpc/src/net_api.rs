// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use futures::channel::oneshot;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use log::error;

use beacon::Beacon;
use blockstore::BlockStore;
use forest_libp2p::{NetRPCMethods, NetworkMessage, PeerId};
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
    let peer_addresses = rx.await?;

    let connections = peer_addresses
        .into_iter()
        .map(|(id, addrs)| AddrInfo {
            id: id.to_string(),
            addrs,
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
    let (AddrInfo { id, addrs },) = params;
    let (_, id) = multibase::decode(format!("{}{}", "z", id))?;
    let peer_id = PeerId::from_bytes(&id)?;

    let (tx, rx) = oneshot::channel();
    let req = NetworkMessage::JSONRPCRequest {
        method: NetRPCMethods::NetConnect(tx, peer_id, addrs),
    };

    data.network_send.send(req).await?;
    let success = rx.await?;

    if success {
        Ok(())
    } else {
        error!("Peer could not be dialed from any address provided");
        Err(JsonRpcError::INTERNAL_ERROR)
    }
}

pub(crate) async fn net_disconnect<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<NetDisconnectParams>,
) -> Result<NetDisconnectResult, JsonRpcError> {
    let (id,) = params;
    let peer_id = PeerId::from_bytes(id.as_bytes())?;

    let (tx, rx) = oneshot::channel();
    let req = NetworkMessage::JSONRPCRequest {
        method: NetRPCMethods::NetDisconnect(tx, peer_id),
    };

    data.network_send.send(req).await?;
    rx.await?;

    Ok(())
}
