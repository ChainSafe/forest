// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{fs, path::PathBuf};

use tracing::warn;

/// Wrapper of temporary file that deletes file on drop
#[derive(Debug, Clone)]
pub struct TempFile {
    path: PathBuf,
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if self.path.exists() {
            if let Err(err) = fs::remove_file(&self.path) {
                warn!(path = %self.path.display(), %err, "failed to delete");
            }
        }
    }
}
