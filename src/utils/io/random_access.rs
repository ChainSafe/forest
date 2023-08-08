// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use positioned_io::{ReadAt, Size};
use std::fs::File;
use std::io;
use std::path::Path;

/// This wrapper is needed, because [`positioned_io::RandomAccessFile`] does not support
/// [`positioned_io::Size`].
pub struct RandomAccessFile {
    file: positioned_io::RandomAccessFile,
    size: Option<u64>,
}

impl RandomAccessFile {
    /// This opens a file and fetches the size. Fails with an error if the path is a directory.
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<RandomAccessFile> {
        let file = File::open(path)?;
        let size = file.size()?;
        let file = positioned_io::RandomAccessFile::try_new(file)?;
        Ok(Self { file, size })
    }
}

impl Size for RandomAccessFile {
    fn size(&self) -> std::io::Result<Option<u64>> {
        Ok(self.size)
    }
}

impl ReadAt for RandomAccessFile {
    fn read_at(&self, pos: u64, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read_at(pos, buf)
    }

    fn read_exact_at(&self, pos: u64, buf: &mut [u8]) -> io::Result<()> {
        self.file.read_exact_at(pos, buf)
    }
}
