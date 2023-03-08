// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use log::warn;

pub struct FileBacked<T: FileBackedObject> {
    inner: Option<T>,
    path: PathBuf,
}

impl<T: FileBackedObject> FileBacked<T> {
    /// Gets a borrow of the inner object
    pub fn inner(&self) -> &Option<T> {
        &self.inner
    }

    /// Gets a mutable borrow of the inner object
    pub fn inner_mut(&mut self) -> &mut Option<T> {
        &mut self.inner
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

impl<T: FileBackedObject + Default> FileBacked<T> {
    pub fn inner_mut_or_default(&mut self) -> &mut T {
        self.inner_mut().get_or_insert_with(Default::default)
    }
}

/// An object that is backed by a single file on disk
pub trait FileBackedObject: Sized {
    /// Serializes into a byte array
    fn serialize(&self) -> anyhow::Result<Vec<u8>>;

    /// Deserializes from a byte array
    fn deserialize(bytes: &[u8]) -> anyhow::Result<Self>;
}
