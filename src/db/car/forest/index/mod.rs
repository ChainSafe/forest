// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//! Embedded index for the `.forest.car.zst` format.
//!
//! Maps from [`Cid`]s to candidate zstd frame offsets.
//!
//! # Design statement
//!
//! - Create once, read many times.
//!   This means that existing databases are overkill - most of their API
//!   complexity is for write support.
//! - Embeddable in-file.
//!   This precludes most existing databases, which operate on files or folders.
//! - Lookups must NOT require reading the index into memory.
//!   This precludes using e.g [`serde::Serialize`]
//! - (Bonus) efficient merging of multiple indices.
//!
//! # Implementation
//!
//! The simplest implementation is a sorted list of `(Cid, u64)`, pairs.
//! We'll call each such pair an `entry`.
//! But this simple implementation has a couple of downsides:
//! - `O(log(n))` time complexity for searches with binary search.
//!   (We could try to amortise this by doing an initial scan for checkpoints,
//!   but seeking backwards in the file may still be penalised by the OS).
//! - Variable length, possibly large entries on disk.
//!
//! We can address this by using a hash table with linear probing.
//! This is a linear array of equal-length [`Slot`]s.
//! - [hashing](hash::summary) the [`Cid`] gives us a fixed length entry.
//! - A [`hash::ideal_slot_ix`] gives us a likely location to find the entry,
//!   given a table size.
//!   That is, a hash in a collection of length 10 has a different `ideal_slot_ix`
//!   than if the same hash were in a collection of length 20.
//! - We have two types of collisions:
//!   - Hash collisions.
//!   - [`hash::ideal_slot_ix`] collisions.
//!
//!   We use linear probing, which means that colliding entries are always
//!   concatenated - seeking forward to the next entry will yield any collisions.
//! - A slot is always found at or within [`Table::longest_distance`] after its
//!   [`hash::ideal_slot_ix`].

#[cfg_vis(feature = "benchmark-private", pub)]
use self::util::NonMaximalU64;
use byteorder::{LittleEndian, ReadBytesExt as _, WriteBytesExt as _};
use cfg_vis::cfg_vis;
use cid::Cid;
use itertools::Itertools as _;
use positioned_io::ReadAt;
use smallvec::{smallvec, SmallVec};
use std::{
    cmp,
    io::{self, Read, Write},
    iter,
    num::NonZeroUsize,
};

#[cfg(not(any(test, feature = "benchmark-private")))]
mod hash;
#[cfg(any(test, feature = "benchmark-private"))]
pub mod hash;

/// Reader for the `.forest.car.zst`'s embedded index.
///
/// See [module documentation](mod@self) for more.
pub struct Reader<R> {
    inner: R,
    table_offset: u64,
    header: V1Header,
}

