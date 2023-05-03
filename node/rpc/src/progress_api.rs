// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::{
    atomic::{self, AtomicU64},
    Arc,
};

use forest_ipld::{WALK_SNAPSHOT_PROGRESS_DB_GC, WALK_SNAPSHOT_PROGRESS_EXPORT};
use forest_rpc_api::progress_api::{GetProgressParams, GetProgressResult, GetProgressType};

use crate::*;

pub(crate) async fn get_progress(
    Params((typ,)): Params<GetProgressParams>,
) -> RpcResult<GetProgressResult> {
    let tracker: &Arc<(AtomicU64, AtomicU64)> = match typ {
        GetProgressType::SnapshotExport => &WALK_SNAPSHOT_PROGRESS_EXPORT,
        GetProgressType::DatabaseGarbageCollection => &WALK_SNAPSHOT_PROGRESS_DB_GC,
    };

    Ok((
        tracker.0.load(atomic::Ordering::Relaxed),
        tracker.1.load(atomic::Ordering::Relaxed),
    ))
}
