// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use jsonrpc_v2::{Data, Error as JsonRpcError, Params};

use beacon::json::BeaconEntryJson;
use beacon::Beacon;
use blockstore::BlockStore;
use rpc_api::beacon_api::*;
use rpc_api::data_types::RPCState;

/// BeaconGetEntry returns the beacon entry for the given filecoin epoch. If
/// the entry has not yet been produced, the call will block until the entry
/// becomes available
pub(crate) async fn beacon_get_entry<'a, DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<BeaconGetEntryParams>,
) -> Result<BeaconGetEntryResult, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (first,) = params;
    let (_, beacon) = data.beacon.beacon_for_epoch(first)?;
    let rr = beacon.max_beacon_round_for_epoch(first);
    let e = beacon.entry(rr).await?;
    Ok(BeaconEntryJson(e))
}
