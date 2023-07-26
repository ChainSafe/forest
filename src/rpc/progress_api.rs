// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(clippy::unused_async)]

use std::sync::atomic;

use crate::ipld::{ProgressBarCurrentTotalPair, WALK_SNAPSHOT_PROGRESS_DB_GC};
use crate::rpc_api::progress_api::{GetProgressParams, GetProgressResult, GetProgressType};

use crate::rpc::*;

pub(in crate::rpc) async fn get_progress(
    Params((typ,)): Params<GetProgressParams>,
) -> RpcResult<GetProgressResult> {
    let tracker: &ProgressBarCurrentTotalPair = match typ {
        GetProgressType::DatabaseGarbageCollection => &WALK_SNAPSHOT_PROGRESS_DB_GC,
    };

    Ok((
        tracker.0.load(atomic::Ordering::Relaxed),
        tracker.1.load(atomic::Ordering::Relaxed),
    ))
}
