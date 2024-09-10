// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//!
//! This module contains F3(fast finality) related V1 RPC methods
//! as well as some internal RPC methods(F3.*) that power
//! the go-f3 node in sidecar mode.
//!

mod types;
mod util;

use self::{types::*, util::*};
use super::wallet::WalletSign;
use crate::{
    chain::index::ResolveNullTipset,
    libp2p::{NetRPCMethods, NetworkMessage},
    lotus_json::HasLotusJson as _,
    rpc::{ApiPaths, Ctx, Permission, RpcMethod, ServerError},
    shim::{
        address::{Address, Protocol},
        clock::ChainEpoch,
        crypto::Signature,
    },
};
use ahash::{HashMap, HashSet};
use fil_actor_interface::{
    convert::{
        from_policy_v13_to_v10, from_policy_v13_to_v11, from_policy_v13_to_v12,
        from_policy_v13_to_v14, from_policy_v13_to_v9,
    },
    miner, power,
};
use fvm_ipld_blockstore::Blockstore;
use jsonrpsee::core::{client::ClientT as _, params::ArrayParams};
use libp2p::PeerId;
use num::Signed as _;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use std::{borrow::Cow, fmt::Display, str::FromStr as _, sync::Arc};

static F3_LEASE_MANAGER: Lazy<F3LeaseManager> = Lazy::new(Default::default);

pub enum GetTipsetByEpoch {}
impl RpcMethod<1> for GetTipsetByEpoch {
    const NAME: &'static str = "F3.GetTipsetByEpoch";
    const PARAM_NAMES: [&'static str; 1] = ["epoch"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (ChainEpoch,);
    type Ok = F3TipSet;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (epoch,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_index().tipset_by_height(
            epoch,
            ctx.chain_store().heaviest_tipset(),
            ResolveNullTipset::TakeOlder,
        )?;
        Ok(ts.into())
    }
}

pub enum GetTipset {}
impl RpcMethod<1> for GetTipset {
    const NAME: &'static str = "F3.GetTipset";
    const PARAM_NAMES: [&'static str; 1] = ["tipset_key"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (F3TipSetKey,);
    type Ok = F3TipSet;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (f3_tsk,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let tsk = f3_tsk.try_into()?;
        let ts = ctx.chain_index().load_required_tipset(&tsk)?;
        Ok(ts.into())
    }
}

pub enum GetHead {}
impl RpcMethod<0> for GetHead {
    const NAME: &'static str = "F3.GetHead";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = F3TipSet;

