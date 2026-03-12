// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod types;
use itertools::Itertools;
pub use types::*;

use std::any::Any;
use std::str::FromStr;

use crate::libp2p::{NetRPCMethods, NetworkMessage, PeerId};
use crate::rpc::{ApiPaths, Ctx, Permission, RpcMethod, ServerError};
use anyhow::{Context as _, Result};
use cid::multibase;
use enumflags2::BitFlags;
use fvm_ipld_blockstore::Blockstore;

pub enum NetAddrsListen {}
impl RpcMethod<0> for NetAddrsListen {
    const NAME: &'static str = "Filecoin.NetAddrsListen";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns a list of listening addresses and the peer ID.");

    type Params = ();
    type Ok = AddrInfo;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let (tx, rx) = flume::bounded(1);
        let req = NetworkMessage::JSONRPCRequest {
            method: NetRPCMethods::AddrsListen(tx),
        };

        ctx.network_send().send_async(req).await?;
        let (id, addrs) = rx.recv_async().await?;

        Ok(AddrInfo::new(id, addrs))
    }
}

pub enum NetPeers {}
impl RpcMethod<0> for NetPeers {
    const NAME: &'static str = "Filecoin.NetPeers";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns a list of currently connected peers.");

    type Params = ();
    type Ok = Vec<AddrInfo>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let (tx, rx) = flume::bounded(1);
        let req = NetworkMessage::JSONRPCRequest {
            method: NetRPCMethods::Peers(tx),
        };

        ctx.network_send().send_async(req).await?;
        let peer_addresses = rx.recv_async().await?;

        let connections = peer_addresses
            .into_iter()
            .map(|(id, addrs)| AddrInfo::new(id, addrs))
            .collect();

        Ok(connections)
    }
}

pub enum NetFindPeer {}
impl RpcMethod<1> for NetFindPeer {
    const NAME: &'static str = "Filecoin.NetFindPeer";
    const PARAM_NAMES: [&'static str; 1] = ["peer_id"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (String,);
    type Ok = AddrInfo;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (peer_id,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let peer_id = PeerId::from_str(&peer_id)?;
        let (tx, rx) = flume::bounded(1);
        ctx.network_send()
            .send_async(NetworkMessage::JSONRPCRequest {
                method: NetRPCMethods::Peer(tx, peer_id),
            })
            .await?;
        let addrs = rx
            .recv_async()
            .await?
            .with_context(|| format!("peer {peer_id} not found"))?;
        Ok(AddrInfo::new(peer_id, addrs))
    }
}

pub enum NetListening {}
impl RpcMethod<0> for NetListening {
    const NAME: &'static str = "Filecoin.NetListening";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;
    const NAME_ALIAS: Option<&'static str> = Some("net_listening");

    type Params = ();
    type Ok = bool;

    async fn handle(
        _: Ctx<impl Any>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(true)
    }
}

pub enum NetInfo {}
impl RpcMethod<0> for NetInfo {
    const NAME: &'static str = "Forest.NetInfo";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = NetInfoResult;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let (tx, rx) = flume::bounded(1);
        let req = NetworkMessage::JSONRPCRequest {
            method: NetRPCMethods::Info(tx),
        };

        ctx.network_send().send_async(req).await?;
        Ok(rx.recv_async().await?)
    }
}

