// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{BitReader, Result};

/// An iterator over the runs of 1s and 0s of RLE+ encoded data.
pub struct Runs<'a> {
    /// The `BitReader` that is read from.
    reader: BitReader<'a>,
    /// The value of the next bit.
    next_value: bool,
}

impl<'a> Runs<'a> {
    /// Creates a new `Runs` instance given data that may or may
    /// not be correctly RLE+ encoded. Immediately returns an
    /// error if the version number is incorrect.
    pub fn new(bytes: &'a [u8]) -> Result<Self> {
        let mut reader = BitReader::new(bytes);

        let version = reader.read(2);
        if version != 0 {
            return Err("incorrect version");
        }

        let next_value = reader.read(1) == 1;
        Ok(Self { reader, next_value })
    }
}

impl Iterator for Runs<'_> {
    type Item = Result<(bool, usize)>;

    fn next(&mut self) -> Option<Self::Item> {
        let len = match self.reader.read_len() {
            Ok(len) => len?,
            Err(e) => return Some(Err(e)),
        };

        let run = (self.next_value, len);
        self.next_value = !self.next_value;
        Some(Ok(run))
    }
}
