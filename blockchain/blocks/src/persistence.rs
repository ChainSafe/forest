// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use fvm_ipld_encoding::Cbor;
use log::warn;

use crate::*;

pub struct FileBacked<T: FileBackedObject> {
    inner: Option<T>,
    path: PathBuf,
}

impl<T: FileBackedObject> FileBacked<T> {
    /// Gets a borrow of the inner object
    pub fn inner(&self) -> &Option<T> {
        &self.inner
    }

    /// Sets the inner object and flushes to file
    pub fn set_inner(&mut self, inner: T) -> anyhow::Result<()> {
        self.inner = Some(inner);
        self.flush_to_file()
    }

    /// Creates a new file backed object
    pub fn new(inner: Option<T>, path: PathBuf) -> Self {
        Self { inner, path }
    }

    /// Loads an object from a file and creates a new instance
    pub fn load_from_file_or_new(path: PathBuf) -> anyhow::Result<Self> {
        if path.is_file() {
            let bytes = std::fs::read(path.as_path())?;
            Ok(Self {
                inner: T::deserialize(&bytes)
                    .map_err(|e| {
                        warn!("Error loading object from {}", path.display());
                        e
                    })
                    .ok(),
                path,
            })
        } else {
            Ok(Self { inner: None, path })
        }
    }

    /// Flushes the object to the file
    pub fn flush_to_file(&self) -> anyhow::Result<()> {
        if let Some(inner) = &self.inner {
            let bytes = inner.serialize()?;
            Ok(std::fs::write(&self.path, bytes)?)
        } else {
            anyhow::bail!("Inner object is not set")
        }
    }
}

/// An object that is backed by a single file on disk
pub trait FileBackedObject: Sized {
    /// Serializes into a byte array
    fn serialize(&self) -> anyhow::Result<Vec<u8>>;

    /// Deserializes from a byte array
    fn deserialize(bytes: &[u8]) -> anyhow::Result<Self>;
}

impl FileBackedObject for BlockHeader {
    fn serialize(&self) -> anyhow::Result<Vec<u8>> {
        Ok(self.marshal_cbor()?)
    }

    fn deserialize(bytes: &[u8]) -> anyhow::Result<Self> {
        Ok(BlockHeader::unmarshal_cbor(bytes)?)
    }
}

impl FileBackedObject for TipsetKeys {
    fn serialize(&self) -> anyhow::Result<Vec<u8>> {
        Ok(self.marshal_cbor()?)
    }

    fn deserialize(bytes: &[u8]) -> anyhow::Result<Self> {
        Ok(fvm_ipld_encoding::from_slice(bytes)?)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::*;

    use super::*;

    #[test]
    fn block_header_round_trip() -> Result<()> {
        let path = Path::new("tests/calibnet/GENESIS");
        let obj1: FileBacked<BlockHeader> = FileBacked::load_from_file_or_new(path.into())?;
        ensure!(obj1.inner().is_some());
        obj1.flush_to_file()?;
        let obj2: FileBacked<BlockHeader> = FileBacked::load_from_file_or_new(path.into())?;
        ensure!(obj1.inner() == obj2.inner());

        Ok(())
    }

    #[test]
    fn tipset_keys_round_trip() -> Result<()> {
        let path = Path::new("tests/calibnet/HEAD");
        let obj1: FileBacked<TipsetKeys> = FileBacked::load_from_file_or_new(path.into())?;
        ensure!(obj1.inner().is_some());
        obj1.flush_to_file()?;
        let obj2: FileBacked<TipsetKeys> = FileBacked::load_from_file_or_new(path.into())?;
        ensure!(obj1.inner() == obj2.inner());

        Ok(())
    }
}