    async fn handle(ctx: Ctx<impl Blockstore>, _: Self::Params) -> Result<Self::Ok, ServerError> {
        Ok(ctx.chain_store().heaviest_tipset().into())
    }
}

pub enum GetParent {}
impl RpcMethod<1> for GetParent {
    const NAME: &'static str = "F3.GetParent";
    const PARAM_NAMES: [&'static str; 1] = ["tipset_key"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (F3TipSetKey,);
    type Ok = F3TipSet;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (f3_tsk,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let tsk = f3_tsk.try_into()?;
        let ts = ctx.chain_index().load_required_tipset(&tsk)?;
        let parent = ctx.chain_index().load_required_tipset(ts.parents())?;
        Ok(parent.into())
    }
}

pub enum GetPowerTable {}
impl RpcMethod<1> for GetPowerTable {
    const NAME: &'static str = "F3.GetPowerTable";
    const PARAM_NAMES: [&'static str; 1] = ["tipset_key"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (F3TipSetKey,);
    type Ok = Vec<F3PowerEntry>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (f3_tsk,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        macro_rules! handle_miner_state_v12_on {
            ($version:tt, $id_power_worker_mappings:ident, $ts:expr, $state:expr, $policy:expr) => {
                fn map_err<E: Display>(e: E) -> fil_actors_shared::$version::ActorError {
                    fil_actors_shared::$version::ActorError::unspecified(e.to_string())
                }

                let claims = $state.load_claims(ctx.store())?;
                claims.for_each(|miner, claim| {
                    if !claim.quality_adj_power.is_positive() {
                        return Ok(());
                    }

                    let id = miner.id().map_err(map_err)?;
                    let (_, ok) = $state.miner_nominal_power_meets_consensus_minimum(
                        $policy,
                        ctx.store(),
                        id,
                    )?;
                    if !ok {
                        return Ok(());
                    }
                    let power = claim.quality_adj_power.clone();
                    let miner_state: miner::State = ctx
                        .state_manager
                        .get_actor_state_from_address($ts, &miner.into())
                        .map_err(map_err)?;
                    let debt = miner_state.fee_debt();
                    if !debt.is_zero() {
                        // fee debt don't add the miner to power table
                        return Ok(());
                    }
                    let miner_info = miner_state.info(ctx.store()).map_err(map_err)?;
                    // check consensus faults
                    if $ts.epoch() <= miner_info.consensus_fault_elapsed {
                        return Ok(());
                    }
                    $id_power_worker_mappings.push((id, power, miner_info.worker.into()));
                    Ok(())
                })?;
            };
        }

        let tsk = f3_tsk.try_into()?;
        let ts = ctx.chain_index().load_required_tipset(&tsk)?;
        let state: power::State = ctx.state_manager.get_actor_state(&ts)?;
        let mut id_power_worker_mappings = vec![];
        match &state {
            power::State::V8(s) => {
                fn map_err<E: Display>(e: E) -> fil_actors_shared::v8::ActorError {
                    fil_actors_shared::v8::ActorError::unspecified(e.to_string())
                }

                let claims = fil_actors_shared::v8::make_map_with_root::<
                    _,
                    fil_actor_power_state::v8::Claim,
                >(&s.claims, ctx.store())?;
                claims.for_each(|key, claim| {
                    let miner = Address::from_bytes(key)?;
                    if !claim.quality_adj_power.is_positive() {
                        return Ok(());
                    }

                    let id = miner.id().map_err(map_err)?;
                    let ok = s.miner_nominal_power_meets_consensus_minimum(
                        &from_policy_v13_to_v9(&ctx.chain_config().policy),
                        ctx.store(),
                        &miner.into(),
                    )?;
                    if !ok {
                        return Ok(());
                    }
                    let power = claim.quality_adj_power.clone();
                    let miner_state: miner::State = ctx
                        .state_manager
                        .get_actor_state_from_address(&ts, &miner)
                        .map_err(map_err)?;
                    let debt = miner_state.fee_debt();
                    if !debt.is_zero() {
                        // fee debt don't add the miner to power table
                        return Ok(());
                    }
                    let miner_info = miner_state.info(ctx.store()).map_err(map_err)?;
                    // check consensus faults
                    if ts.epoch() <= miner_info.consensus_fault_elapsed {
                        return Ok(());
                    }
                    id_power_worker_mappings.push((id, power, miner_info.worker.into()));
                    Ok(())
                })?;
            }
            power::State::V9(s) => {
                fn map_err<E: Display>(e: E) -> fil_actors_shared::v9::ActorError {
                    fil_actors_shared::v9::ActorError::unspecified(e.to_string())
                }

                let claims = fil_actors_shared::v9::make_map_with_root::<
                    _,
                    fil_actor_power_state::v9::Claim,
                >(&s.claims, ctx.store())?;
                claims.for_each(|key, claim| {
                    let miner = Address::from_bytes(key)?;
                    if !claim.quality_adj_power.is_positive() {
                        return Ok(());
                    }

                    let id = miner.id().map_err(map_err)?;
                    let ok = s.miner_nominal_power_meets_consensus_minimum(
                        &from_policy_v13_to_v9(&ctx.chain_config().policy),
                        ctx.store(),
                        &miner.into(),
                    )?;
                    if !ok {
                        return Ok(());
                    }
                    let power = claim.quality_adj_power.clone();
                    let miner_state: miner::State = ctx
                        .state_manager
                        .get_actor_state_from_address(&ts, &miner)
                        .map_err(map_err)?;
                    let debt = miner_state.fee_debt();
                    if !debt.is_zero() {
                        // fee debt don't add the miner to power table
                        return Ok(());
                    }
                    let miner_info = miner_state.info(ctx.store()).map_err(map_err)?;
                    // check consensus faults
                    if ts.epoch() <= miner_info.consensus_fault_elapsed {
                        return Ok(());
                    }
                    id_power_worker_mappings.push((id, power, miner_info.worker.into()));
                    Ok(())
                })?;
            }
            power::State::V10(s) => {
                fn map_err<E: Display>(e: E) -> fil_actors_shared::v10::ActorError {
                    fil_actors_shared::v10::ActorError::unspecified(e.to_string())
                }

                let claims = fil_actors_shared::v10::make_map_with_root::<
                    _,
                    fil_actor_power_state::v10::Claim,
                >(&s.claims, ctx.store())?;
                claims.for_each(|key, claim| {
                    let miner = Address::from_bytes(key)?;
                    if !claim.quality_adj_power.is_positive() {
                        return Ok(());
                    }

                    let id = miner.id().map_err(map_err)?;
                    let (_, ok) = s.miner_nominal_power_meets_consensus_minimum(
                        &from_policy_v13_to_v10(&ctx.chain_config().policy),
                        ctx.store(),
                        id,
                    )?;
                    if !ok {
                        return Ok(());
                    }
                    let power = claim.quality_adj_power.clone();
                    let miner_state: miner::State = ctx
                        .state_manager
                        .get_actor_state_from_address(&ts, &miner)
                        .map_err(map_err)?;
                    let debt = miner_state.fee_debt();
                    if !debt.is_zero() {
                        // fee debt don't add the miner to power table
                        return Ok(());
                    }
                    let miner_info = miner_state.info(ctx.store()).map_err(map_err)?;
                    // check consensus faults
                    if ts.epoch() <= miner_info.consensus_fault_elapsed {
                        return Ok(());
                    }
                    id_power_worker_mappings.push((id, power, miner_info.worker.into()));
                    Ok(())
                })?;
            }
            power::State::V11(s) => {
                fn map_err<E: Display>(e: E) -> fil_actors_shared::v11::ActorError {
                    fil_actors_shared::v11::ActorError::unspecified(e.to_string())
                }

                let claims = fil_actors_shared::v11::make_map_with_root::<
                    _,
                    fil_actor_power_state::v11::Claim,
                >(&s.claims, ctx.store())?;
                claims.for_each(|key, claim| {
                    let miner = Address::from_bytes(key)?;
                    if !claim.quality_adj_power.is_positive() {
                        return Ok(());
                    }

                    let id = miner.id().map_err(map_err)?;
                    let (_, ok) = s.miner_nominal_power_meets_consensus_minimum(
                        &from_policy_v13_to_v11(&ctx.chain_config().policy),
                        ctx.store(),
                        id,
                    )?;
                    if !ok {
                        return Ok(());
                    }
                    let power = claim.quality_adj_power.clone();
                    let miner_state: miner::State = ctx
                        .state_manager
                        .get_actor_state_from_address(&ts, &miner)
                        .map_err(map_err)?;
                    let debt = miner_state.fee_debt();
                    if !debt.is_zero() {
                        // fee debt don't add the miner to power table
                        return Ok(());
                    }
                    let miner_info = miner_state.info(ctx.store()).map_err(map_err)?;
                    // check consensus faults
                    if ts.epoch() <= miner_info.consensus_fault_elapsed {
                        return Ok(());
                    }
                    id_power_worker_mappings.push((id, power, miner_info.worker.into()));
                    Ok(())
                })?;
            }
            power::State::V12(s) => {
                handle_miner_state_v12_on!(
                    v12,
                    id_power_worker_mappings,
                    &ts,
                    s,
                    &from_policy_v13_to_v12(&ctx.chain_config().policy)
                );
            }
            power::State::V13(s) => {
                handle_miner_state_v12_on!(
                    v13,
                    id_power_worker_mappings,
                    &ts,
                    s,
                    &ctx.chain_config().policy
                );
            }
            power::State::V14(s) => {
                handle_miner_state_v12_on!(
                    v14,
                    id_power_worker_mappings,
                    &ts,
                    s,
                    &from_policy_v13_to_v14(&ctx.chain_config().policy)
                );
            }
        }
        let mut power_entries = vec![];
        for (id, power, worker) in id_power_worker_mappings {
            let waddr = ctx
                .state_manager
                .resolve_to_deterministic_address(worker, ts.clone())
                .await?;
            if waddr.protocol() != Protocol::BLS {
                return Err(anyhow::anyhow!("wrong type of worker address").into());
            }
            let pub_key = waddr.payload_bytes();
            power_entries.push(F3PowerEntry { id, power, pub_key });
        }
        power_entries.sort();
        Ok(power_entries)
    }
}

pub enum ProtectPeer {}
impl RpcMethod<1> for ProtectPeer {
    const NAME: &'static str = "F3.ProtectPeer";
    const PARAM_NAMES: [&'static str; 1] = ["peer_id"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (String,);
    type Ok = bool;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (peer_id,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let peer_id = PeerId::from_str(&peer_id)?;
        let (tx, rx) = flume::bounded(1);
        ctx.network_send
            .send_async(NetworkMessage::JSONRPCRequest {
                method: NetRPCMethods::ProtectPeer(tx, std::iter::once(peer_id).collect()),
            })
            .await?;
        rx.recv_async().await?;
        Ok(true)
    }
}

pub enum GetParticipatingMinerIDs {}
impl RpcMethod<0> for GetParticipatingMinerIDs {
    const NAME: &'static str = "F3.GetParticipatingMinerIDs";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = Vec<u64>;

