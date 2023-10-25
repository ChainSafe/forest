//! Embedded index for the `.forest.car.zst` format.
//!
//! Maps from [`cid::Cid`]s to zstd frame offsets.
//!
//! # Design statement
//!
//! - Create once, read many times.
//!   This means that existing databases are overkill - most of their API
//!   complexity is for write support.
//! - Embeddable.
//!   This precludes most existing databases, which operate on files or folders.
//! - Lookups must NOT require reading the index into memory.
//!   This precludes using e.g [`serde::Serialize`]
//!
//! ## Implementation
//!
//!
//! ## Wishlist
//! - use [`std::num::NonZeroU64`] for the reserved hash.
//! - use [`std::hash::Hasher`]s instead of custom hashing
//!   The current code says using e.g the default hasher

use self::util::NonMaximalU64;
use byteorder::{LittleEndian, ReadBytesExt as _, WriteBytesExt as _};
use cid::Cid;
use std::{
    cmp,
    io::{self, Read, Write},
    iter::ExactSizeIterator,
    num::NonZeroUsize,
};

mod hash;

/// An in-memory representation of a hash-table
#[derive(Debug, Clone, Hash, PartialOrd, Ord, PartialEq, Eq)]
struct Table {
    slots: Vec<Slot>,
    initial_width: usize,
    collisions: usize,
    longest_distance: usize,
}

impl Table {
    fn new<I>(locations: I, load_factor: f64) -> Self
    where
        I: IntoIterator<Item = (Cid, u64)>,
        I::IntoIter: ExactSizeIterator,
    {
        let locations = locations.into_iter();
        assert!((0.0..=1.0).contains(&load_factor));
        let initial_width = cmp::max(
            (locations.len() as f64 / load_factor) as usize,
            locations.len(),
        );
        let Some(initial_width) = NonZeroUsize::new(initial_width) else {
            return Self {
                slots: vec![Slot::Empty],
                initial_width: 0,
                collisions: 0,
                longest_distance: 0,
            };
        };
        let mut slots = vec![Slot::Empty; initial_width.get()];

        let mut collisions = 0;
        let mut longest_distance = 0;

        for (cid, frame_offset) in locations {
            let mut insert_me = OccupiedSlot {
                hash: hash::of(&cid),
                frame_offset,
            };
            let ideal_ix = hash::ideal_bucket_ix(insert_me.hash, initial_width);
            let mut current_ix = ideal_ix;
            // this is guaranteed to terminate because table_width >= locations.len()
            loop {
                match slots[current_ix] {
                    Slot::Empty => {
                        slots[current_ix] = Slot::Occupied(insert_me);
                        longest_distance = cmp::max(
                            longest_distance,
                            distance(insert_me.hash, current_ix, initial_width),
                        );
                        break;
                    }
                    Slot::Occupied(already) => {
                        if insert_me.hash == already.hash {
                            collisions += 1;
                        }
                        // TODO(aatifsyed): document this
                        let already_dist = distance(already.hash, current_ix, initial_width);
                        let insert_me_dist = distance(insert_me.hash, current_ix, initial_width);

                        if already_dist < insert_me_dist
                            || (already_dist == insert_me_dist && insert_me.hash < already.hash)
                        {
                            slots[current_ix] = Slot::Occupied(insert_me);
                            insert_me = already;
                        }

                        longest_distance = cmp::max(longest_distance, insert_me_dist);
                        current_ix = (current_ix + 1) % initial_width
                    }
                }
            }
        }
        // TODO(aatifsyed): document this
        for i in 0..longest_distance {
            slots.push(slots[i])
        }
        slots.push(Slot::Empty);
        Self {
            slots,
            initial_width: initial_width.get(),
            collisions,
            longest_distance,
        }
    }
    fn header(&self) -> Header {
        Header {
            magic_number: Header::V1_MAGIC,
            longest_distance: self.longest_distance.try_into().unwrap(),
            collisions: self.collisions.try_into().unwrap(),
            initial_buckets: self.initial_width.try_into().unwrap(),
        }
    }
}

pub fn distance(hash: NonMaximalU64, current_ix: usize, initial_width: NonZeroUsize) -> usize {
    {
        let ideal_ix = hash::ideal_bucket_ix(hash, initial_width);
        match ideal_ix > current_ix {
            true => initial_width.get() - ideal_ix + current_ix,
            false => current_ix - ideal_ix,
        }
    }
}

