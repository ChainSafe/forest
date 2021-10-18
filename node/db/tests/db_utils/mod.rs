// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "rocksdb")]

// Taken from
// https://github.com/rust-rocksdb/rust-rocksdb/blob/master/tests/util/mod.rs
use rocksdb::{Options, DB};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Ensures that DB::Destroy is called for this database when DBPath is dropped.
pub struct DBPath {
    pub path: PathBuf,
}

impl DBPath {
    /// Suffixes the given `prefix` with a timestamp to ensure that subsequent test runs don't reuse
    /// an old database in case of panics prior to Drop being called.
    pub fn new(prefix: &str) -> DBPath {
        let current_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let path = format!(
            "{}.{}.{}",
            prefix,
            current_time.as_secs(),
            current_time.subsec_nanos()
        );

        DBPath {
            path: PathBuf::from(path),
        }
    }
}

impl Drop for DBPath {
    fn drop(&mut self) {
        let opts = Options::default();
        DB::destroy(&opts, &self.path).unwrap();
    }
}

impl AsRef<Path> for DBPath {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}
