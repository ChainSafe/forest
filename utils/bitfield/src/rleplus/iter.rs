// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{BitReader, RangeIterator, Result, RlePlus};
use std::{iter::FusedIterator, ops::Range};

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

/// An iterator over the ranges of 1s of RLE+ encoded data that has already been verified.
pub struct Ranges<'a> {
    /// The underlying runs of 1s and 0s.
    runs: Runs<'a>,
    /// The current position, i.e. the end of the last range that was read,
    /// or 0 if no ranges have been read yet.
    offset: usize,
}

impl<'a> Ranges<'a> {
    pub(super) fn new(encoded: &'a RlePlus) -> Self {
        Self {
            // the data has already been verified, so this cannot fail
            runs: Runs::new(encoded.as_bytes()).unwrap(),
            offset: 0,
        }
    }
}

impl Iterator for Ranges<'_> {
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        // this loop will run either 1 or 2 times because runs alternate
        loop {
            // the data has already been verified, so this cannot fail
            let (value, len) = self.runs.next()?.unwrap();

            let start = self.offset;
            self.offset += len;

            if value {
                return Some(start..self.offset);
            }
        }
    }
}

impl FusedIterator for Ranges<'_> {}
impl RangeIterator for Ranges<'_> {}
