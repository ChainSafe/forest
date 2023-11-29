// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{fs, io, path::Path};

use memmap2::MmapAsRawDesc;
use positioned_io::{RandomAccessFile, ReadAt, Size};

/// Wrapper type of [`memmap2::Mmap`] that implements [`ReadAt`] and [`Size`]
pub struct Mmap(memmap2::Mmap);

impl Mmap {
    pub fn map(file: impl MmapAsRawDesc) -> io::Result<Self> {
        Ok(Self(unsafe { memmap2::Mmap::map(file)? }))
    }

    pub fn map_path(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::map(&fs::File::open(path.as_ref())?)
    }
}

impl ReadAt for Mmap {
    fn read_at(&self, pos: u64, buf: &mut [u8]) -> io::Result<usize> {
        let start = pos as usize;
        if start >= self.0.len() {
            // This matches the behaviour for seeking past the end of a file
            return Ok(0);
        }
        let end = start + buf.len();
        if end <= self.0.len() {
            buf.copy_from_slice(&self.0[start..end]);
            Ok(buf.len())
        } else {
            let len = self.0.len() - start;
            buf[..len].copy_from_slice(&self.0[start..]);
            Ok(len)
        }
    }
}

impl Size for Mmap {
    fn size(&self) -> io::Result<Option<u64>> {
        Ok(Some(self.0.len() as _))
    }
}

pub enum EitherMmapOrRandomAccessFile {
    Mmap(Mmap),
    RandomAccessFile(RandomAccessFile),
}

impl EitherMmapOrRandomAccessFile {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(if should_use_file_io() {
            Self::RandomAccessFile(RandomAccessFile::open(path)?)
        } else {
            Self::Mmap(Mmap::map_path(path)?)
        })
    }
}

impl ReadAt for EitherMmapOrRandomAccessFile {
    fn read_at(&self, pos: u64, buf: &mut [u8]) -> io::Result<usize> {
        use EitherMmapOrRandomAccessFile::*;
        match self {
            Mmap(mmap) => mmap.read_at(pos, buf),
            RandomAccessFile(file) => file.read_at(pos, buf),
        }
    }
}

impl Size for EitherMmapOrRandomAccessFile {
    fn size(&self) -> io::Result<Option<u64>> {
        use EitherMmapOrRandomAccessFile::*;
        match self {
            Mmap(mmap) => mmap.size(),
            RandomAccessFile(file) => file.size(),
        }
    }
}

fn should_use_file_io() -> bool {
    // Use mmap by default, switch to file-io when `FOREST_CAR_LOADER_FILE_IO` is set to `1` or `true`
    match std::env::var("FOREST_CAR_LOADER_FILE_IO") {
        Ok(var) => matches!(var.to_lowercase().as_str(), "1" | "true"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn test_mmap_read_at_and_size(bytes: Vec<u8>) -> anyhow::Result<()> {
        let tmp = tempfile::Builder::new().tempfile()?.into_temp_path();
        fs::write(&tmp, &bytes)?;
        let mmap = Mmap::map(&fs::File::open(&tmp)?)?;

        assert_eq!(mmap.size()?.unwrap_or_default() as usize, bytes.len());

        let mut buffer = [0; 128];
        for pos in 0..bytes.len() {
            let size = mmap.read_at(pos as _, &mut buffer)?;
            assert_eq!(&bytes[pos..(pos + size)], &buffer[..size]);
        }

        Ok(())
    }

    #[test]
    fn test_out_of_band_mmap_read() {
        let temp_file = tempfile::Builder::new()
            .tempfile()
            .unwrap()
            .into_temp_path();
        let mmap = Mmap::map(&fs::File::open(&temp_file).unwrap()).unwrap();

        let mut buffer = [];
        // This matches the behaviour for seeking past the end of a file
        assert_eq!(mmap.read_at(u64::MAX, &mut buffer).unwrap(), 0);
    }
}
