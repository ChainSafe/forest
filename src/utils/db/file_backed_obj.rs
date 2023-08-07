// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{path::PathBuf, str::FromStr};

use ahash::HashSet;
use cid::Cid;
use tracing::warn;

pub struct FileBacked<T: FileBackedObject> {
    inner: T,
    path: PathBuf,
}

impl<T: FileBackedObject> FileBacked<T> {
    /// Gets a borrow of the inner object
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// Gets a mutable borrow of the inner object
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Loads an object from a file and creates a new instance
    pub fn load_from_file_or_create<F: Fn() -> T>(
        path: PathBuf,
        create: F,
    ) -> anyhow::Result<Self> {
        let mut need_sync = false;
        let obj = if path.is_file() {
            let bytes = std::fs::read(path.as_path())?;
            Self {
                inner: T::deserialize(&bytes)
                    .map_err(|e| {
                        warn!("Error loading object from {}", path.display());
                        need_sync = true;
                        e
                    })
                    .unwrap_or_else(|_| create()),
                path,
            }
        } else {
            need_sync = true;
            Self {
                inner: create(),
                path,
            }
        };

        if need_sync {
            obj.sync()?;
        }

        Ok(obj)
    }

    /// Syncs the object to the file
    pub fn sync(&self) -> anyhow::Result<()> {
        let bytes = self.inner().serialize()?;
        Ok(std::fs::write(&self.path, bytes)?)
    }
}

/// An object that is backed by a single file on disk
pub trait FileBackedObject: Sized {
    /// Serializes into a byte array
    fn serialize(&self) -> anyhow::Result<Vec<u8>>;

    /// De-serializes from a byte array
    fn deserialize(bytes: &[u8]) -> anyhow::Result<Self>;
}

impl FileBackedObject for Cid {
    fn serialize(&self) -> anyhow::Result<Vec<u8>> {
        Ok(self.to_string().into_bytes())
    }

    fn deserialize(bytes: &[u8]) -> anyhow::Result<Self> {
        Ok(Cid::from_str(String::from_utf8_lossy(bytes).trim())?)
    }
}

impl FileBackedObject for HashSet<Cid> {
    fn serialize(&self) -> anyhow::Result<Vec<u8>> {
        let serialized = serde_json::to_string(&self)?;
        Ok(serialized.into_bytes())
    }

    fn deserialize(bytes: &[u8]) -> anyhow::Result<Self> {
        let result = serde_json::from_str(String::from_utf8_lossy(bytes).trim());
        Ok(result?)
    }
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct ChainMeta {
    pub estimated_reachable_records: usize,
}

impl FileBackedObject for ChainMeta {
    fn serialize(&self) -> anyhow::Result<Vec<u8>> {
        let serialized = serde_yaml::to_string(&self)?;
        Ok(serialized.into_bytes())
    }

    fn deserialize(bytes: &[u8]) -> anyhow::Result<Self> {
        let result = serde_yaml::from_str(String::from_utf8_lossy(bytes).trim())?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use anyhow::*;
    use cid::multihash::{self, MultihashDigest};
    use rand::Rng;
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn cid_round_trip() -> Result<()> {
        let mut bytes = [0; 1024];
        rand::rngs::OsRng.fill(&mut bytes);
        let cid = Cid::new_v0(multihash::Code::Sha2_256.digest(bytes.as_slice()))?;
        let serialized = cid.serialize()?;
        let deserialized = Cid::deserialize(&serialized)?;
        ensure!(cid == deserialized);

        let dir = TempDir::new()?;
        let file_path = dir.path().join("CID");
        let obj1: FileBacked<Cid> =
            FileBacked::load_from_file_or_create(file_path.clone(), || cid)?;
        let obj2: FileBacked<Cid> =
            FileBacked::load_from_file_or_create(file_path, Default::default)?;
        ensure!(obj1.inner() == obj2.inner());

        Ok(())
    }
}
