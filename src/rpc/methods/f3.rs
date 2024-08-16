// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//!
//! This module contains F3(fast finality) related V1 RPC methods
//! as well as some internal RPC methods(F3.*) that power
//! the go-f3 node in sidecar mode.
//!

mod types;
use std::sync::Arc;

use types::*;

use crate::{
    chain::index::ResolveNullTipset,
    rpc::{ApiPaths, Ctx, Permission, RpcMethod, ServerError},
    shim::clock::ChainEpoch,
};
use fvm_ipld_blockstore::Blockstore;

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
        Ok(Arc::unwrap_or_clone(ts).into())
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
        (tsk,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        unimplemented!()
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
        unimplemented!()
    }
}

pub enum GetParent {}
impl RpcMethod<1> for GetParent {
    const NAME: &'static str = "F3.GetParent";
    const PARAM_NAMES: [&'static str; 1] = ["tipset"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (F3TipSet,);
    type Ok = F3TipSet;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (ts,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        unimplemented!()
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
        ctx: Ctx<impl Blockstore>,
        (ts,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        unimplemented!()
    }
}
