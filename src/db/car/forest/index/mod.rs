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
//! - Variable length, possibly large entries on disk, which balloons our size
//!   and/or implementation complexity.
//!
//! We can address this by using an open-addressed hash table with linear probing.
//! - "hash table": Have a linear array
//!
//! We create a linear array of equal-length [`Slot`]s.
//! - [hashing](hash::summary) the [`Cid`] gives us a fixed length entry.
//! - A [`hash::ideal_slot_ix`] gives us a likely location to find the entry,
//!   given a table size.
//!   (That is, a hash in a collection of length 10 has a different `ideal_slot_ix`
//!   than if the same hash were in a collection of length 20.)
//!   We insert padding [`Slot::Empty`]s to ensure each entry is at or after its
//!   [`ideal_slot_ix`](hash::ideal_slot_ix).
//! - We sort the [hashes](NonMaximalU64) from lowest to highest, so lookups can
//!   scan forwards from the [`ideal_slot_ix`](hash::ideal_slot_ix) to find the hash they're looking for.
//!   This is called _linear probing_.
//! - We have two types of collisions.
//!   Both must be handled by callers of [`Reader::get`].
//!   - Hash collisions, where two different [`Cid`]s have the same hash.
//!   - [`hash::ideal_slot_ix`] collisions.
//! - A slot is always found at or within [`V1Header::longest_distance`] after its
//!   [`hash::ideal_slot_ix`].
//!   This is calculated at construction time.
//!
//! So the layout on disk is as follows:
//!
//! ```text
//! ┌──────────────┐
//! │Version::V1   │
//! ├──────────────┤
//! │Header        │ <- Contains the "intial width", required to perform lookups
//! ├──────────────┤
//! │Slot::Occupied│
//! ├──────────────┤
//! │Slot::Empty   │
//! ├──────────────┤    The hash table does not know how many slots it contains:
//! │Slot::Empty   │    Length information must be stored out of band (e.g in the
//! ├──────────────┤ <- Zstd skip frame header)
//! ```

#[cfg_vis(feature = "benchmark-private", pub)]
use self::util::NonMaximalU64;
use byteorder::{LittleEndian, ReadBytesExt as _, WriteBytesExt as _};
use cfg_vis::cfg_vis;
use cid::Cid;
use itertools::Itertools as _;
use positioned_io::{ReadAt, Size};
use smallvec::{smallvec, SmallVec};
use std::{
    cmp,
    io::{self, Read, Write},
    iter,
    num::NonZeroUsize,
    pin::pin,
};
use tokio::io::{AsyncWrite, AsyncWriteExt as _};

#[cfg(not(any(test, feature = "benchmark-private")))]
mod hash;
#[cfg(any(test, feature = "benchmark-private"))]
pub mod hash;
mod util;

/// Reader for the `.forest.car.zst`'s embedded index.
///
/// Performs lookups and slot iteration.
///
/// See [module documentation](mod@self) for more.
pub struct Reader<R> {
    inner: R,
    table_offset: u64,
    #[cfg(feature = "benchmark-private")]
    pub header: V1Header,
    #[cfg(not(feature = "benchmark-private"))]
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
            u64::try_from(hash::ideal_slot_ix(needle, initial_buckets)).unwrap() * RawSlot::LEN;
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
#[allow(unused)] // TODO(aatifsyed): https://github.com/ChainSafe/forest/issues/3565
struct Iter<R> {
    inner: R,
    positions: iter::StepBy<std::ops::Range<u64>>,
}

#[allow(unused)] // TODO(aatifsyed): https://github.com/ChainSafe/forest/issues/3565
impl<R> Iterator for Iter<R>
where
    R: ReadAt + Size,
{
    type Item = io::Result<Slot>;

    fn next(&mut self) -> Option<Self::Item> {
        self.positions
            .next()
            .map(|pos| Slot::read_from(positioned_io::Cursor::new_pos(&self.inner, pos)))
    }
}

impl<R> Reader<R>
where
    R: ReadAt + Size,
{
    #[cfg_vis(feature = "benchmark-private", pub)]
    #[allow(unused)] // TODO(aatifsyed): https://github.com/ChainSafe/forest/issues/3565
    fn iter(&self) -> io::Result<Iter<&R>> {
        let end = self.inner.size()?.ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "couldn't get end of table size")
        })?;
        Ok(Iter {
            inner: &self.inner,
            positions: (self.table_offset..end).step_by(Slot::LEN.try_into().unwrap()),
        })
    }
}

const DEFAULT_LOAD_FACTOR: f64 = 0.8;

