// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{path::PathBuf, str::FromStr};

use cid::Cid;
use log::warn;

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

    /// Sets the inner object and flushes to file
    pub fn set_inner(&mut self, inner: T) -> anyhow::Result<()> {
        self.inner = inner;
        self.flush_to_file()
    }

    /// Creates a new file backed object
    pub fn new(inner: T, path: PathBuf) -> Self {
        Self { inner, path }
    }

    /// Loads an object from a file and creates a new instance
    pub fn load_from_file_or_create<F: Fn() -> T>(
        path: PathBuf,
        create: F,
    ) -> anyhow::Result<Self> {
        let mut need_flush = false;
        let obj = if path.is_file() {
            let bytes = std::fs::read(path.as_path())?;
            Self {
                inner: T::deserialize(&bytes)
                    .map_err(|e| {
                        warn!("Error loading object from {}", path.display());
                        need_flush = true;
                        e
                    })
                    .unwrap_or_else(|_| create()),
                path,
            }
        } else {
            need_flush = true;
            Self {
                inner: create(),
                path,
            }
        };

        if need_flush {
            obj.flush_to_file()?;
        }

        Ok(obj)
    }

    /// Flushes the object to the file
    pub fn flush_to_file(&self) -> anyhow::Result<()> {
        let bytes = self.inner().serialize()?;
        Ok(std::fs::write(&self.path, bytes)?)
    }
}

/// An object that is backed by a single file on disk
pub trait FileBackedObject: Sized {
    /// Serializes into a byte array
    fn serialize(&self) -> anyhow::Result<Vec<u8>>;

    /// Deserializes from a byte array
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

#[cfg(test)]
mod tests {
    use anyhow::*;
    use cid::multihash::MultihashDigest;
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
