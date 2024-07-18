// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod types;
pub use types::*;

use std::any::Any;
use std::str::FromStr;

use crate::libp2p::{NetRPCMethods, NetworkMessage, PeerId};
use crate::rpc::{ApiPaths, Ctx, Permission, RpcMethod, ServerError};
use anyhow::{Context as _, Result};
use cid::multibase;
use futures::channel::oneshot;
use fvm_ipld_blockstore::Blockstore;

pub enum NetAddrsListen {}
impl RpcMethod<0> for NetAddrsListen {
    const NAME: &'static str = "Filecoin.NetAddrsListen";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: ApiPaths = ApiPaths::V0;
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

        Ok(AddrInfo::new(id, addrs))
    }
}

pub enum NetPeers {}
impl RpcMethod<0> for NetPeers {
    const NAME: &'static str = "Filecoin.NetPeers";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: ApiPaths = ApiPaths::V0;
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
            .map(|(id, addrs)| AddrInfo::new(id, addrs))
            .collect();

        Ok(connections)
    }
}

pub enum NetFindPeer {}
impl RpcMethod<1> for NetFindPeer {
    const NAME: &'static str = "Filecoin.NetFindPeer";
    const PARAM_NAMES: [&'static str; 1] = ["peer_id"];
    const API_PATHS: ApiPaths = ApiPaths::V0;
    const PERMISSION: Permission = Permission::Read;

    type Params = (String,);
    type Ok = AddrInfo;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (peer_id,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let peer_id = PeerId::from_str(&peer_id)?;
        let (tx, rx) = oneshot::channel();
        let req = NetworkMessage::JSONRPCRequest {
            method: NetRPCMethods::Peer(tx, peer_id),
        };
        ctx.network_send.send_async(req).await?;
        let addrs = rx
            .await?
            .with_context(|| format!("peer {peer_id} not found"))?;
        Ok(AddrInfo::new(peer_id, addrs))
    }
}

pub enum NetListening {}
impl RpcMethod<0> for NetListening {
    const NAME: &'static str = "Filecoin.NetListening";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: ApiPaths = ApiPaths::V1;
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
    const API_PATHS: ApiPaths = ApiPaths::V0;
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
    const API_PATHS: ApiPaths = ApiPaths::V0;
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
    const API_PATHS: ApiPaths = ApiPaths::V0;
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
    const API_PATHS: ApiPaths = ApiPaths::V0;
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
    const API_PATHS: ApiPaths = ApiPaths::V0;
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
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = String;

    async fn handle(ctx: Ctx<impl Blockstore>, (): Self::Params) -> Result<Self::Ok, ServerError> {
        Ok(ctx.chain_config().eth_chain_id.to_string())
    }
}

pub enum NetProtectAdd {}
impl RpcMethod<1> for NetProtectAdd {
    const NAME: &'static str = "Filecoin.NetProtectAdd";
    const PARAM_NAMES: [&'static str; 1] = ["acl"];
    const API_PATHS: ApiPaths = ApiPaths::V0;
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