    async fn handle(_: Ctx<impl Blockstore>, _: Self::Params) -> Result<Self::Ok, ServerError> {
        let mut ids = F3_LEASE_MANAGER.get_active_participants();
        if let Some(permanent_miner_ids) = (*F3_PERMANENT_PARTICIPATING_MINER_IDS).clone() {
            ids.extend(permanent_miner_ids);
        }
        Ok(ids.into_iter().collect())
    }
}

pub enum SignMessage {}
impl RpcMethod<2> for SignMessage {
    const NAME: &'static str = "F3.SignMessage";
    const PARAM_NAMES: [&'static str; 2] = ["pubkey", "message"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (Vec<u8>, Vec<u8>);
    type Ok = Signature;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (pubkey, message): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let addr = Address::new_bls(&pubkey)?;
        // Signing can be delegated to curio, we will follow how lotus does it once the feature lands.
        WalletSign::handle(ctx, (addr, message)).await
    }
}

/// returns a finality certificate at given instance number
pub enum F3GetCertificate {}
impl RpcMethod<1> for F3GetCertificate {
    const NAME: &'static str = "Filecoin.F3GetCertificate";
    const PARAM_NAMES: [&'static str; 1] = ["instance"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (u64,);
    type Ok = serde_json::Value;

    async fn handle(
        _: Ctx<impl Blockstore>,
        (instance,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let client = get_rpc_http_client()?;
        let mut params = ArrayParams::new();
        params.insert(instance)?;
        let response = client.request(Self::NAME, params).await?;
        Ok(response)
    }
}

/// returns the latest finality certificate
pub enum F3GetLatestCertificate {}
impl RpcMethod<0> for F3GetLatestCertificate {
    const NAME: &'static str = "Filecoin.F3GetLatestCertificate";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = serde_json::Value;

    async fn handle(_: Ctx<impl Blockstore>, _: Self::Params) -> Result<Self::Ok, ServerError> {
        let client = get_rpc_http_client()?;
        let response = client.request(Self::NAME, ArrayParams::new()).await?;
        Ok(response)
    }
}

pub enum F3GetECPowerTable {}
impl RpcMethod<1> for F3GetECPowerTable {
    const NAME: &'static str = "Filecoin.F3GetECPowerTable";
    const PARAM_NAMES: [&'static str; 1] = ["tipset_key"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (F3TipSetKey,);
    type Ok = Vec<F3PowerEntry>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        params: Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        GetPowerTable::handle(ctx, params).await
    }
}

pub enum F3GetF3PowerTable {}
impl RpcMethod<1> for F3GetF3PowerTable {
    const NAME: &'static str = "Filecoin.F3GetF3PowerTable";
    const PARAM_NAMES: [&'static str; 1] = ["tipset_key"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (F3TipSetKey,);
    type Ok = serde_json::Value;

    async fn handle(
        _: Ctx<impl Blockstore>,
        (tsk,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let client = get_rpc_http_client()?;
        let mut params = ArrayParams::new();
        params.insert(tsk.into_lotus_json())?;
        let response = client.request(Self::NAME, params).await?;
        Ok(response)
    }
}

/// F3Participate should be called by a storage provider to participate in signing F3 consensus.
/// Calling this API gives the node a lease to sign in F3 on behalf of given SP.
/// The lease should be active only on one node. The lease will expire at the newLeaseExpiration.
/// To continue participating in F3 with the given node, call F3Participate again before the newLeaseExpiration time.
/// newLeaseExpiration cannot be further than 5 minutes in the future.
/// It is recommended to call F3Participate every 60 seconds with newLeaseExpiration set 2min into the future.
/// The oldLeaseExpiration has to be set to newLeaseExpiration of the last successful call.
/// For the first call to F3Participate, set the oldLeaseExpiration to zero value/time in the past.
/// F3Participate will return true if the lease was accepted. The minerID has to be the ID address of the miner.
pub enum F3Participate {}
impl RpcMethod<3> for F3Participate {
    const NAME: &'static str = "Filecoin.F3Participate";
    const PARAM_NAMES: [&'static str; 3] = [
        "miner_address",
        "new_lease_expiration",
        "old_lease_expiration",
    ];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Sign;

    type Params = (
        Address,
        chrono::DateTime<chrono::Utc>,
        chrono::DateTime<chrono::Utc>,
    );
    type Ok = bool;

    async fn handle(
        _: Ctx<impl Blockstore>,
        (miner, new_lease_expiration, old_lease_expiration): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        Ok(F3_LEASE_MANAGER.upsert_defensive(
            miner.id()?,
            new_lease_expiration,
            old_lease_expiration,
        )?)
    }
}

pub fn get_f3_rpc_endpoint() -> Cow<'static, str> {
    if let Ok(host) = std::env::var("FOREST_F3_SIDECAR_RPC_ENDPOINT") {
        Cow::Owned(host)
    } else {
        Cow::Borrowed("127.0.0.1:23456")
    }
}

fn get_rpc_http_client() -> anyhow::Result<jsonrpsee::http_client::HttpClient> {
    let client = jsonrpsee::http_client::HttpClientBuilder::new()
        .build(format!("http://{}", get_f3_rpc_endpoint()))?;
    Ok(client)
}
