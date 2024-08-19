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
    rpc::{ApiPaths, Ctx, Permission, RpcMethod, ServerError},
    shim::{address::Protocol, clock::ChainEpoch},
};
use fil_actor_interface::{
    convert::{from_policy_v13_to_v12, from_policy_v13_to_v14},
    miner, power,
};
use fvm_ipld_blockstore::Blockstore;
use num::Signed as _;
use std::{fmt::Display, sync::Arc};

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
            _ => unimplemented!("v8-v11 support is to be implemented."),
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
