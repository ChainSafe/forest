// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(clippy::unused_async)]

use std::sync::atomic;

use crate::db::parity_db::DATABASE_DUMP_PROGRESS;
use crate::ipld::{WALK_SNAPSHOT_PROGRESS_DB_GC, WALK_SNAPSHOT_PROGRESS_EXPORT};
use crate::rpc_api::progress_api::{GetProgressParams, GetProgressResult, GetProgressType};

use crate::rpc::*;
use crate::utils::io::progress_bar::ProgressBarCurrentTotalPair;

pub(in crate::rpc) async fn get_progress(
    Params((typ,)): Params<GetProgressParams>,
) -> RpcResult<GetProgressResult> {
    let tracker: &ProgressBarCurrentTotalPair = match typ {
        GetProgressType::SnapshotExport => &WALK_SNAPSHOT_PROGRESS_EXPORT,
        GetProgressType::DatabaseGarbageCollection => &WALK_SNAPSHOT_PROGRESS_DB_GC,
        GetProgressType::DatabaseDump => &DATABASE_DUMP_PROGRESS,
    };

    Ok((
        tracker.0.load(atomic::Ordering::Relaxed),
        tracker.1.load(atomic::Ordering::Relaxed),
    ))
}
