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

use crate::cid_collections::CidHashMap;

use self::util::NonMaximalU64;
use byteorder::{LittleEndian, ReadBytesExt as _, WriteBytesExt as _};
#[cfg(test)]
use quickcheck::quickcheck;
use std::{
    cmp,
    io::{self, Read, Write},
    num::NonZeroUsize,
};

mod hash;

fn build_table(locations: CidHashMap<u64>, load_factor: f64) -> Box<[Slot]> {
    assert!((0.0..=1.0).contains(&load_factor));
    let table_width = cmp::max(
        (locations.len() as f64 / load_factor) as usize,
        locations.len(),
    );
    let Some(table_width) = NonZeroUsize::new(table_width) else {
        return Box::new([]);
    };
    let mut table = vec![Slot::Empty; table_width.get()].into_boxed_slice();

    for (cid, frame_offset) in locations {
        let hash = hash::of(&cid);
        let mut current_ix = hash::ideal_bucket_ix(hash, table_width);
        loop {
            // this is guaranteed to terminate because table_width >= locations.len()
            match table[current_ix] {
                Slot::Empty => {
                    table[current_ix] = Slot::Occupied { hash, frame_offset };
                    break;
                }
                Slot::Occupied { .. } => current_ix = (current_ix + 1) % table_width,
            }
        }
    }
    table
}

pub fn write(
    locations: CidHashMap<u64>,
    load_factor: f64,
    mut writer: impl io::Write,
) -> io::Result<()> {
    let table = build_table(locations, load_factor);
    Header {
        magic_number: Header::V1_MAGIC,
        longest_distance: u64::MAX,
        collisions: u64::MAX,
        buckets: u64::try_from(table.len()).unwrap(),
    }
    .write_to(&mut writer)?;
    for slot in &*table {
        slot.write_to(&mut writer)?
    }
    Slot::Empty.write_to(writer)
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
        for _ in 0..header.buckets {
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
    // Version number
    magic_number: u64,
    // Worst-case distance between an entry and its bucket.
    longest_distance: u64,
    // Number of hash collisions. Reserved for future use.
    collisions: u64,
    // Number of buckets. Note that the index includes padding after the last
    // bucket.
    buckets: u64,
}

impl Header {
    const V0_MAGIC: u64 = 0xdeadbeef;
    const V1_MAGIC: u64 = 0xdeadbeef + 1;
    // const V2_MAGIC: u64 = 0xdeadbeef + 2;
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
enum Slot {
    Empty,
    Occupied {
        hash: NonMaximalU64,
        frame_offset: u64,
    },
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
            Some(hash) => Ok(Self::Occupied { hash, frame_offset }),
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
            Slot::Occupied { hash, frame_offset } => RawSlot {
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
            buckets: reader.read_u64::<LittleEndian>()?,
        })
    }
}

impl Writeable for Header {
    fn write_to(&self, mut writer: impl Write) -> io::Result<()> {
        let Self {
            magic_number,
            longest_distance,
            collisions,
            buckets,
        } = *self;
        writer.write_u64::<LittleEndian>(magic_number)?;
        writer.write_u64::<LittleEndian>(longest_distance)?;
        writer.write_u64::<LittleEndian>(collisions)?;
        writer.write_u64::<LittleEndian>(buckets)?;
        Ok(())
    }
}

#[cfg(test)]
quickcheck! {
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
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
#[track_caller]
fn round_trip<T: PartialEq + std::fmt::Debug + Readable + Writeable>(original: &T) {
    let serialized = {
        let mut v = vec![];
        original
            .write_to(&mut v)
            .expect("Vec<u8> has infallible IO, and illegal states should be unrepresentable");
        v
    };
    let deserialized =
        T::read_from(serialized.as_slice()).expect("couldn't deserialize T from a deserialized T");
    pretty_assertions::assert_eq!(original, &deserialized);
}
