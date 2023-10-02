// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_api::{data_types::RPCState, db_api::*};
use fvm_ipld_blockstore::Blockstore;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};

pub(in crate::rpc) async fn db_gc<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(_): Params<DBGCParams>,
) -> Result<DBGCResult, JsonRpcError> {
    if !data.manual_gc {
        // There might be a more suitable error type for this.
        return Err(JsonRpcError::internal("manual garbage collection disabled"));
    }
    let (tx, rx) = flume::bounded(1);
    data.gc_event_tx.send_async(tx).await?;
    rx.recv_async().await??;
    Ok(())
}
