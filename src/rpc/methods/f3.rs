// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//!
//! This module contains F3(fast finality) related V1 RPC methods
//! as well as some internal RPC methods(F3.*) that power
//! the go-f3 node in sidecar mode.
//!

mod types;
mod util;

pub use self::types::{
    F3InstanceProgress, F3LeaseManager, F3Manifest, F3PowerEntry, FinalityCertificate,
};
use self::{types::*, util::*};
use super::wallet::WalletSign;
use crate::shim::actors::{miner, power};
use crate::{
    blocks::Tipset,
    chain::index::ResolveNullTipset,
    chain_sync::TipsetValidator,
    db::{
        BlockstoreReadCacheStats as _, BlockstoreWithReadCache, DefaultBlockstoreReadCacheStats,
        LruBlockstoreReadCache,
    },
    libp2p::{NetRPCMethods, NetworkMessage},
    lotus_json::{HasLotusJson as _, LotusJson},
    rpc::{ApiPaths, Ctx, Permission, RpcMethod, ServerError, types::ApiTipsetKey},
    shim::{
        address::{Address, Protocol},
        clock::ChainEpoch,
        crypto::Signature,
    },
    utils::misc::env::is_env_set_and_truthy,
};
use ahash::{HashMap, HashSet};
use anyhow::Context as _;
use cid::Cid;
use enumflags2::BitFlags;
use fvm_ipld_blockstore::Blockstore;
use jsonrpsee::core::{client::ClientT as _, params::ArrayParams};
use libp2p::PeerId;
use num::Signed as _;
use parking_lot::RwLock;
use std::{
    borrow::Cow,
    fmt::Display,
    str::FromStr as _,
    sync::{Arc, LazyLock, OnceLock},
};

pub static F3_LEASE_MANAGER: OnceLock<F3LeaseManager> = OnceLock::new();

pub enum GetRawNetworkName {}

impl RpcMethod<0> for GetRawNetworkName {
    const NAME: &'static str = "F3.GetRawNetworkName";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = String;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(ctx.chain_config().network.genesis_name().into())
    }
}

pub enum GetTipsetByEpoch {}
impl RpcMethod<1> for GetTipsetByEpoch {
    const NAME: &'static str = "F3.GetTipsetByEpoch";
    const PARAM_NAMES: [&'static str; 1] = ["epoch"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (ChainEpoch,);
    type Ok = F3TipSet;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (epoch,): Self::Params,
        _: &http::Extensions,
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
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (F3TipSetKey,);
    type Ok = F3TipSet;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (f3_tsk,): Self::Params,
        _: &http::Extensions,
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
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = F3TipSet;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        _: Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(ctx.chain_store().heaviest_tipset().into())
    }
}