pub enum NetConnect {}
impl RpcMethod<1> for NetConnect {
    const NAME: &'static str = "Filecoin.NetConnect";
    const PARAM_NAMES: [&'static str; 1] = ["peerAddressInfo"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Write;
    const DESCRIPTION: Option<&'static str> = Some("Connects to a specified peer.");

    type Params = (AddrInfo,);
    type Ok = ();

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (AddrInfo { id, addrs },): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let (_, id) = multibase::decode(format!("{}{}", "z", id))?;
        let peer_id = PeerId::from_bytes(&id)?;

        let (tx, rx) = flume::bounded(1);
        let req = NetworkMessage::JSONRPCRequest {
            method: NetRPCMethods::Connect(tx, peer_id, addrs),
        };

        ctx.network_send().send_async(req).await?;
        let success = rx.recv_async().await?;

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
    const PARAM_NAMES: [&'static str; 1] = ["peerId"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Write;
    const DESCRIPTION: Option<&'static str> = Some("Disconnects from the specified peer.");

    type Params = (String,);
    type Ok = ();

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (peer_id,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let peer_id = PeerId::from_str(&peer_id)?;

        let (tx, rx) = flume::bounded(1);
        let req = NetworkMessage::JSONRPCRequest {
            method: NetRPCMethods::Disconnect(tx, peer_id),
        };

        ctx.network_send().send_async(req).await?;
        rx.recv_async().await?;

        Ok(())
    }
}

pub enum NetAgentVersion {}
impl RpcMethod<1> for NetAgentVersion {
    const NAME: &'static str = "Filecoin.NetAgentVersion";
    const PARAM_NAMES: [&'static str; 1] = ["peerId"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns the agent version string.");

    type Params = (String,);
    type Ok = String;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (peer_id,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let peer_id = PeerId::from_str(&peer_id)?;
        let (tx, rx) = flume::bounded(1);
        ctx.network_send()
            .send_async(NetworkMessage::JSONRPCRequest {
                method: NetRPCMethods::AgentVersion(tx, peer_id),
            })
            .await?;
        Ok(rx.recv_async().await?.context("item not found")?)
    }
}

pub enum NetAutoNatStatus {}
impl RpcMethod<0> for NetAutoNatStatus {
    const NAME: &'static str = "Filecoin.NetAutoNatStatus";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = NatStatusResult;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let (tx, rx) = flume::bounded(1);
        let req = NetworkMessage::JSONRPCRequest {
            method: NetRPCMethods::AutoNATStatus(tx),
        };
        ctx.network_send().send_async(req).await?;
        let nat_status = rx.recv_async().await?;
        Ok(nat_status.into())
    }
}

pub enum NetVersion {}
impl RpcMethod<0> for NetVersion {
    const NAME: &'static str = "Filecoin.NetVersion";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;
    const NAME_ALIAS: Option<&'static str> = Some("net_version");

    type Params = ();
    type Ok = String;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(ctx.chain_config().eth_chain_id.to_string())
    }
}

pub enum NetProtectAdd {}
impl RpcMethod<1> for NetProtectAdd {
    const NAME: &'static str = "Filecoin.NetProtectAdd";
    const PARAM_NAMES: [&'static str; 1] = ["peerIdList"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Admin;
    const DESCRIPTION: Option<&'static str> = Some(
        "Protects a peer from having its connection(s) pruned in the event the libp2p host reaches its maximum number of peers.",
    );

    type Params = (Vec<String>,);
    type Ok = ();

    // This whitelists a peer in forest peer manager but has no impact on libp2p swarm
    // due to the fact that `rust-libp2p` implementation is very different to that
    // in go. However it would be nice to investigate connection limiting options in Rust.
    // See: <https://github.com/ChainSafe/forest/issues/4355>.
    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (peer_ids,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let peer_ids = peer_ids
            .iter()
            .map(String::as_str)
            .map(PeerId::from_str)
            .try_collect()?;
        let (tx, rx) = flume::bounded(1);
        ctx.network_send()
            .send_async(NetworkMessage::JSONRPCRequest {
                method: NetRPCMethods::ProtectPeer(tx, peer_ids),
            })
            .await?;
        rx.recv_async().await?;
        Ok(())
    }
}

pub enum NetProtectList {}
impl RpcMethod<0> for NetProtectList {
    const NAME: &'static str = "Filecoin.NetProtectList";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns the current list of protected peers.");

    type Params = ();
    type Ok = Vec<String>;
    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let (tx, rx) = flume::bounded(1);
        ctx.network_send()
            .send_async(NetworkMessage::JSONRPCRequest {
                method: NetRPCMethods::ListProtectedPeers(tx),
            })
            .await?;
        let peers = rx.recv_async().await?;
        Ok(peers.into_iter().map(|p| p.to_string()).collect())
    }
}

pub enum NetProtectRemove {}
impl RpcMethod<1> for NetProtectRemove {
    const NAME: &'static str = "Filecoin.NetProtectRemove";
    const PARAM_NAMES: [&'static str; 1] = ["peerIdList"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Admin;
    const DESCRIPTION: Option<&'static str> = Some("Remove a peer from the protected list.");

    type Params = (Vec<String>,);
    type Ok = ();

    // Similar to NetProtectAdd
    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (peer_ids,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let peer_ids = peer_ids
            .iter()
            .map(String::as_str)
            .map(PeerId::from_str)
            .try_collect()?;
        let (tx, rx) = flume::bounded(1);
        ctx.network_send()
            .send_async(NetworkMessage::JSONRPCRequest {
                method: NetRPCMethods::UnprotectPeer(tx, peer_ids),
            })
            .await?;
        rx.recv_async().await?;
        Ok(())
    }
}
