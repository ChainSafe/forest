// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fs;
use std::path::PathBuf;

use log::warn;

/// Wrapper of temporary file that deletes file on drop
#[derive(Debug, Clone)]
pub struct TempFile {
    path: PathBuf,
}

impl TempFile {
    /// Creates a temporary file wrapper
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Gets path of the temporary file
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if self.path.exists() {
            if let Err(err) = fs::remove_file(&self.path) {
                warn!("Failed to delete {}: {err}", self.path.display());
            }
        }
    }
}
