// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

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
