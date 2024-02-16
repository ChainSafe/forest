// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::redundant_allocation)]

use std::str::FromStr;

use crate::libp2p::{NetRPCMethods, NetworkMessage, PeerId};
use crate::rpc::error::JsonRpcError;
use crate::rpc_api::{
    data_types::{AddrInfo, RPCState},
    net_api::*,
};
use cid::multibase;
use futures::channel::oneshot;
use fvm_ipld_blockstore::Blockstore;
use jsonrpsee::types::Params;

use anyhow::Result;
use std::sync::Arc;

pub async fn net_addrs_listen<DB: Blockstore>(
    data: Arc<Arc<RPCState<DB>>>,
) -> Result<AddrInfo, JsonRpcError> {
    let (tx, rx) = oneshot::channel();
    let req = NetworkMessage::JSONRPCRequest {
        method: NetRPCMethods::AddrsListen(tx),
    };

    data.network_send.send_async(req).await?;
    let (id, addrs) = rx.await?;

    Ok(AddrInfo {
        id: id.to_string(),
        addrs,
    })
}

pub async fn net_peers<DB: Blockstore>(
    data: Arc<Arc<RPCState<DB>>>,
) -> Result<Vec<AddrInfo>, JsonRpcError> {
    let (tx, rx) = oneshot::channel();
    let req = NetworkMessage::JSONRPCRequest {
        method: NetRPCMethods::Peers(tx),
    };

    data.network_send.send_async(req).await?;
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

pub async fn net_info<DB: Blockstore>(
    data: Arc<Arc<RPCState<DB>>>,
) -> Result<NetInfoResult, JsonRpcError> {
    let (tx, rx) = oneshot::channel();
    let req = NetworkMessage::JSONRPCRequest {
        method: NetRPCMethods::Info(tx),
    };

    data.network_send.send_async(req).await?;
    Ok(rx.await?)
}

pub async fn net_connect<DB: Blockstore>(
    params: Params<'_>,
    data: Arc<Arc<RPCState<DB>>>,
) -> Result<(), JsonRpcError> {
    let (AddrInfo { id, addrs },) = params.parse()?;

    let (_, id) = multibase::decode(format!("{}{}", "z", id))?;
    let peer_id = PeerId::from_bytes(&id)?;

    let (tx, rx) = oneshot::channel();
    let req = NetworkMessage::JSONRPCRequest {
        method: NetRPCMethods::Connect(tx, peer_id, addrs),
    };

    data.network_send.send_async(req).await?;
    let success = rx.await?;

    if success {
        Ok(())
    } else {
        Err(anyhow::anyhow!("Peer could not be dialed from any address provided").into())
    }
}

pub async fn net_disconnect<DB: Blockstore>(
    params: Params<'_>,
    data: Arc<Arc<RPCState<DB>>>,
) -> Result<(), JsonRpcError> {
    let (id,): (String,) = params.parse()?;

    let peer_id = PeerId::from_str(&id)?;

    let (tx, rx) = oneshot::channel();
    let req = NetworkMessage::JSONRPCRequest {
        method: NetRPCMethods::Disconnect(tx, peer_id),
    };

    data.network_send.send_async(req).await?;
    rx.await?;

    Ok(())
}
