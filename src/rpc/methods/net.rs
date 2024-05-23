// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::any::Any;
use std::str::FromStr;

use crate::libp2p::{NetRPCMethods, NetworkMessage, PeerId};
use crate::lotus_json::lotus_json_with_self;
use crate::rpc::{ApiVersion, Permission, ServerError};
use crate::rpc::{Ctx, RpcMethod};
use anyhow::Result;
use cid::multibase;
use futures::channel::oneshot;
use fvm_ipld_blockstore::Blockstore;
use libp2p::Multiaddr;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub enum NetAddrsListen {}
impl RpcMethod<0> for NetAddrsListen {
    const NAME: &'static str = "Filecoin.NetAddrsListen";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_VERSION: ApiVersion = ApiVersion::V0;
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = AddrInfo;

    async fn handle(ctx: Ctx<impl Blockstore>, (): Self::Params) -> Result<Self::Ok, ServerError> {
        let (tx, rx) = oneshot::channel();
        let req = NetworkMessage::JSONRPCRequest {
            method: NetRPCMethods::AddrsListen(tx),
        };

        ctx.network_send.send_async(req).await?;
        let (id, addrs) = rx.await?;

        Ok(AddrInfo {
            id: id.to_string(),
            addrs,
        })
    }
}

pub enum NetPeers {}
impl RpcMethod<0> for NetPeers {
    const NAME: &'static str = "Filecoin.NetPeers";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_VERSION: ApiVersion = ApiVersion::V0;
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = Vec<AddrInfo>;

    async fn handle(ctx: Ctx<impl Blockstore>, (): Self::Params) -> Result<Self::Ok, ServerError> {
        let (tx, rx) = oneshot::channel();
        let req = NetworkMessage::JSONRPCRequest {
            method: NetRPCMethods::Peers(tx),
        };

        ctx.network_send.send_async(req).await?;
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
}

pub enum NetListening {}
impl RpcMethod<0> for NetListening {
    const NAME: &'static str = "Filecoin.NetListening";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_VERSION: ApiVersion = ApiVersion::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = bool;

    async fn handle(_: Ctx<impl Any>, (): Self::Params) -> Result<Self::Ok, ServerError> {
        Ok(true)
    }
}

pub enum NetInfo {}
impl RpcMethod<0> for NetInfo {
    const NAME: &'static str = "Forest.NetInfo";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_VERSION: ApiVersion = ApiVersion::V0;
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = NetInfoResult;

    async fn handle(ctx: Ctx<impl Blockstore>, (): Self::Params) -> Result<Self::Ok, ServerError> {
        let (tx, rx) = oneshot::channel();
        let req = NetworkMessage::JSONRPCRequest {
            method: NetRPCMethods::Info(tx),
        };

        ctx.network_send.send_async(req).await?;
        Ok(rx.await?)
    }
}

pub enum NetConnect {}
impl RpcMethod<1> for NetConnect {
    const NAME: &'static str = "Filecoin.NetConnect";
    const PARAM_NAMES: [&'static str; 1] = ["info"];
    const API_VERSION: ApiVersion = ApiVersion::V0;
    const PERMISSION: Permission = Permission::Write;

    type Params = (AddrInfo,);
    type Ok = ();

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (AddrInfo { id, addrs },): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let (_, id) = multibase::decode(format!("{}{}", "z", id))?;
        let peer_id = PeerId::from_bytes(&id)?;

        let (tx, rx) = oneshot::channel();
        let req = NetworkMessage::JSONRPCRequest {
            method: NetRPCMethods::Connect(tx, peer_id, addrs),
        };

        ctx.network_send.send_async(req).await?;
        let success = rx.await?;

        if success {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Peer could not be dialed from any address provided").into())
        }
    }
}

pub enum NetDisconnect {}
impl RpcMethod<1> for NetDisconnect {
    const NAME: &'static str = "Filecoin.NetDisconnect";
    const PARAM_NAMES: [&'static str; 1] = ["id"];
    const API_VERSION: ApiVersion = ApiVersion::V0;
    const PERMISSION: Permission = Permission::Write;

    type Params = (String,);
    type Ok = ();

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (id,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let peer_id = PeerId::from_str(&id)?;

        let (tx, rx) = oneshot::channel();
        let req = NetworkMessage::JSONRPCRequest {
            method: NetRPCMethods::Disconnect(tx, peer_id),
        };

        ctx.network_send.send_async(req).await?;
        rx.await?;

        Ok(())
    }
}

pub enum NetAgentVersion {}
impl RpcMethod<1> for NetAgentVersion {
    const NAME: &'static str = "Filecoin.NetAgentVersion";
    const PARAM_NAMES: [&'static str; 1] = ["id"];
    const API_VERSION: ApiVersion = ApiVersion::V0;
    const PERMISSION: Permission = Permission::Read;

    type Params = (String,);
    type Ok = String;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (id,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let peer_id = PeerId::from_str(&id)?;

        let (tx, rx) = oneshot::channel();
        let req = NetworkMessage::JSONRPCRequest {
            method: NetRPCMethods::AgentVersion(tx, peer_id),
        };

        ctx.network_send.send_async(req).await?;
        if let Some(agent_version) = rx.await? {
            Ok(agent_version)
        } else {
            Err(anyhow::anyhow!("item not found").into())
        }
    }
}

pub enum NetAutoNatStatus {}
impl RpcMethod<0> for NetAutoNatStatus {
    const NAME: &'static str = "Filecoin.NetAutoNatStatus";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_VERSION: ApiVersion = ApiVersion::V0;
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = NatStatusResult;

    async fn handle(ctx: Ctx<impl Blockstore>, (): Self::Params) -> Result<Self::Ok, ServerError> {
        let (tx, rx) = oneshot::channel();
        let req = NetworkMessage::JSONRPCRequest {
            method: NetRPCMethods::AutoNATStatus(tx),
        };
        ctx.network_send.send_async(req).await?;
        let nat_status = rx.await?;
        Ok(nat_status.into())
    }
}

pub enum NetVersion {}
impl RpcMethod<0> for NetVersion {
    const NAME: &'static str = "Filecoin.NetVersion";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_VERSION: ApiVersion = ApiVersion::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = String;

    async fn handle(ctx: Ctx<impl Blockstore>, (): Self::Params) -> Result<Self::Ok, ServerError> {
        Ok(ctx.state_manager.chain_config().eth_chain_id.to_string())
    }
}

pub enum NetProtectAdd {}
impl RpcMethod<1> for NetProtectAdd {
    const NAME: &'static str = "Filecoin.NetProtectAdd";
    const PARAM_NAMES: [&'static str; 1] = ["acl"];
    const API_VERSION: ApiVersion = ApiVersion::V1;
    const PERMISSION: Permission = Permission::Admin;

    type Params = (String,);
    type Ok = ();

    // This is a no-op due to the fact that `rust-libp2p` implementation is very different to that
    // in go. However it would be nice to investigate connection limiting options in Rust.
    // See: <https://github.com/ChainSafe/forest/issues/4355>.
    async fn handle(
        _: Ctx<impl Blockstore>,
        (peer_id,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let _ = PeerId::from_str(&peer_id)?;
        Ok(())
    }
}

// Net API
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct AddrInfo {
    #[serde(rename = "ID")]
    pub id: String,
    #[schemars(with = "ahash::HashSet<String>")]
    pub addrs: ahash::HashSet<Multiaddr>,
}

lotus_json_with_self!(AddrInfo);

#[derive(Debug, Default, Serialize, Deserialize, Clone, JsonSchema)]
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

#[derive(Debug, Default, Serialize, Deserialize, Clone, JsonSchema)]
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
