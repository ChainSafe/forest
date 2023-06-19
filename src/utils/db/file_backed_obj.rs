// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    path::PathBuf,
    str::FromStr,
    time::{Duration, SystemTime},
};

use ahash::HashSet;
use cid::Cid;
use log::warn;

pub struct FileBacked<T: FileBackedObject> {
    inner: T,
    path: PathBuf,
    last_sync: Option<SystemTime>,
    sync_period: Option<Duration>,
}

pub const SYNC_PERIOD: Duration = Duration::from_secs(600);

impl<T: FileBackedObject> FileBacked<T> {
    /// Gets a borrow of the inner object
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// Gets a mutable borrow of the inner object
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Sets the inner object and try sync to file
    pub fn set_inner(&mut self, inner: T) -> anyhow::Result<()> {
        self.inner = inner;
        self.try_sync()
    }

    /// Calls function with inner mutable reference and try sync to file
    pub fn with_inner<F>(&mut self, func: F) -> anyhow::Result<()>
    where
        F: FnOnce(&mut T),
    {
        func(&mut self.inner);
        self.try_sync()
    }

    /// Creates a new file backed object
    pub fn new(inner: T, path: PathBuf) -> Self {
        Self {
            inner,
            path,
            last_sync: None,
            sync_period: None,
        }
    }

    /// Loads an object from a file and creates a new instance
    pub fn load_from_file_or_create<F: Fn() -> T>(
        path: PathBuf,
        create: F,
        sync_period: Option<Duration>,
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
                last_sync: None,
                sync_period,
            }
        } else {
            need_sync = true;
            Self {
                inner: create(),
                path,
                last_sync: None,
                sync_period,
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

    /// Try to sync to file if there is some sync period, otherwise syncs
    pub fn try_sync(&mut self) -> anyhow::Result<()> {
        if let Some(sync_period) = self.sync_period {
            let now = SystemTime::now();
            if let Some(last_sync) = self.last_sync {
                if now.duration_since(last_sync)? > sync_period {
                    self.last_sync = Some(now);
                    self.sync()?;
                }
                return Ok(());
            }
            self.last_sync = Some(now);
        } else {
            self.sync()?;
        }
        Ok(())
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
            FileBacked::load_from_file_or_create(file_path.clone(), || cid, None)?;
        let obj2: FileBacked<Cid> =
            FileBacked::load_from_file_or_create(file_path, Default::default, None)?;
        ensure!(obj1.inner() == obj2.inner());

        Ok(())
    }

    #[test]
    fn with_inner() -> Result<()> {
        let mut bytes = [0; 1024];
        rand::rngs::OsRng.fill(&mut bytes);
        let cid0 = Cid::new_v0(multihash::Code::Sha2_256.digest(bytes.as_slice()))?;
        let serialized0 = cid0.serialize()?;

        rand::rngs::OsRng.fill(&mut bytes);
        let cid1 = Cid::new_v0(multihash::Code::Sha2_256.digest(bytes.as_slice()))?;
        let serialized1 = cid1.serialize()?;

        let dir = TempDir::new()?;
        let file_path = dir.path().join("CID");
        let mut obj1: FileBacked<Cid> =
            FileBacked::load_from_file_or_create(file_path.clone(), || cid0, None)?;
        // Check if content of file match the cid value
        let result = std::fs::read(file_path.as_path())?;
        ensure!(serialized0 == result);

        obj1.with_inner(|inner| *inner = cid1)?;
        // Check if content of file match the new cid1 value
        let result = std::fs::read(file_path.as_path())?;
        ensure!(serialized1 == result);

        Ok(())
    }

    #[test]
    fn with_inner_with_period() -> Result<()> {
        const TEST_SYNC_PERIOD: Duration = Duration::from_millis(1);

        let mut bytes = [0; 1024];
        rand::rngs::OsRng.fill(&mut bytes);
        let cid0 = Cid::new_v0(multihash::Code::Sha2_256.digest(bytes.as_slice()))?;
        let serialized0 = cid0.serialize()?;

        rand::rngs::OsRng.fill(&mut bytes);
        let cid1 = Cid::new_v0(multihash::Code::Sha2_256.digest(bytes.as_slice()))?;
        let serialized1 = cid1.serialize()?;

        let dir = TempDir::new()?;
        let file_path = dir.path().join("CID");
        let mut obj1: FileBacked<Cid> = FileBacked::load_from_file_or_create(
            file_path.clone(),
            || cid0,
            Some(TEST_SYNC_PERIOD),
        )?;
        // Check if content of file match the cid value
        let result = std::fs::read(file_path.as_path())?;
        ensure!(serialized0 == result);

        obj1.with_inner(|inner| *inner = cid1)?;
        // Check if content of file still match the old cid0 value
        let result = std::fs::read(file_path.as_path())?;
        ensure!(obj1.inner() == &cid1);
        ensure!(serialized0 == result);

        // Wait for the period
        std::thread::sleep(TEST_SYNC_PERIOD);

        obj1.with_inner(|inner| *inner = cid1)?;
        // Check now if content of file match the new cid1 value
        let result = std::fs::read(file_path.as_path())?;
        ensure!(serialized1 == result);

        Ok(())
    }
}
