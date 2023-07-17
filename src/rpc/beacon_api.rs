// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::beacon::{json::BeaconEntryJson, Beacon};
use crate::rpc_api::{beacon_api::*, data_types::RPCState};
use fvm_ipld_blockstore::Blockstore;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};

/// `BeaconGetEntry` returns the beacon entry for the given Filecoin epoch. If
/// the entry has not yet been produced, the call will block until the entry
/// becomes available
pub(in crate::rpc) async fn beacon_get_entry<DB>(
    data: Data<RPCState<DB>>,
    Params(params): Params<BeaconGetEntryParams>,
) -> Result<BeaconGetEntryResult, JsonRpcError>
where
    DB: Blockstore + Clone + Send + Sync + 'static,
{
    let (first,) = params;
    let (_, beacon) = data.beacon.beacon_for_epoch(first)?;
    let rr =
        beacon.max_beacon_round_for_epoch(data.state_manager.get_network_version(first), first);
    let e = beacon.entry(rr).await?;
    Ok(BeaconEntryJson(e))
}
