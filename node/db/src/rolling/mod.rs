// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod impls;
mod index;

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

use forest_utils::db::file_backed_obj::FileBacked;
use log::{info, warn};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::db_engine::{open_db, Db, DbConfig};

pub struct RollingDB {
    db_root: PathBuf,
    db_config: DbConfig,
    db_index: RwLock<FileBacked<DbIndex>>,
    /// A queue of active databases, from youngest to oldest
    db_queue: RwLock<VecDeque<Db>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct DbIndex {
    db_names: VecDeque<String>,
}
