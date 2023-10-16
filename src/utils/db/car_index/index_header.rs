// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use positioned_io::ReadAt;
use std::io;
use std::mem::size_of;
use zerocopy::{little_endian::U64 as U64LE, AsBytes, FromBytes, FromZeroes};

/// Layout of this struct is the same in-memory as on the wire, so it can be
/// serialized using [`zerocopy::FromBytes::read_from`] and deserialized using
/// [`zerocopy::AsBytes::as_bytes`].
#[derive(AsBytes, Clone, Copy, Debug, Eq, FromBytes, FromZeroes, Hash, PartialEq)]
#[repr(C)]
pub struct IndexHeader {
    // Version number
    pub magic_number: U64LE,
    // Worst-case distance between an entry and its bucket.
    pub longest_distance: U64LE,
    // Number of hash collisions. Reserved for future use.
    pub collisions: U64LE,
    // Number of buckets. Note that the index includes padding after the last
    // bucket.
    pub buckets: U64LE,
}

// There are no padding bytes
static_assertions::const_assert_eq!(size_of::<IndexHeader>(), size_of::<u64>() * 4);

impl IndexHeader {
    pub const SIZE: usize = size_of::<Self>();
    // 0xdeadbeef + 0 used a different hash algorithm
    pub const MAGIC_NUMBER: u64 = 0xdeadbeef + 1;

    pub fn read(reader: impl ReadAt, offset: u64) -> io::Result<IndexHeader> {
        let mut buffer = [0; Self::SIZE];
        reader.read_exact_at(offset, &mut buffer)?;
        Ok(IndexHeader::read_from(buffer.as_slice()).expect("`buffer` is the correct size"))
    }
}