impl Writeable for Table {
    fn write_to(&self, mut writer: impl Write) -> io::Result<()> {
        self.header().write_to(&mut writer)?;
        for slot in &self.slots {
            slot.write_to(&mut writer)?
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
struct OccupiedSlot {
    hash: NonMaximalU64,
    frame_offset: u64,
}

pub struct Index<ReaderT> {
    reader: ReaderT,
    header: Header,
}

impl<ReaderT> Index<ReaderT>
where
    ReaderT: positioned_io::ReadAt,
{
    pub fn new(mut reader: ReaderT) -> io::Result<Self> {
        let mut cursor = positioned_io::Cursor::new(&mut reader);
        let header = Header::read_from(&mut cursor)?;
        for _ in 0..header.initial_buckets + header.longest_distance {
            Slot::read_from(&mut cursor)?;
        }
        let Slot::Empty = Slot::read_from(&mut cursor)? else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "index must be terminated with an empty slot",
            ));
        };
        // we don't check that this is the end of the file...
        Ok(Self { reader, header })
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
struct Header {
    /// Version number
    magic_number: u64,
    /// Worst-case distance between an entry and its bucket.
    longest_distance: u64,
    /// Number of hash collisions. Reserved for future use.
    collisions: u64,
    /// Number of buckets before duplication.
    /// Note that the index includes:
    /// - [`Self::longest_distance`] additional buckets
    /// - a terminal [`Slot::Empty`].
    initial_buckets: u64,
}

impl Header {
    const V0_MAGIC: u64 = 0xdeadbeef;
    const V1_MAGIC: u64 = 0xdeadbeef + 1;
    // const V2_MAGIC: u64 = 0xdeadbeef + 2;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
enum Slot {
    Empty,
    Occupied(OccupiedSlot),
}

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
        let raw = match *self {
            Slot::Empty => RawSlot::EMPTY,
            Slot::Occupied(OccupiedSlot { hash, frame_offset }) => RawSlot {
                hash: hash.get(),
                frame_offset,
            },
        };
        raw.write_to(writer)
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
}

impl Readable for Header {
    fn read_from(mut reader: impl Read) -> io::Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {
            magic_number: reader.read_u64::<LittleEndian>()?,
            longest_distance: reader.read_u64::<LittleEndian>()?,
            collisions: reader.read_u64::<LittleEndian>()?,
            initial_buckets: reader.read_u64::<LittleEndian>()?,
        })
    }
}

impl Writeable for Header {
    fn write_to(&self, mut writer: impl Write) -> io::Result<()> {
        let Self {
            magic_number,
            longest_distance,
            collisions,
            initial_buckets: buckets,
        } = *self;
        writer.write_u64::<LittleEndian>(magic_number)?;
        writer.write_u64::<LittleEndian>(longest_distance)?;
        writer.write_u64::<LittleEndian>(collisions)?;
        writer.write_u64::<LittleEndian>(buckets)?;
        Ok(())
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
}

// This lives in a module so its constructor is private
mod util {
    /// Like [`std::num::NonZeroU64`], but is never [`u64::MAX`]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
    use cid::Cid;
    use pretty_assertions::assert_eq;

    fn do_test(pairs: Vec<(Cid, u64)>) {
        let subject = Table::new(pairs.clone(), 0.8);
        let reference = crate::utils::db::car_index::CarIndexBuilder::new(
            pairs
                .into_iter()
                .map(|(cid, u)| (crate::utils::db::car_index::Hash::from(cid), u)),
        );
        let subject = write_to_vec(|v| subject.write_to(v));
        let reference = write_to_vec(|v| reference.write(v));

        assert_eq!(subject, reference);
    }

    quickcheck::quickcheck! {
        fn do_quickcheck(pairs: Vec<(Cid, u64)>) -> () {
            do_test(pairs)
        }
        fn header(it: Header) -> () {
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
        let deserialized = T::read_from(serialized.as_slice())
            .expect("couldn't deserialize T from a deserialized T");
        pretty_assertions::assert_eq!(original, &deserialized);
    }

    fn write_to_vec(f: impl FnOnce(&mut Vec<u8>) -> io::Result<()>) -> Vec<u8> {
        let mut v = Vec::new();
        f(&mut v).unwrap();
        v
    }
}
