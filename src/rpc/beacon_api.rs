// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::redundant_allocation)]

use crate::rpc::error::JsonRpcError;
use crate::{
    beacon::BeaconEntry, lotus_json::LotusJson, rpc_api::data_types::RPCState,
    shim::clock::ChainEpoch,
};
use anyhow::Result;
use fvm_ipld_blockstore::Blockstore;
use jsonrpsee::types::Params as JsonRpseeParams;

use std::sync::Arc;

/// `BeaconGetEntry` returns the beacon entry for the given Filecoin epoch. If
/// the entry has not yet been produced, the call will block until the entry
/// becomes available
pub async fn beacon_get_entry<DB: Blockstore>(
    params: JsonRpseeParams<'_>,
    data: Arc<Arc<RPCState<DB>>>,
) -> Result<LotusJson<BeaconEntry>, JsonRpcError> {
    let (first,): (ChainEpoch,) = params.parse()?;

    let (_, beacon) = data.beacon.beacon_for_epoch(first)?;
    let rr =
        beacon.max_beacon_round_for_epoch(data.state_manager.get_network_version(first), first);
    let e = beacon.entry(rr).await?;
    Ok(e.into())
}
