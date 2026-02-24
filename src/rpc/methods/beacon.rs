// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::{ApiPaths, Ctx, Permission, RpcMethod, ServerError};
use crate::{beacon::BeaconEntry, shim::clock::ChainEpoch};
use anyhow::Result;
use enumflags2::{BitFlags, make_bitflags};
use fvm_ipld_blockstore::Blockstore;

/// `BeaconGetEntry` returns the beacon entry for the given Filecoin epoch. If
/// the entry has not yet been produced, the call will block until the entry
/// becomes available
pub enum BeaconGetEntry {}
impl RpcMethod<1> for BeaconGetEntry {
    const NAME: &'static str = "Filecoin.BeaconGetEntry";
    const PARAM_NAMES: [&'static str; 1] = ["first"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::V0); // Not supported in V1
    const PERMISSION: Permission = Permission::Read;

    type Params = (ChainEpoch,);
    type Ok = BeaconEntry;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (first,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let (_, beacon) = ctx.beacon().beacon_for_epoch(first)?;
        let rr =
            beacon.max_beacon_round_for_epoch(ctx.state_manager.get_network_version(first), first);
        let e = beacon.entry(rr).await?;
        Ok(e)
    }
}