impl<R> Reader<R>
where
    R: ReadAt,
{
    pub fn new(reader: R) -> io::Result<Self> {
        let mut reader = positioned_io::Cursor::new(reader);
        let Version::V1 = Version::read_from(&mut reader)? else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported embedded index version",
            ));
        };
        let header = V1Header::read_from(&mut reader)?;
        Ok(Self {
            table_offset: reader.position(),
            inner: reader.into_inner(),
            header,
        })
    }

    /// Look up possible frame offsets for a [`Cid`].
    /// Returns `Ok([])` if no offsets are found, or [`Err(_)`] if the underlying
    /// IO fails.
    ///
    /// Does not allocate unless 2 or more CIDs have collided, see [module documentation](mod@self).
    ///
    /// You MUST check the actual CID at the offset to see if it matches.
    pub fn get(&self, key: Cid) -> io::Result<SmallVec<[u64; 1]>> {
        self.get_by_hash(hash::summary(&key))
    }

    /// Jump to slot offset and scan downstream. All key-value pairs with a
    /// matching key are guaranteed to appear before we encounter an empty slot.
    #[cfg_vis(feature = "benchmark-private", pub)]
    fn get_by_hash(&self, needle: NonMaximalU64) -> io::Result<SmallVec<[u64; 1]>> {
        let Some(initial_buckets) =
            NonZeroUsize::new(self.header.initial_buckets.try_into().unwrap())
        else {
            return Ok(smallvec![]); // empty table
        };
        let offset_in_table =
            u64::try_from(hash::ideal_slot_ix(needle, initial_buckets)).unwrap() * RawSlot::WIDTH;
        let mut haystack =
            positioned_io::Cursor::new_pos(&self.inner, self.table_offset + offset_in_table);

        let mut limit = self.header.longest_distance;
        while let Slot::Occupied(OccupiedSlot { hash, frame_offset }) =
            Slot::read_from(&mut haystack)?
        {
            if hash == needle {
                let mut found = smallvec![frame_offset];
                // The entries are sorted. Once we've found a matching key, all
                // duplicate hash keys will be right next to it.
                loop {
                    match Slot::read_from(&mut haystack)? {
                        Slot::Occupied(another) if another.hash == needle => {
                            found.push(another.frame_offset)
                        }
                        Slot::Empty | Slot::Occupied(_) => return Ok(found),
                    }
                }
            }
            if limit == 0 {
                // Even the biggest bucket does not have this many entries. We
                // can safely return an empty result now.
                return Ok(smallvec![]);
            }
            limit -= 1;
        }
        Ok(smallvec![]) // didn't find anything
    }

    /// Gets a reference to the underlying reader.
    pub fn reader(&self) -> &R {
        &self.inner
    }

    /// Replace the inner reader.
    /// It MUST point to the same underlying IO, else future calls to `get`
    /// will be incorrect.
    pub fn map<T>(self, f: impl FnOnce(R) -> T) -> Reader<T> {
        Reader {
            inner: f(self.inner),
            table_offset: self.table_offset,
            header: self.header,
        }
    }
}

#[cfg_vis(feature = "benchmark-private", pub)]
const DEFAULT_LOAD_FACTOR: f64 = 0.8;

#[derive(Debug)]
pub struct Writer {
    version: Version,
    header: V1Header,
    slots: Vec<Slot>,
}

impl Writer {
    pub fn new(locations: impl IntoIterator<Item = (Cid, u64)>) -> Self {
        Self::from_table(Table::new(locations, DEFAULT_LOAD_FACTOR))
    }
    fn from_table(table: Table) -> Self {
        let Table {
            slots,
            initial_width,
            collisions,
            longest_distance,
        } = table;
        Self {
            version: Version::V1,
            header: V1Header {
                longest_distance: longest_distance.try_into().unwrap(),
                collisions: collisions.try_into().unwrap(),
                initial_buckets: initial_width.try_into().unwrap(),
            },
            slots,
        }
    }
    pub fn written_len(&self) -> u64 {
        let Self {
            version,
            header,
            slots,
        } = self;
        version.written_len()
            + header.written_len()
            + slots.iter().map(Writeable::written_len).sum::<u64>()
    }
    pub fn write_into(self, mut writer: impl Write) -> io::Result<()> {
        let Self {
            version,
            header,
            slots,
        } = self;
        version.write_to(&mut writer)?;
        header.write_to(&mut writer)?;
        for slot in slots {
            slot.write_to(&mut writer)?
        }
        Ok(())
    }
}

/// An in-memory representation of a hash-table.
#[derive(Debug, Clone, Hash, PartialOrd, Ord, PartialEq, Eq)]
#[cfg_vis(feature = "benchmark-private", pub)]
struct Table {
    // public for benchmarks
    pub slots: Vec<Slot>,
    pub initial_width: usize,
    collisions: usize,
    longest_distance: usize,
}