/// Accumulator of [`Cid`]s and frame offsets ([`u64`]s) for the hash table.
///
/// Call [`Self::into_writer`] when you're ready to write the table to disk.
pub struct Builder {
    load_factor: f64,
    /// The first field is unused, but we preserve it to not allocate in
    /// [`Self::into_writer`]
    slots: Vec<(usize, OccupiedSlot)>,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        Self::new_with_load_factor(DEFAULT_LOAD_FACTOR)
    }

    fn new_with_load_factor(load_factor: f64) -> Self {
        Self {
            load_factor,
            slots: vec![],
        }
    }

    pub fn into_writer(self) -> Writer {
        let Self {
            load_factor,
            mut slots,
        } = self;
        // First, sort by hash
        slots.sort_unstable_by_key(|(_, it)| *it);
        slots.dedup_by_key(|(_, it)| *it);
        let Some(initial_width) = initial_width(slots.len(), load_factor) else {
            return Writer {
                version: Version::V1,
                header: V1Header {
                    longest_distance: 0,
                    collisions: 0,
                    initial_buckets: 0,
                },
                slots: vec![],
            };
        };
        let collisions = slots
            .iter()
            .group_by(|(_, it)| it.hash)
            .into_iter()
            // subtract one because a lone item is not a collision
            .map(|(_, group)| group.count() - 1)
            .sum::<usize>();

        // keep track of how many `Slot::Empty`s should precede each slot so that
        // it appears at or after its ideal_slot_ix.
        // We don't need to actually have any `Slot::Empty`s in-memory
        let mut total_padding = 0;
        let mut longest_distance = 0;
        for (ix, (pre_padding, slot)) in slots.iter_mut().enumerate() {
            let ix = ix + total_padding;
            let ideal_ix = hash::ideal_slot_ix(slot.hash, initial_width);
            *pre_padding = ideal_ix.saturating_sub(ix);
            let actual_ix = ix + *pre_padding;
            let distance = actual_ix - ideal_ix;
            longest_distance = cmp::max(longest_distance, distance);
            total_padding += *pre_padding;
        }

        Writer {
            version: Version::V1,
            header: V1Header {
                longest_distance: longest_distance.try_into().unwrap(),
                collisions: collisions.try_into().unwrap(),
                initial_buckets: initial_width.get().try_into().unwrap(),
            },
            slots,
        }
    }
}

impl Extend<(Cid, u64)> for Builder {
    fn extend<T: IntoIterator<Item = (Cid, u64)>>(&mut self, iter: T) {
        self.extend(iter.into_iter().map(|(cid, u)| (hash::summary(&cid), u)))
    }
}

impl Extend<(NonMaximalU64, u64)> for Builder {
    fn extend<T: IntoIterator<Item = (NonMaximalU64, u64)>>(&mut self, iter: T) {
        self.slots.extend(
            iter.into_iter()
                .map(|(hash, frame_offset)| (0, OccupiedSlot { hash, frame_offset })),
        )
    }
}

impl FromIterator<(Cid, u64)> for Builder {
    fn from_iter<T: IntoIterator<Item = (Cid, u64)>>(iter: T) -> Self {
        let mut this = Self::default();
        this.extend(iter);
        this
    }
}
impl FromIterator<(NonMaximalU64, u64)> for Builder {
    fn from_iter<T: IntoIterator<Item = (NonMaximalU64, u64)>>(iter: T) -> Self {
        let mut this = Self::default();
        this.extend(iter);
        this
    }
}

/// Writes the actual slot table to disk.
///
/// Importantly, this knows the [`Self::written_len`] of the table, which is
/// required for some containers.
pub struct Writer {
    version: Version,
    header: V1Header,
    /// Number of preceding [`Slot::Empty`]s, followed by the [`Slot::Occupied`].
    ///
    /// This is so that [`Slot::Empty`]s aren't created, saving memory.
    ///
    /// Note that there must additionally be a terminal [`Slot::Empty`].
    slots: Vec<(usize, OccupiedSlot)>,
}

