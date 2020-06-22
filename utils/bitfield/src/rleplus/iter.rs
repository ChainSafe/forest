// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{BitReader, RLEPlus, RangeIterator, Result};
use std::{iter::FusedIterator, ops::Range};

/// An iterator over the ranges of 1s of RLE+ encoded data.
pub struct DecodedRanges<'a> {
    /// The `BitReader` that is read from.
    reader: BitReader<'a>,
    /// The value of the next bit.
    next_value: bool,
    /// The current position, i.e. the end of the last range that was
    /// read, or 0 if no ranges have been read.
    offset: usize,
}

impl<'a> DecodedRanges<'a> {
    /// Creates a new `DecodedRanges` instance given data that may or
    /// may not be correctly RLE+ encoded. Immediately returns an error
    /// if the version number is incorrect.
    pub fn new(bytes: &'a [u8]) -> Result<Self> {
        let mut reader = BitReader::new(bytes);

        let version = reader.read(2);
        if version != 0 {
            return Err("incorrect version");
        }

        let next_value = reader.read(1) == 1;
        Ok(Self {
            reader,
            next_value,
            offset: 0,
        })
    }

    /// Returns the next range of 1s from the RLE+ encoded data, or an error
    /// if an error is encountered decoding the data.
    fn next_range(&mut self) -> Result<Option<Range<usize>>> {
        // if the next value is a 0 (which is always the case unless we're reading
        // the first range of 1s which is right at the start) then we read the
        // number of 0s and update `offset` accordingly
        if !self.next_value {
            match self.reader.read_len()? {
                Some(zeros) => self.offset += zeros,
                None => return Ok(None),
            }
        }

        let start = self.offset;

        match self.reader.read_len()? {
            Some(ones) => self.offset += ones,
            None => return Ok(None),
        }

        self.next_value = false;
        let end = self.offset;
        Ok(Some(start..end))
    }
}

impl Iterator for DecodedRanges<'_> {
    type Item = Result<Range<usize>>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_range().transpose()
    }
}

/// An iterator over the ranges of 1s of RLE+ encoded data that has already been verified.
pub struct Ranges<'a>(DecodedRanges<'a>);

impl<'a> Ranges<'a> {
    pub(super) fn new(encoded: &'a RLEPlus) -> Self {
        // the data has already been verified, so this cannot fail
        Self(DecodedRanges::new(encoded.as_bytes()).unwrap())
    }
}

impl Iterator for Ranges<'_> {
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        // the data has already been verified, so this cannot fail
        self.0.next().map(Result::unwrap)
    }
}

impl FusedIterator for Ranges<'_> {}
impl RangeIterator for Ranges<'_> {}
