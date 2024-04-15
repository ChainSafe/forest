// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

use crate::libp2p::{NetRPCMethods, NetworkMessage, PeerId};
use crate::lotus_json::lotus_json_with_self;
use crate::rpc::Ctx;
use crate::rpc::{ApiVersion, RPCState, RpcMethodExt as _, ServerError};
use anyhow::Result;
use cid::multibase;
use futures::channel::oneshot;
use fvm_ipld_blockstore::Blockstore;
use jsonrpsee::types::Params;
use libp2p::Multiaddr;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

macro_rules! for_each_method {
    ($callback:ident) => {
        //
    };
}
pub(crate) use for_each_method;

// Net API
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct AddrInfo {
    #[serde(rename = "ID")]
    pub id: String,
    pub addrs: ahash::HashSet<Multiaddr>,
}

lotus_json_with_self!(AddrInfo);

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct NetInfoResult {
    pub num_peers: usize,
    pub num_connections: u32,
    pub num_pending: u32,
    pub num_pending_incoming: u32,
    pub num_pending_outgoing: u32,
    pub num_established: u32,
}
lotus_json_with_self!(NetInfoResult);

impl From<libp2p::swarm::NetworkInfo> for NetInfoResult {
    fn from(i: libp2p::swarm::NetworkInfo) -> Self {
        let counters = i.connection_counters();
        Self {
            num_peers: i.num_peers(),
            num_connections: counters.num_connections(),
            num_pending: counters.num_pending(),
            num_pending_incoming: counters.num_pending_incoming(),
            num_pending_outgoing: counters.num_pending_outgoing(),
            num_established: counters.num_established(),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct NatStatusResult {
    pub reachability: i32,
    pub public_addrs: Option<Vec<String>>,
}
lotus_json_with_self!(NatStatusResult);

impl NatStatusResult {
    // See <https://github.com/libp2p/go-libp2p/blob/164adb40fef9c19774eb5fe6d92afb95c67ba83c/core/network/network.go#L93>
    pub fn reachability_as_str(&self) -> &'static str {
        match self.reachability {
            0 => "Unknown",
            1 => "Public",
            2 => "Private",
            _ => "(unrecognized)",
        }
    }
}

impl From<libp2p::autonat::NatStatus> for NatStatusResult {
    fn from(nat: libp2p::autonat::NatStatus) -> Self {
        use libp2p::autonat::NatStatus;

        // See <https://github.com/libp2p/go-libp2p/blob/91e1025f04519a5560361b09dfccd4b5239e36e6/core/network/network.go#L77>
        let (reachability, public_addrs) = match &nat {
            NatStatus::Unknown => (0, None),
            NatStatus::Public(addr) => (1, Some(vec![addr.to_string()])),
            NatStatus::Private => (2, None),
        };

        NatStatusResult {
            reachability,
            public_addrs,
        }
    }
}

pub const NET_ADDRS_LISTEN: &str = "Filecoin.NetAddrsListen";
pub async fn net_addrs_listen<DB: Blockstore>(data: Ctx<DB>) -> Result<AddrInfo, ServerError> {
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

pub const NET_PEERS: &str = "Filecoin.NetPeers";
pub async fn net_peers<DB: Blockstore>(data: Ctx<DB>) -> Result<Vec<AddrInfo>, ServerError> {
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

// NET_LISTENING always returns true.
pub const NET_LISTENING: &str = "Filecoin.NetListening"; // V1
pub async fn net_listening() -> Result<bool, ServerError> {
    Ok(true)
}

pub const NET_INFO: &str = "Filecoin.NetInfo";
pub async fn net_info<DB: Blockstore>(data: Ctx<DB>) -> Result<NetInfoResult, ServerError> {
    let (tx, rx) = oneshot::channel();
    let req = NetworkMessage::JSONRPCRequest {
        method: NetRPCMethods::Info(tx),
    };

    data.network_send.send_async(req).await?;
    Ok(rx.await?)
}

pub const NET_CONNECT: &str = "Filecoin.NetConnect";
pub async fn net_connect<DB: Blockstore>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<(), ServerError> {
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

pub const NET_DISCONNECT: &str = "Filecoin.NetDisconnect";
pub async fn net_disconnect<DB: Blockstore>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<(), ServerError> {
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

pub const NET_AGENT_VERSION: &str = "Filecoin.NetAgentVersion";
pub async fn net_agent_version<DB: Blockstore>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<String, ServerError> {
    let (id,): (String,) = params.parse()?;

    let peer_id = PeerId::from_str(&id)?;

    let (tx, rx) = oneshot::channel();
    let req = NetworkMessage::JSONRPCRequest {
        method: NetRPCMethods::AgentVersion(tx, peer_id),
    };

    data.network_send.send_async(req).await?;
    if let Some(agent_version) = rx.await? {
        Ok(agent_version)
    } else {
        Err(anyhow::anyhow!("item not found").into())
    }
}

pub const NET_AUTO_NAT_STATUS: &str = "Filecoin.NetAutoNatStatus";
pub async fn net_auto_nat_status<DB: Blockstore>(
    _params: Params<'_>,
    data: Ctx<DB>,
) -> Result<NatStatusResult, ServerError> {
    let (tx, rx) = oneshot::channel();
    let req = NetworkMessage::JSONRPCRequest {
        method: NetRPCMethods::AutoNATStatus(tx),
    };
    data.network_send.send_async(req).await?;
    let nat_status = rx.await?;
    Ok(nat_status.into())
}

pub const NET_VERSION: &str = "Filecoin.NetVersion"; // V1
pub async fn net_version<DB: Blockstore>(
    _params: Params<'_>,
    data: Ctx<DB>,
) -> Result<String, ServerError> {
    Ok(format!(
        "{}",
        data.state_manager.chain_config().eth_chain_id
    ))
}
