// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod gc;
pub use gc::*;
mod impls;

use std::{
    collections::VecDeque,
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
    /// A queue of active databases, from youngest to oldest
    db_queue: Arc<RwLock<VecDeque<Db>>>,
    /// The current writable DB
    current: Arc<RwLock<(String, Db)>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct DbIndex {
    db_names: VecDeque<String>,
}