impl Writer {
    pub fn written_len(&self) -> u64 {
        let Self {
            version,
            header,
            slots,
        } = self;
        written_len(version)
            + written_len(header)
            // this logic must be kept in sync with [`slots`], below
            + cmp::max(
                u64::try_from(
                    slots
                        .iter()
                        .map(|(pre, _)| *pre + 1 /* occupied */)
                        .sum::<usize>()
                        + 1, /* trailing */
                )
                .unwrap(),
                header.initial_buckets + 1, /* trailing */
            ) * Slot::LEN
    }
    fn slots(
        min_slots: usize,
        slots: impl IntoIterator<Item = (usize, OccupiedSlot)>,
    ) -> impl Iterator<Item = Slot> {
        // this logic must be kept in sync with [`written_len`], above
        slots
            .into_iter()
            .flat_map(|(pre, occ)| {
                iter::repeat(Slot::Empty)
                    .take(pre)
                    .chain(iter::once(Slot::Occupied(occ)))
            })
            // ensure there are at least `initial_width` slots, else lookups could
            // try and read off the end of the table
            .pad_using(min_slots, |_ix| Slot::Empty)
            .chain(iter::once(Slot::Empty))
    }
    pub async fn write_into(self, writer: impl AsyncWrite) -> io::Result<()> {
        let mut buf = vec![];
        let mut writer = pin!(writer);
        let Self {
            version,
            header,
            slots,
        } = self;
        /// Bridge between our sync [`Writeable`] trait, and async writing code
        async fn write_via_buf(
            buf: &mut Vec<u8>,
            mut writer: impl AsyncWrite,
            data: impl Writeable,
        ) -> io::Result<()> {
            buf.clear();
            data.write_to(&mut *buf)?;
            pin!(writer).write_all(buf).await
        }
        write_via_buf(&mut buf, &mut writer, version).await?;
        write_via_buf(&mut buf, &mut writer, &header).await?;
        for slot in Self::slots(
            header.initial_buckets.try_into().unwrap(),
            slots.iter().copied(),
        ) {
            write_via_buf(&mut buf, &mut writer, slot).await?;
        }
        Ok(())
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
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
#[cfg_vis(feature = "benchmark-private", pub)]
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
    pub initial_buckets: u64,
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
    const LEN: u64 = std::mem::size_of::<u64>() as u64;
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
    const LEN: u64 = RawSlot::LEN;
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
    const LEN: u64 = std::mem::size_of::<u64>() as u64 * 2;
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
    const LEN: u64 = std::mem::size_of::<u64>() as u64 * 3;
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
    const LEN: u64;
}

/// Useful for exhaustiveness checking
fn written_len<T: Writeable>(_: T) -> u64 {
    T::LEN
}

impl<T> Writeable for &T
where
    T: Writeable,
{
    fn write_to(&self, writer: impl Write) -> io::Result<()> {
        T::write_to(self, writer)
    }
    const LEN: u64 = T::LEN;
}

#[cfg(test)]
mod tests {
    use super::*;
    use ahash::{HashMap, HashSet};
    use cid::Cid;
    use futures::executor::block_on;
    use tap::Tap as _;

    /// [`Reader`] should behave like a [`HashMap`], with a caveat for collisions.
    fn do_hashmap_of_cids(reference: HashMap<Cid, HashSet<u64>>) {
        let subject = Reader::new(write_to_vec(|v| {
            let writer =
                Builder::from_iter(reference.clone().into_iter().flat_map(|(hash, offsets)| {
                    offsets.into_iter().map(move |offset| (hash, offset))
                }))
                .into_writer();
            let expected_len = writer.written_len();
            block_on(writer.write_into(&mut *v))?;
            assert_eq!(expected_len as usize, v.len());
            Ok(())
        }))
        .unwrap();
        for (cid, expected) in reference {
            let actual = subject.get(cid).unwrap().into_iter().collect();
            assert!(expected.is_subset(&actual)); // collisions
        }
    }

    /// [`Reader`] should behave like a [`HashMap<Hash, HashSet<FrameOffset>>`](HashMap).
    /// What does that mean?
    ///
    /// `HashSet<FrameOffset>` are the expected candidate frame offsets for a given hash.
    /// They
    ///
    ///
    /// Additionally checks [`Reader::iter`]
    fn do_hashmap_of_hashes(reference: HashMap<NonMaximalU64, HashSet<u64>>) {
        let subject = Reader::new(write_to_vec(|v| {
            let writer =
                Builder::from_iter(reference.clone().into_iter().flat_map(|(hash, offsets)| {
                    offsets.into_iter().map(move |offset| (hash, offset))
                }))
                .into_writer();
            let expected_len = writer.written_len();
            block_on(writer.write_into(&mut *v))?;
            assert_eq!(expected_len as usize, v.len());
            Ok(())
        }))
        .unwrap();
        for (hash, expected) in &reference {
            let actual = subject.get_by_hash(*hash).unwrap().into_iter().collect();
            assert!(expected.is_subset(&actual)) // collisions
        }

        let via_iter = subject
            .iter()
            .unwrap()
            .filter_map(|it| match it.unwrap() {
                Slot::Empty => None,
                Slot::Occupied(it) => Some(it),
            })
            .group_by(|it| it.hash)
            .into_iter()
            .map(|(hash, group)| (hash, HashSet::from_iter(group.map(|it| it.frame_offset))))
            .collect::<HashMap<_, _>>();
        assert_eq!(
            via_iter,
            reference.tap_mut(|it| it.retain(|_, v| !v.is_empty()))
        );
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
            usize::try_from(written_len(original)).unwrap()
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