pub enum GetParent {}
impl RpcMethod<1> for GetParent {
    const NAME: &'static str = "F3.GetParent";
    const PARAM_NAMES: [&'static str; 1] = ["tipset_key"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (F3TipSetKey,);
    type Ok = F3TipSet;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (f3_tsk,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let tsk = f3_tsk.try_into()?;
        let ts = ctx.chain_index().load_required_tipset(&tsk)?;
        let parent = ctx.chain_index().load_required_tipset(ts.parents())?;
        Ok(parent.into())
    }
}

pub enum GetPowerTable {}

impl GetPowerTable {
    async fn compute(
        ctx: &Ctx<impl Blockstore + Send + Sync + 'static>,
        ts: &Tipset,
    ) -> anyhow::Result<Vec<F3PowerEntry>> {
        // The RAM overhead on mainnet is ~14MiB
        const BLOCKSTORE_CACHE_CAP: usize = 65536;
        static BLOCKSTORE_CACHE: LazyLock<LruBlockstoreReadCache> = LazyLock::new(|| {
            LruBlockstoreReadCache::new_with_metrics(
                "get_powertable".into(),
                BLOCKSTORE_CACHE_CAP.try_into().expect("Infallible"),
            )
        });
        let db = BlockstoreWithReadCache::new(
            ctx.store_owned(),
            BLOCKSTORE_CACHE.clone(),
            Some(DefaultBlockstoreReadCacheStats::default()),
        );

        macro_rules! handle_miner_state_v12_on {
            ($version:tt, $id_power_worker_mappings:ident, $ts:expr, $state:expr, $policy:expr) => {
                fn map_err<E: Display>(e: E) -> fil_actors_shared::$version::ActorError {
                    fil_actors_shared::$version::ActorError::unspecified(e.to_string())
                }

                let claims = $state.load_claims(&db)?;
                claims.for_each(|miner, claim| {
                    if !claim.quality_adj_power.is_positive() {
                        return Ok(());
                    }

                    let id = miner.id().map_err(map_err)?;
                    let (_, ok) =
                        $state.miner_nominal_power_meets_consensus_minimum($policy, &db, id)?;
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
                    let miner_info = miner_state.info(&db).map_err(map_err)?;
                    // check consensus faults
                    if $ts.epoch() <= miner_info.consensus_fault_elapsed {
                        return Ok(());
                    }
                    $id_power_worker_mappings.push((id, power, miner_info.worker.into()));
                    Ok(())
                })?;
            };
        }

        let state: power::State = ctx.state_manager.get_actor_state(ts)?;
        let mut id_power_worker_mappings = vec![];
        let policy = &ctx.chain_config().policy;
        match &state {
            power::State::V8(s) => {
                fn map_err<E: Display>(e: E) -> fil_actors_shared::v8::ActorError {
                    fil_actors_shared::v8::ActorError::unspecified(e.to_string())
                }

                let claims = fil_actors_shared::v8::make_map_with_root::<
                    _,
                    fil_actor_power_state::v8::Claim,
                >(&s.claims, &db)?;
                claims.for_each(|key, claim| {
                    let miner = Address::from_bytes(key)?;
                    if !claim.quality_adj_power.is_positive() {
                        return Ok(());
                    }

                    let id = miner.id().map_err(map_err)?;
                    let ok = s.miner_nominal_power_meets_consensus_minimum(
                        &policy.into(),
                        &db,
                        &miner.into(),
                    )?;
                    if !ok {
                        return Ok(());
                    }
                    let power = claim.quality_adj_power.clone();
                    let miner_state: miner::State = ctx
                        .state_manager
                        .get_actor_state_from_address(ts, &miner)
                        .map_err(map_err)?;
                    let debt = miner_state.fee_debt();
                    if !debt.is_zero() {
                        // fee debt don't add the miner to power table
                        return Ok(());
                    }
                    let miner_info = miner_state.info(&db).map_err(map_err)?;
                    // check consensus faults
                    if ts.epoch() <= miner_info.consensus_fault_elapsed {
                        return Ok(());
                    }
                    id_power_worker_mappings.push((id, power, miner_info.worker));
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
                >(&s.claims, &db)?;
                claims.for_each(|key, claim| {
                    let miner = Address::from_bytes(key)?;
                    if !claim.quality_adj_power.is_positive() {
                        return Ok(());
                    }

                    let id = miner.id().map_err(map_err)?;
                    let ok = s.miner_nominal_power_meets_consensus_minimum(
                        &policy.into(),
                        &db,
                        &miner.into(),
                    )?;
                    if !ok {
                        return Ok(());
                    }
                    let power = claim.quality_adj_power.clone();
                    let miner_state: miner::State = ctx
                        .state_manager
                        .get_actor_state_from_address(ts, &miner)
                        .map_err(map_err)?;
                    let debt = miner_state.fee_debt();
                    if !debt.is_zero() {
                        // fee debt don't add the miner to power table
                        return Ok(());
                    }
                    let miner_info = miner_state.info(&db).map_err(map_err)?;
                    // check consensus faults
                    if ts.epoch() <= miner_info.consensus_fault_elapsed {
                        return Ok(());
                    }
                    id_power_worker_mappings.push((id, power, miner_info.worker));
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
                >(&s.claims, &db)?;
                claims.for_each(|key, claim| {
                    let miner = Address::from_bytes(key)?;
                    if !claim.quality_adj_power.is_positive() {
                        return Ok(());
                    }

                    let id = miner.id().map_err(map_err)?;
                    let (_, ok) =
                        s.miner_nominal_power_meets_consensus_minimum(&policy.into(), &db, id)?;
                    if !ok {
                        return Ok(());
                    }
                    let power = claim.quality_adj_power.clone();
                    let miner_state: miner::State = ctx
                        .state_manager
                        .get_actor_state_from_address(ts, &miner)
                        .map_err(map_err)?;
                    let debt = miner_state.fee_debt();
                    if !debt.is_zero() {
                        // fee debt don't add the miner to power table
                        return Ok(());
                    }
                    let miner_info = miner_state.info(&db).map_err(map_err)?;
                    // check consensus faults
                    if ts.epoch() <= miner_info.consensus_fault_elapsed {
                        return Ok(());
                    }
                    id_power_worker_mappings.push((id, power, miner_info.worker));
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
                >(&s.claims, &db)?;
                claims.for_each(|key, claim| {
                    let miner = Address::from_bytes(key)?;
                    if !claim.quality_adj_power.is_positive() {
                        return Ok(());
                    }

                    let id = miner.id().map_err(map_err)?;
                    let (_, ok) =
                        s.miner_nominal_power_meets_consensus_minimum(&policy.into(), &db, id)?;
                    if !ok {
                        return Ok(());
                    }
                    let power = claim.quality_adj_power.clone();
                    let miner_state: miner::State = ctx
                        .state_manager
                        .get_actor_state_from_address(ts, &miner)
                        .map_err(map_err)?;
                    let debt = miner_state.fee_debt();
                    if !debt.is_zero() {
                        // fee debt don't add the miner to power table
                        return Ok(());
                    }
                    let miner_info = miner_state.info(&db).map_err(map_err)?;
                    // check consensus faults
                    if ts.epoch() <= miner_info.consensus_fault_elapsed {
                        return Ok(());
                    }
                    id_power_worker_mappings.push((id, power, miner_info.worker));
                    Ok(())
                })?;
            }
            power::State::V12(s) => {
                handle_miner_state_v12_on!(v12, id_power_worker_mappings, &ts, s, &policy.into());
            }
            power::State::V13(s) => {
                handle_miner_state_v12_on!(v13, id_power_worker_mappings, &ts, s, &policy.into());
            }
            power::State::V14(s) => {
                handle_miner_state_v12_on!(v14, id_power_worker_mappings, &ts, s, &policy.into());
            }
            power::State::V15(s) => {
                handle_miner_state_v12_on!(v15, id_power_worker_mappings, &ts, s, &policy.into());
            }
            power::State::V16(s) => {
                handle_miner_state_v12_on!(v16, id_power_worker_mappings, &ts, s, &policy.into());
            }
            power::State::V17(s) => {
                handle_miner_state_v12_on!(v17, id_power_worker_mappings, &ts, s, &policy.into());
            }
        }
        let mut power_entries = vec![];
        for (id, power, worker) in id_power_worker_mappings {
            let waddr = ctx
                .state_manager
                .resolve_to_deterministic_address(worker, ts)
                .await?;
            if waddr.protocol() != Protocol::BLS {
                anyhow::bail!("wrong type of worker address");
            }
            let pub_key = waddr.payload_bytes();
            power_entries.push(F3PowerEntry { id, power, pub_key });
        }
        power_entries.sort();

        if let Some(stats) = db.stats() {
            tracing::debug!(epoch=%ts.epoch(), hit=%stats.hit(), miss=%stats.miss(),cache_len=%BLOCKSTORE_CACHE.len(), "F3.GetPowerTable blockstore read cache");
        }

        Ok(power_entries)
    }
}

impl RpcMethod<1> for GetPowerTable {
    const NAME: &'static str = "F3.GetPowerTable";
    const PARAM_NAMES: [&'static str; 1] = ["tipset_key"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (F3TipSetKey,);
    type Ok = Vec<F3PowerEntry>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (f3_tsk,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let tsk = f3_tsk.try_into()?;
        let start = std::time::Instant::now();
        let ts = ctx.chain_index().load_required_tipset(&tsk)?;
        let power_entries = Self::compute(&ctx, &ts).await?;
        tracing::debug!(epoch=%ts.epoch(), %tsk, "F3.GetPowerTable, took {}", humantime::format_duration(start.elapsed()));
        Ok(power_entries)
    }
}

pub enum ProtectPeer {}
impl RpcMethod<1> for ProtectPeer {
    const NAME: &'static str = "F3.ProtectPeer";
    const PARAM_NAMES: [&'static str; 1] = ["peer_id"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (String,);
    type Ok = bool;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (peer_id,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let peer_id = PeerId::from_str(&peer_id)?;
        let (tx, rx) = flume::bounded(1);
        ctx.network_send()
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
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = Vec<u64>;

    async fn handle(
        _: Ctx<impl Blockstore>,
        _: Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let participants = F3ListParticipants::run().await?;
        let mut ids: HashSet<u64> = participants.into_iter().map(|p| p.miner_id).collect();
        if let Some(permanent_miner_ids) = (*F3_PERMANENT_PARTICIPATING_MINER_IDS).clone() {
            ids.extend(permanent_miner_ids);
        }
        Ok(ids.into_iter().collect())
    }
}

pub enum Finalize {}
impl RpcMethod<1> for Finalize {
    const NAME: &'static str = "F3.Finalize";
    const PARAM_NAMES: [&'static str; 1] = ["tipset_key"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Write;

    type Params = (F3TipSetKey,);
    type Ok = ();

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (f3_tsk,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        // Respect the environment variable when set, and fallback to chain config when not set.
        let enabled = is_env_set_and_truthy("FOREST_F3_CONSENSUS_ENABLED")
            .unwrap_or(ctx.chain_config().f3_consensus);
        if !enabled {
            return Ok(());
        }

        let tsk = f3_tsk.try_into()?;
        let finalized_ts = match ctx.chain_index().load_tipset(&tsk)? {
            Some(ts) => ts,
            None => ctx
                .sync_network_context
                .chain_exchange_headers(None, &tsk, 1.try_into().expect("Infallible"))
                .await?
                .first()
                .cloned()
                .with_context(|| format!("failed to get tipset via chain exchange. tsk: {tsk}"))?,
        };
        let head = ctx.chain_store().heaviest_tipset();
        // When finalized_ts is not part of the current chain,
        // reset the current head to finalized_ts.
        // Note that when finalized_ts is newer than head or older than head - chain_finality,
        // we don't reset the head to allow the chain or F3 to catch up.
        if head.epoch() >= finalized_ts.epoch()
            && head.epoch() <= finalized_ts.epoch() + ctx.chain_config().policy.chain_finality
        {
            tracing::debug!(
                "F3 finalized tsk {} at epoch {}",
                finalized_ts.key(),
                finalized_ts.epoch()
            );
            if !head
                .chain(ctx.store())
                .take_while(|ts| ts.epoch() >= finalized_ts.epoch())
                .any(|ts| ts == finalized_ts)
            {
                tracing::info!(
                    "F3 reset chain head to tsk {} at epoch {}",
                    finalized_ts.key(),
                    finalized_ts.epoch()
                );
                let fts = ctx
                    .sync_network_context
                    .chain_exchange_full_tipset(None, &tsk)
                    .await?;
                fts.persist(ctx.store())?;
                let validator = TipsetValidator(&fts);
                validator.validate(
                    ctx.chain_store(),
                    None,
                    &ctx.chain_store().genesis_tipset(),
                    ctx.chain_config().block_delay_secs,
                )?;
                let ts = Arc::new(Tipset::from(fts));
                ctx.chain_store().put_tipset(&ts)?;
                ctx.chain_store().set_heaviest_tipset(finalized_ts)?;
            }
        }
        Ok(())
    }
}

pub enum SignMessage {}
impl RpcMethod<2> for SignMessage {
    const NAME: &'static str = "F3.SignMessage";
    const PARAM_NAMES: [&'static str; 2] = ["pubkey", "message"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Sign;

    type Params = (Vec<u8>, Vec<u8>);
    type Ok = Signature;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (pubkey, message): Self::Params,
        ext: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let addr = Address::new_bls(&pubkey)?;
        // Signing can be delegated to curio, we will follow how lotus does it once the feature lands.
        WalletSign::handle(ctx, (addr, message), ext).await
    }
}

pub enum F3ExportLatestSnapshot {}

impl F3ExportLatestSnapshot {
    pub async fn run(path: String) -> anyhow::Result<Cid> {
        let client = get_rpc_http_client()?;
        let mut params = ArrayParams::new();
        params.insert(path)?;
        let LotusJson(cid): LotusJson<Cid> = client
            .request("Filecoin.F3ExportLatestSnapshot", params)
            .await?;
        Ok(cid)
    }
}

impl RpcMethod<1> for F3ExportLatestSnapshot {
    const NAME: &'static str = "F3.ExportLatestSnapshot";
    const PARAM_NAMES: [&'static str; 1] = ["path"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Exports the latest F3 snapshot to the specified path and returns its CID");

    type Params = (String,);
    type Ok = Cid;

    async fn handle(
        _ctx: Ctx<impl Blockstore>,
        (path,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(Self::run(path).await?)
    }
}

/// returns a finality certificate at given instance number
pub enum F3GetCertificate {}
impl RpcMethod<1> for F3GetCertificate {
    const NAME: &'static str = "Filecoin.F3GetCertificate";
    const PARAM_NAMES: [&'static str; 1] = ["instance"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (u64,);
    type Ok = FinalityCertificate;

    async fn handle(
        _: Ctx<impl Blockstore>,
        (instance,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let client = get_rpc_http_client()?;
        let mut params = ArrayParams::new();
        params.insert(instance)?;
        let response: LotusJson<Self::Ok> = client.request(Self::NAME, params).await?;
        Ok(response.into_inner())
    }
}

/// returns the latest finality certificate
pub enum F3GetLatestCertificate {}

impl F3GetLatestCertificate {
    /// Fetches the latest finality certificate via RPC.
    pub async fn get() -> anyhow::Result<FinalityCertificate> {
        let client = get_rpc_http_client()?;
        let response: LotusJson<FinalityCertificate> = client
            .request(<Self as RpcMethod<0>>::NAME, ArrayParams::new())
            .await?;
        Ok(response.into_inner())
    }
}

impl RpcMethod<0> for F3GetLatestCertificate {
    const NAME: &'static str = "Filecoin.F3GetLatestCertificate";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = FinalityCertificate;

    async fn handle(
        _: Ctx<impl Blockstore + Send + Sync + 'static>,
        _: Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(Self::get().await?)
    }
}

pub enum F3GetECPowerTable {}
impl RpcMethod<1> for F3GetECPowerTable {
    const NAME: &'static str = "Filecoin.F3GetECPowerTable";
    const PARAM_NAMES: [&'static str; 1] = ["tipset_key"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (ApiTipsetKey,);
    type Ok = Vec<F3PowerEntry>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (ApiTipsetKey(tsk_opt),): Self::Params,
        ext: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let tsk = tsk_opt.unwrap_or_else(|| ctx.chain_store().heaviest_tipset().key().clone());
        GetPowerTable::handle(ctx, (tsk.into(),), ext).await
    }
}

pub enum F3GetF3PowerTable {}
impl RpcMethod<1> for F3GetF3PowerTable {
    const NAME: &'static str = "Filecoin.F3GetF3PowerTable";
    const PARAM_NAMES: [&'static str; 1] = ["tipset_key"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (ApiTipsetKey,);
    type Ok = Vec<F3PowerEntry>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (ApiTipsetKey(tsk_opt),): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let tsk: F3TipSetKey = tsk_opt
            .unwrap_or_else(|| ctx.chain_store().heaviest_tipset().key().clone())
            .into();
        let client = get_rpc_http_client()?;
        let mut params = ArrayParams::new();
        params.insert(tsk.into_lotus_json())?;
        let response: LotusJson<Self::Ok> = client.request(Self::NAME, params).await?;
        Ok(response.into_inner())
    }
}

pub enum F3GetF3PowerTableByInstance {}
impl RpcMethod<1> for F3GetF3PowerTableByInstance {
    const NAME: &'static str = "Filecoin.F3GetF3PowerTableByInstance";
    const PARAM_NAMES: [&'static str; 1] = ["instance"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Gets the power table (committee) used to validate the specified instance");

    type Params = (u64,);
    type Ok = Vec<F3PowerEntry>;

    async fn handle(
        _ctx: Ctx<impl Blockstore>,
        (instance,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let client = get_rpc_http_client()?;
        let mut params = ArrayParams::new();
        params.insert(instance)?;
        let response: LotusJson<Self::Ok> = client.request(Self::NAME, params).await?;
        Ok(response.into_inner())
    }
}

pub enum F3IsRunning {}

impl F3IsRunning {
    pub async fn is_f3_running() -> anyhow::Result<bool> {
        let client = get_rpc_http_client()?;
        let response = client.request(Self::NAME, ArrayParams::new()).await?;
        Ok(response)
    }
}

impl RpcMethod<0> for F3IsRunning {
    const NAME: &'static str = "Filecoin.F3IsRunning";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = bool;

    async fn handle(
        _: Ctx<impl Blockstore>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(Self::is_f3_running().await?)
    }
}

/// See <https://github.com/filecoin-project/lotus/blob/master/documentation/en/api-methods-v1-stable.md#F3GetProgress>
pub enum F3GetProgress {}

impl F3GetProgress {
    async fn run() -> anyhow::Result<F3InstanceProgress> {
        let client = get_rpc_http_client()?;
        let response: LotusJson<F3InstanceProgress> =
            client.request(Self::NAME, ArrayParams::new()).await?;
        Ok(response.into_inner())
    }
}

impl RpcMethod<0> for F3GetProgress {
    const NAME: &'static str = "Filecoin.F3GetProgress";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = F3InstanceProgress;

    async fn handle(
        _: Ctx<impl Blockstore>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(Self::run().await?)
    }
}

/// See <https://github.com/filecoin-project/lotus/blob/master/documentation/en/api-methods-v1-stable.md#F3GetManifest>
pub enum F3GetManifest {}

impl F3GetManifest {
    async fn run() -> anyhow::Result<F3Manifest> {
        let client = get_rpc_http_client()?;
        let response: LotusJson<F3Manifest> =
            client.request(Self::NAME, ArrayParams::new()).await?;
        Ok(response.into_inner())
    }
}

impl RpcMethod<0> for F3GetManifest {
    const NAME: &'static str = "Filecoin.F3GetManifest";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = F3Manifest;

    async fn handle(
        _: Ctx<impl Blockstore>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(Self::run().await?)
    }
}

/// returns the list of miner addresses that are currently participating in F3 via this node.
pub enum F3ListParticipants {}
impl RpcMethod<0> for F3ListParticipants {
    const NAME: &'static str = "Filecoin.F3ListParticipants";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = Vec<F3Participant>;

    async fn handle(
        _: Ctx<impl Blockstore>,
        _: Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(Self::run().await?)
    }
}

impl F3ListParticipants {
    async fn run() -> anyhow::Result<Vec<F3Participant>> {
        let current_instance = F3GetProgress::run().await?.id;
        Ok(F3_LEASE_MANAGER
            .get()
            .context("F3 lease manager is not initialized")?
            .get_active_participants(current_instance)
            .values()
            .map(F3Participant::from)
            .collect())
    }
}

/// retrieves or renews a participation ticket necessary for a miner to engage in
/// the F3 consensus process for the given number of instances.
pub enum F3GetOrRenewParticipationTicket {}
impl RpcMethod<3> for F3GetOrRenewParticipationTicket {
    const NAME: &'static str = "Filecoin.F3GetOrRenewParticipationTicket";
    const PARAM_NAMES: [&'static str; 3] = ["miner_address", "previous_lease_ticket", "instances"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Sign;

    type Params = (Address, Vec<u8>, u64);
    type Ok = Vec<u8>;

    async fn handle(
        _: Ctx<impl Blockstore>,
        (miner, previous_lease_ticket, instances): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let id = miner.id()?;
        let previous_lease = if previous_lease_ticket.is_empty() {
            None
        } else {
            Some(
                fvm_ipld_encoding::from_slice::<F3ParticipationLease>(&previous_lease_ticket)
                    .context("the previous lease ticket is invalid")?,
            )
        };
        let lease = F3_LEASE_MANAGER
            .get()
            .context("F3 lease manager is not initialized")?
            .get_or_renew_participation_lease(id, previous_lease, instances)
            .await?;
        Ok(fvm_ipld_encoding::to_vec(&lease)?)
    }
}

/// enrolls a storage provider in the F3 consensus process using a
/// provided participation ticket. This ticket grants a temporary lease that enables
/// the provider to sign transactions as part of the F3 consensus.
pub enum F3Participate {}
impl RpcMethod<1> for F3Participate {
    const NAME: &'static str = "Filecoin.F3Participate";
    const PARAM_NAMES: [&'static str; 1] = ["lease_ticket"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Sign;

    type Params = (Vec<u8>,);
    type Ok = F3ParticipationLease;

    async fn handle(
        _: Ctx<impl Blockstore>,
        (lease_ticket,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let lease: F3ParticipationLease =
            fvm_ipld_encoding::from_slice(&lease_ticket).context("invalid lease ticket")?;
        let current_instance = F3GetProgress::run().await?.id;
        F3_LEASE_MANAGER
            .get()
            .context("F3 lease manager is not initialized")?
            .participate(&lease, current_instance)?;
        Ok(lease)
    }
}

pub fn get_f3_rpc_endpoint() -> Cow<'static, str> {
    if let Ok(host) = std::env::var("FOREST_F3_SIDECAR_RPC_ENDPOINT") {
        Cow::Owned(host)
    } else {
        Cow::Borrowed("127.0.0.1:23456")
    }
}

pub fn get_rpc_http_client() -> anyhow::Result<jsonrpsee::http_client::HttpClient> {
    let client = jsonrpsee::http_client::HttpClientBuilder::new()
        .build(format!("http://{}", get_f3_rpc_endpoint()))?;
    Ok(client)
}
