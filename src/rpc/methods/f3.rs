// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//!
//! This module contains F3(fast finality) related V1 RPC methods
//! as well as some internal RPC methods(F3.*) that power
//! the go-f3 node in sidecar mode.
//!

mod types;

use self::types::*;
use crate::{
    chain::index::ResolveNullTipset,
    libp2p::{NetRPCMethods, NetworkMessage},
    rpc::{ApiPaths, Ctx, Permission, RpcMethod, ServerError},
    shim::{
        address::{Address, Protocol},
        clock::ChainEpoch,
        crypto::Signature,
    },
};
use fil_actor_interface::{
    convert::{
        from_policy_v13_to_v10, from_policy_v13_to_v11, from_policy_v13_to_v12,
        from_policy_v13_to_v14, from_policy_v13_to_v9,
    },
    miner, power,
};
use fvm_ipld_blockstore::Blockstore;
use libp2p::PeerId;
use num::Signed as _;
use std::{fmt::Display, str::FromStr as _, sync::Arc};

use super::wallet::WalletSign;

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
                method: NetRPCMethods::ProtectPeer(tx, peer_id),
            })
            .await?;
        rx.recv_async().await?;
        Ok(true)
    }
}

pub enum GetParticipatedMinerIDs {}
impl RpcMethod<0> for GetParticipatedMinerIDs {
    const NAME: &'static str = "F3.GetParticipatedMinerIDs";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = Vec<u64>;

    async fn handle(_ctx: Ctx<impl Blockstore>, _: Self::Params) -> Result<Self::Ok, ServerError> {
        // For now, just hard code the shared miner for testing
        let shared_miner_addr = Address::from_str("t0111551")?;
        Ok(vec![shared_miner_addr.id()?])
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