impl Table {
    /// Construct a new table with the invariants outlined in the [module documentation](mod@self).
    ///
    /// The `load_factor` determines the average number of bucket a lookup has
    /// to scan.
    /// The formula, with 'f' being the load factor, is `(1+1/(1-load_factor))/2`.
    /// A load factor of `0.8` means [`Reader::get`] has to scan through 3
    /// slots on average.
    /// A load-factor of `0.9` means we have to scan through 5.5 slots on average.
    ///
    /// See the `car-index` benchmark for measurements of scans at different lengths.
    ///
    /// # Panics
    /// - if `load_factor` is not in the interval `0..=1`
    pub fn new<I>(locations: I, load_factor: f64) -> Self
    where
        I: IntoIterator<Item = (Cid, u64)>,
    {
        Self::new_from_hashes(
            locations
                .into_iter()
                .map(|(cid, frame_offset)| (hash::summary(&cid), frame_offset)),
            load_factor,
        )
    }
    /// Separate constructor for testability.
    fn new_from_hashes<I>(locations: I, load_factor: f64) -> Self
    where
        I: IntoIterator<Item = (NonMaximalU64, u64)>,
    {
        assert!((0.0..=1.0).contains(&load_factor));

        let slots = locations
            .into_iter()
            .map(|(hash, frame_offset)| OccupiedSlot { hash, frame_offset })
            .sorted()
            .collect::<Vec<_>>();

        let Some(initial_width) = initial_width(slots.len(), load_factor) else {
            return Self {
                slots: vec![Slot::Empty],
                initial_width: 0,
                collisions: 0,
                longest_distance: 0,
            };
        };

        let collisions = slots
            .iter()
            .group_by(|it| it.hash)
            .into_iter()
            .map(|(_, group)| group.count() - 1)
            .max()
            .unwrap_or_default();

        let mut total_padding = 0;
        let mut slots = slots
            .into_iter()
            .enumerate()
            .flat_map(|(ix, it)| {
                let actual_ix = ix + total_padding;
                let ideal_ix = hash::ideal_slot_ix(it.hash, initial_width);
                let padding = ideal_ix.saturating_sub(actual_ix);
                total_padding += padding;
                iter::repeat(Slot::Empty)
                    .take(padding)
                    .chain(iter::once(Slot::Occupied(it)))
            })
            .chain(iter::once(Slot::Empty))
            .collect::<Vec<_>>();

        // ensure there are at least `initial_width` slots, else lookups could
        // try and read off the end of the table
        if let Some(padding) = initial_width.get().checked_sub(slots.len()) {
            slots.extend(iter::repeat(Slot::Empty).take(padding))
        }

        Self {
            longest_distance: slots
                .iter()
                .enumerate()
                .filter_map(|(ix, slot)| {
                    slot.as_occupied()
                        .map(|it| ix - hash::ideal_slot_ix(it.hash, initial_width))
                })
                .max()
                .unwrap_or_default(),
            slots,
            initial_width: initial_width.get(),
            collisions,
        }
    }
}

fn initial_width(slots_len: usize, load_factor: f64) -> Option<NonZeroUsize> {
    NonZeroUsize::new(cmp::max(
        (slots_len as f64 / load_factor) as usize,
        slots_len,
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, num_derive::FromPrimitive)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
#[repr(u64)]
enum Version {
    V0 = 0xdeadbeef,
    V1 = 0xdeadbeef + 1,
    // V2 should use [`std::num::NonZeroU64`] instead of [`util::NonMaximalU64`]
    // since that allows a niche optimization on [`Slot`] (and there will be
    // many [`Slot`]s)
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
struct V1Header {
    /// Worst-case distance between an entry and its bucket.
    longest_distance: u64,
    /// Number of hash collisions.
    /// Not currently considered by the reader.
    collisions: u64,
    /// Number of buckets for the sake of [`hash::ideal_slot_ix`] calculations.
    ///
    /// Note that the index includes:
    /// - A number of slots according to the `load_factor`.
    /// - [`Self::longest_distance`] additional buckets.
    /// - a terminal [`Slot::Empty`].
    initial_buckets: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
#[cfg_vis(feature = "benchmark-private", pub)]
struct OccupiedSlot {
    pub hash: NonMaximalU64,
    frame_offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
#[cfg_vis(feature = "benchmark-private", pub)]
enum Slot {
    Empty,
    Occupied(OccupiedSlot),
}

impl Slot {
    pub fn as_occupied(&self) -> Option<&OccupiedSlot> {
        match self {
            Slot::Empty => None,
            Slot::Occupied(occ) => Some(occ),
        }
    }
    fn into_raw(self) -> RawSlot {
        match self {
            Slot::Empty => RawSlot::EMPTY,
            Slot::Occupied(OccupiedSlot { hash, frame_offset }) => RawSlot {
                hash: hash.get(),
                frame_offset,
            },
        }
    }
}

/// A [`Slot`] as it appears on disk.
///
/// If [`Self::hash`] is [`u64::MAX`], then this represents a [`Slot::Empty`],
/// see [`Self::EMPTY`]
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
struct RawSlot {
    hash: u64,
    frame_offset: u64,
}

impl RawSlot {
    const EMPTY: Self = Self {
        hash: u64::MAX,
        frame_offset: u64::MAX,
    };
    /// How many bytes occupied by [`RawSlot`] when serialized.
    const WIDTH: u64 = std::mem::size_of::<u64>() as u64 * 2;
}

//////////////////////////////////////
// De/serialization                 //
// (Integers are all little-endian) //
//////////////////////////////////////

impl Readable for Version {
    fn read_from(mut reader: impl Read) -> io::Result<Self>
    where
        Self: Sized,
    {
        num::FromPrimitive::from_u64(reader.read_u64::<LittleEndian>()?).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "unknown header magic/version")
        })
    }
}

impl Writeable for Version {
    fn write_to(&self, mut writer: impl Write) -> io::Result<()> {
        writer.write_u64::<LittleEndian>(*self as u64)
    }

    fn written_len(&self) -> u64 {
        u64::try_from(std::mem::size_of::<u64>()).unwrap()
    }
}

impl Readable for Slot {
    fn read_from(reader: impl Read) -> io::Result<Self>
    where
        Self: Sized,
    {
        let raw @ RawSlot { hash, frame_offset } = Readable::read_from(reader)?;
        match NonMaximalU64::new(hash) {
            Some(hash) => Ok(Self::Occupied(OccupiedSlot { hash, frame_offset })),
            None => match raw == RawSlot::EMPTY {
                true => Ok(Self::Empty),
                false => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "empty slots must have a frame offset of u64::MAX",
                )),
            },
        }
    }
}

