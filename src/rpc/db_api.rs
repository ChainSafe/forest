// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::beacon::Beacon;
use crate::blocks::tipset_keys_json::TipsetKeysJson;
use crate::chain::DBDump;
use crate::db::Dump;
use crate::rpc_api::{data_types::RPCState, db_api::*};
use fvm_ipld_blockstore::Blockstore;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use once_cell::sync::Lazy;
use tempfile::NamedTempFile;
use tokio::sync::Mutex;
use tokio_util::compat::TokioAsyncReadCompatExt;

pub(in crate::rpc) async fn db_gc<DB: Blockstore + Clone + Send + Sync + 'static, B: Beacon>(
    data: Data<RPCState<DB, B>>,
    Params(_): Params<DBGCParams>,
) -> Result<DBGCResult, JsonRpcError> {
    let (tx, rx) = flume::bounded(1);
    data.gc_event_tx.send_async(tx).await?;
    rx.recv_async().await??;
    Ok(())
}

pub(in crate::rpc) async fn db_dump<DB: Blockstore + Clone + Send + Sync + 'static, B: Beacon>(
    data: Data<RPCState<DB, B>>,
    Params(DBDumpParams {
        epoch,
        output_path,
        tipset_keys: TipsetKeysJson(tsk),
        compression,
    }): Params<DBDumpParams>,
) -> Result<DBDumpResult, JsonRpcError>
where
    DB: Dump,
{
    static LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    let _locked = LOCK.try_lock();
    if _locked.is_err() {
        return Err(JsonRpcError::Provided {
            code: http::StatusCode::SERVICE_UNAVAILABLE.as_u16() as _,
            message: "Another db dump job is in progress",
        });
    }

    let output_dir = output_path.parent().ok_or_else(|| JsonRpcError::Provided {
        code: http::StatusCode::INTERNAL_SERVER_ERROR.as_u16() as _,
        message: "Failed to determine snapshot export directory",
    })?;
    let temp_path = NamedTempFile::new_in(output_dir)?.into_temp_path();
    let head = data.chain_store.tipset_from_keys(&tsk)?;
    let start_ts = data.chain_store.tipset_by_height(epoch, head, true)?;
    let file = tokio::fs::File::create(&temp_path).await?;
    data.chain_store
        .dump_db_as_car(file.compat(), &start_ts, compression)
        .await?;

    temp_path.persist(output_path)?;

    Ok(())
}
