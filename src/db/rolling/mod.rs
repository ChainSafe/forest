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

use crate::utils::db::file_backed_obj::FileBacked;
use log::{info, warn};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::db::db_engine::{open_db, Db, DbConfig};

/// This DB wrapper is specially designed for supporting the concurrent,
/// semi-space GC algorithm that is implemented in [`DbGarbageCollector`],
/// containing a reference to the `old` DB space and a reference to the
/// `current` DB space. Both underlying key-vale DB are supposed to contain only
/// block data as value and its content-addressed CID as key
#[derive(Clone)]
pub struct RollingDB {
    db_root: Arc<PathBuf>,
    db_config: Arc<DbConfig>,
    db_index: Arc<RwLock<FileBacked<DbIndex>>>,
    /// The current writable DB
    current: Arc<RwLock<Db>>,
    /// The old writable DB
    old: Arc<RwLock<Db>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct DbIndex {
    current: String,
    #[serde(default = "Default::default")]
    current_creation_epoch: i64,
    old: String,
}