impl Writeable for Slot {
    fn write_to(&self, writer: impl Write) -> io::Result<()> {
        self.into_raw().write_to(writer)
    }

    fn written_len(&self) -> u64 {
        self.into_raw().written_len()
    }
}

impl Readable for RawSlot {
    fn read_from(mut reader: impl Read) -> io::Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {
            hash: reader.read_u64::<LittleEndian>()?,
            frame_offset: reader.read_u64::<LittleEndian>()?,
        })
    }
}

impl Writeable for RawSlot {
    fn write_to(&self, mut writer: impl Write) -> io::Result<()> {
        let Self { hash, frame_offset } = *self;
        writer.write_u64::<LittleEndian>(hash)?;
        writer.write_u64::<LittleEndian>(frame_offset)?;
        Ok(())
    }

    fn written_len(&self) -> u64 {
        Self::WIDTH
    }
}

impl Readable for V1Header {
    fn read_from(mut reader: impl Read) -> io::Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {
            longest_distance: reader.read_u64::<LittleEndian>()?,
            collisions: reader.read_u64::<LittleEndian>()?,
            initial_buckets: reader.read_u64::<LittleEndian>()?,
        })
    }
}

impl Writeable for V1Header {
    fn write_to(&self, mut writer: impl Write) -> io::Result<()> {
        let Self {
            longest_distance,
            collisions,
            initial_buckets,
        } = *self;
        writer.write_u64::<LittleEndian>(longest_distance)?;
        writer.write_u64::<LittleEndian>(collisions)?;
        writer.write_u64::<LittleEndian>(initial_buckets)?;
        Ok(())
    }

    fn written_len(&self) -> u64 {
        u64::try_from(std::mem::size_of::<u64>() * 3).unwrap()
    }
}

trait Readable {
    fn read_from(reader: impl Read) -> io::Result<Self>
    where
        Self: Sized;
}

trait Writeable {
    /// Must only return [`Err(_)`] if the underlying io fails.
    fn write_to(&self, writer: impl Write) -> io::Result<()>;
    /// The number of bytes that will be written on a call to [`Writeable::write_to`].
    ///
    /// Implementations may panic if this is incorrect.
    fn written_len(&self) -> u64;
}

// This lives in a module so its constructor can be private
mod util {
    /// Like [`std::num::NonZeroU64`], but is never [`u64::MAX`]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    pub struct NonMaximalU64(u64);

    impl NonMaximalU64 {
        pub fn new(u: u64) -> Option<Self> {
            match u == u64::MAX {
                true => None,
                false => Some(Self(u)),
            }
        }
        pub fn fit(u: u64) -> Self {
            Self(u.saturating_sub(1))
        }
        pub fn get(&self) -> u64 {
            self.0
        }
    }

