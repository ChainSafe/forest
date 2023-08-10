// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! The state of the Filecoin Blockchain is a persistent, directed acyclic
//! graph. Data in this graph is never mutated nor explicitly deleted but may
//! become unreachable over time.
//!
//! This module contains a concurrent, semi-space garbage collector. The garbage
//! collector is guaranteed to be non-blocking and can be expected to run with a
//! fixed memory overhead and require disk space proportional to the size of the
//! reachable graph. For example, if the size of the reachable graph is 100 GiB,
//! expect this garbage collector to use `3x100 GiB = 300 GiB` of storage.

mod gc;
pub use gc::*;
mod impls;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use super::car::ManyCar;
use crate::db::db_engine::{open_db, Db, DbConfig};
use crate::utils::db::file_backed_obj::FileBacked;

/// This DB wrapper is specially designed for supporting the concurrent,
/// semi-space GC algorithm that is implemented in [`DbGarbageCollector`],
/// containing a reference to the `old` DB space and a reference to the
/// `current` DB space. Both underlying key-vale DB are supposed to contain only
/// block data as value and its content-addressed CID as key
pub struct RollingDB {
    db_root: PathBuf,
    db_config: DbConfig,
    db_index: RwLock<FileBacked<DbIndex>>,
    /// The current writable DB
    current: RwLock<Arc<Db>>,
    /// The old writable DB
    old: RwLock<Arc<Db>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct DbIndex {
    current: String,
    #[serde(default = "Default::default")]
    current_creation_epoch: i64,
    old: String,
}
