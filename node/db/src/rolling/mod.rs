// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//!
//! This DB wrapper is specially designed for supporting the concurrent,
//! semi-space GC algorithm that is implemented in [DbGarbageCollector],
//! containing a reference to the `old` DB space and a reference to the
//! `current` DB space. Both underlying key-vale DB are supposed to contain only
//! block data as value and its content-addressed CID as key

mod gc;
pub use gc::*;
mod impls;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use forest_utils::db::file_backed_obj::FileBacked;
use log::{info, warn};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::db_engine::{open_db, Db, DbConfig};

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
    old: String,
}