    #[cfg(test)]
    impl quickcheck::Arbitrary for NonMaximalU64 {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            Self::fit(u64::arbitrary(g))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ahash::{HashMap, HashSet};
    use cid::Cid;

    /// [`Reader`] should behave like a [`HashMap`], with a caveat for collisions.
    ///
    /// Empty [`HashSet`]s act as a lookup for a non-existent key.
    fn do_hashmap_of_cids(reference: HashMap<Cid, HashSet<u64>>) {
        let subject = Reader::new(write_to_vec(|v| {
            Writer::new(
                reference
                    .clone()
                    .into_iter()
                    .flat_map(|(cid, offsets)| offsets.into_iter().map(move |offset| (cid, offset)))
                    .collect::<Vec<_>>(),
            )
            .write_into(v)
        }))
        .unwrap();
        for (cid, expected) in reference {
            let actual = subject.get(cid).unwrap().into_iter().collect();
            assert!(expected.is_subset(&actual)); // collisions
        }
    }

    /// Like [`do_hashmap_of_cids`], but operates on hashes instead of [`Cid`]s.
    fn do_hashmap_of_hashes(reference: HashMap<NonMaximalU64, HashSet<u64>>) {
        let subject = Reader::new(write_to_vec(|v| {
            Writer::from_table(Table::new_from_hashes(
                reference.clone().into_iter().flat_map(|(hash, offsets)| {
                    offsets.into_iter().map(move |offset| (hash, offset))
                }),
                DEFAULT_LOAD_FACTOR,
            ))
            .write_into(v)
        }))
        .unwrap();
        for (hash, expected) in reference {
            let actual = subject.get_by_hash(hash).unwrap().into_iter().collect();
            assert!(expected.is_subset(&actual))
        }
    }

    quickcheck::quickcheck! {
        fn hashmap_of_cids(reference: HashMap<Cid, HashSet<u64>>) -> () {
            do_hashmap_of_cids(reference)
        }
        fn hashmap_of_hashes(reference: HashMap<NonMaximalU64, HashSet<u64>>) -> () {
            do_hashmap_of_hashes(reference)
        }
        fn everything_maps_to_first_slot(values: Vec<HashSet<u64>>) -> () {
            let Some(initial_width) = initial_width(values.iter().map(HashSet::len).sum(), DEFAULT_LOAD_FACTOR) else {
                return;
            };
            let reference = HashMap::from_iter(iter::zip(hash::from_ideal_slot_ix(0, initial_width).unique(), values));
            do_hashmap_of_hashes(reference)
        }
        fn everything_maps_to_first_10_slots(values: Vec<HashSet<u64>>) -> () {
            let Some(initial_width) = initial_width(values.iter().map(HashSet::len).sum(), DEFAULT_LOAD_FACTOR) else {
                return;
            };
            let mut generators = Vec::from_iter((0..cmp::min(initial_width.get(), 10)).map(|it|hash::from_ideal_slot_ix(it, initial_width).unique()));
            let hashes_in_first_10 = generators.iter_mut().flatten();
            let reference = HashMap::from_iter(iter::zip(hashes_in_first_10, values));
            do_hashmap_of_hashes(reference)
        }
        fn header(it: V1Header) -> () {
            round_trip(&it);
        }
        fn slot(it: Slot) -> () {
            round_trip(&it);
        }
        fn raw_slot(it: RawSlot) -> () {
            round_trip(&it);
        }
    }

    #[track_caller]
    fn round_trip<T: PartialEq + std::fmt::Debug + Readable + Writeable>(original: &T) {
        let serialized = write_to_vec(|v| original.write_to(v));
        assert_eq!(
            serialized.len(),
            usize::try_from(original.written_len()).unwrap()
        );
        let deserialized = T::read_from(serialized.as_slice())
            .expect("couldn't deserialize T from a deserialized T");
        pretty_assertions::assert_eq!(original, &deserialized);
    }

    pub fn write_to_vec(f: impl FnOnce(&mut Vec<u8>) -> io::Result<()>) -> Vec<u8> {
        let mut v = Vec::new();
        f(&mut v).unwrap();
        v
    }
}
