//! This file implements an on-disk hashmap from keys to values.
//! The map is "imperfect", in that [`Reader::get`] will expose hash collisions.
//! That is, multiple values will be returned that were not inserted under a given key.
//!
//! Values implement [`AsBytes`] for de/serialization.
//!
//! Two pieces of information should be stored out-of-band:
//! - [`Writer::nominal_length()`].
//! - (Optional) [`Writer::longest_distance()`].

use std::{
    cmp::{self, Ordering},
    fmt::{self, Debug},
    io::{self, Write},
    iter,
    marker::PhantomData,
    mem,
    num::NonZeroUsize,
};

use itertools::{Either, Itertools, Position};
use positioned_io::{ReadAt, Size};
use util::FlattenSorted;
use zerocopy::{byteorder::little_endian::U64 as u64le, AsBytes, FromBytes, FromZeroes, Unaligned};

const EMPTY_HASH: u64le = zerocopy::transmute!(0xFFFFFFFF_FFFFFFFF_u64);

#[test]
fn empty_is_endian_independent() {
    assert_eq!(EMPTY_HASH.get(), u64::MAX);
    assert_eq!(EMPTY_HASH.get().swap_bytes(), u64::MAX);
}

pub fn merge<'a, I, T>(
    nominal_length: usize,
    slots: impl IntoIterator<Item = I>,
    longest_distance: &'a mut usize,
) -> impl Iterator<Item = io::Result<impl AsBytes>> + 'a
where
    I: Iterator<Item = io::Result<Slot<T>>> + 'a,
    T: FromZeroes + Copy + AsBytes + 'a,
{
    *longest_distance = 0;

    let Some(nominal_length) = NonZeroUsize::new(nominal_length) else {
        return Either::Left(iter::once(Ok(Slot::empty())));
    };

    let mut enumerate = 0;
    let mut total_padding = 0;
    Either::Right(
        FlattenSorted::new_by(slots, |l, r| match (l, r) {
            (Ok(l), Ok(r)) => Ord::cmp(&l.hash.get(), &r.hash.get()),
            // bubble errors up
            (Err(_), _) => Ordering::Less,
            (_, Err(_)) => Ordering::Greater,
        })
        .map_ok(move |slot| {
            let ix = enumerate + total_padding;
            let ideal_ix = ideal_slot_ix(slot.hash.get(), nominal_length);
            let pre_padding = ideal_ix.saturating_sub(ix);
            let actual_ix = ix + pre_padding;
            let distance = actual_ix - ideal_ix;
            *longest_distance = cmp::max(*longest_distance, distance);
            total_padding += pre_padding;
            enumerate += 1;
            iter::repeat(Slot::empty())
                .take(pre_padding)
                .chain(iter::once(slot))
        })
        .flatten_ok()
        .pad_using(nominal_length.get(), |_ix| Ok(Slot::empty()))
        .chain(iter::once(Ok(Slot::empty()))),
    )
}

/// Layout on disk
#[derive(Clone, Copy, AsBytes, FromBytes, FromZeroes, PartialEq, Eq, Hash, Unaligned, Debug)]
#[repr(packed)] // required to `#[derive(AsBytes)]` for generic structs
pub struct Slot<T> {
    /// if [`EMPTY_HASH`] then this represents an empty slot
    hash: u64le,
    // key, K, // if we wanted a perfect hashmap, we'd store the key inline here.
    // public so that users can map slots as desired.
    pub value: T,
}

impl<T> Slot<T> {
    fn as_nonempty(&self) -> io::Result<Option<Self>>
    where
        T: FromZeroes + AsBytes + Copy,
    {
        match (
            self.hash == EMPTY_HASH,
            self.as_bytes() == Self::empty().as_bytes(),
        ) {
            (true, true) => Ok(None),
            (true, false) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid format for table on disk - empty slots should be zero-filled",
            )),
            (false, _) => Ok(Some(*self)),
        }
    }
}

impl<T> Ord for Slot<T>
where
    T: Ord + Copy,
{
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        // Move the values out of the things to compare.
        // This saves us having to dance around unaligned references.
        let hash = self.hash;
        let other_hash = other.hash;
        let value = self.value;
        let other_value = other.value;
        hash.get()
            .cmp(&other_hash.get())
            .then_with(|| value.cmp(&other_value))
    }
}

impl<T> PartialOrd for Slot<T>
where
    T: Ord + Copy,
{
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Slot<T>
where
    T: FromZeroes,
{
    fn empty() -> Self {
        Self {
            hash: EMPTY_HASH,
            value: T::new_zeroed(),
        }
    }
}

pub struct Builder<K, V, H> {
    /// the [`usize`] is always zero, it just saves an allocation in [`Builder::into_writer`]
    values: Vec<(usize, Slot<V>)>,
    hasher: H,
    _key: PhantomData<K>,
}

impl<K, V, H> fmt::Debug for Builder<K, V, H>
where
    V: Debug + Copy,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Builder")
            .field("values", &self.values)
            .finish_non_exhaustive()
    }
}

impl<K, V, H> Builder<K, V, H> {
    /// Start building a new map
    pub fn new(hasher: H) -> Self {
        Self {
            values: Vec::new(),
            hasher,
            _key: PhantomData,
        }
    }
    pub fn with_values(mut self, values: impl IntoIterator<Item = (K, V)>) -> Self
    where
        H: FnMut(K) -> u64,
    {
        self.extend(values);
        self
    }
    pub fn into_writer(self, load_factor: f64) -> Writer<V>
    where
        V: FromZeroes + AsBytes + Copy + Eq,
    {
        let Self {
            mut values,
            hasher: _,
            _key,
        } = self;
        let Some(nominal_length) = load_factor2nominal_length(values.len(), load_factor) else {
            return Writer {
                nominal_length: None,
                values: Vec::new(),
                longest_distance: 0,
                occupied_length: 0,
            };
        };

        values.sort_unstable_by_key(|(_, slot)| slot.hash.get());
        values.dedup_by_key(|(_, slot)| *slot);
        // if unique_values { values.dedup() }

        let mut total_padding = 0;
        let mut longest_distance = 0;
        for (ix, (pre_padding, slot)) in values.iter_mut().enumerate() {
            let ix = ix + total_padding;
            let ideal_ix = ideal_slot_ix(slot.hash.get(), nominal_length);
            *pre_padding = ideal_ix.saturating_sub(ix);
            let actual_ix = ix + *pre_padding;
            let distance = actual_ix - ideal_ix;
            longest_distance = cmp::max(longest_distance, distance);
            total_padding += *pre_padding;
        }
        Writer {
            occupied_length: values.len(),
            nominal_length: Some(nominal_length),
            values,
            longest_distance,
        }
    }
}

impl<K, V, H> Extend<(K, V)> for Builder<K, V, H>
where
    H: FnMut(K) -> u64,
{
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        self.values.extend(iter.into_iter().map(|(k, value)| {
            let hash = u64le::from((self.hasher)(k).saturating_sub(1));
            (0, Slot { hash, value })
        }))
    }
}

pub struct Writer<T> {
    occupied_length: usize,
    nominal_length: Option<NonZeroUsize>,
    longest_distance: usize,
    values: Vec<(usize, Slot<T>)>,
}

impl<T> fmt::Debug for Writer<T>
where
    T: Debug + Copy, // #[derive(Debug)] misses the T: Copy bound
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Writer")
            .field("occupied_length", &self.occupied_length)
            .field("nominal_length", &self.nominal_length)
            .field("longest_distance", &self.longest_distance)
            .field("values", &self.values)
            .finish()
    }
}

impl<T> Writer<T> {
    /// Information that must be forwarded to [`Reader::new`]
    pub fn nominal_length(&self) -> usize {
        self.nominal_length
            .map(NonZeroUsize::get)
            .unwrap_or_default()
    }
    /// Information that could be forwarded to [`Reader::with_longest_distance`]
    pub fn longest_distance(&self) -> usize {
        self.longest_distance
    }
    pub fn occupied_length(&self) -> usize {
        self.occupied_length
    }
    /// How many bytes this map takes on the wire.
    pub fn written_len(&self) -> usize {
        let num_slots = cmp::max(
            self.values.iter().map(|(pre, _)| pre + 1).sum(),
            self.nominal_length(),
        ) + 1;
        num_slots * mem::size_of::<Slot<T>>()
    }
    fn slots(&self) -> impl Iterator<Item = Slot<T>> + '_
    where
        T: FromZeroes + Copy,
    {
        self.values
            .iter()
            .flat_map(|(pre, slot)| {
                iter::repeat(Slot::empty())
                    .take(*pre)
                    .chain(iter::once(*slot))
            })
            .pad_using(self.nominal_length(), |_ix| Slot::empty())
            .chain(iter::once(Slot::empty()))
    }
    /// Write this table to a writer, flushing it.
    pub fn write_into(&self, mut writer: impl Write) -> io::Result<()>
    where
        T: FromZeroes + Copy + AsBytes,
    {
        for slot in self.byte_chunks() {
            writer.write_all(slot.as_bytes())?;
        }
        writer.flush()
    }
    /// Iterate over chunks of binary data that should be written to disk.
    pub fn byte_chunks(&self) -> impl Iterator<Item = impl AsBytes> + '_
    where
        T: FromZeroes + Copy + AsBytes,
    {
        self.slots()
    }
}

pub struct Reader<I, V, K = (), H = ()> {
    nominal_length: Option<NonZeroUsize>,
    io: I,
    hasher: H,
    longest_distance: Option<usize>,
    _phantom: PhantomData<(K, V)>,
}

impl<I, V> Reader<I, V> {
    /// Create a new [`Reader`] over some `io`.
    ///
    /// The `nominal_length` must be preserved (e.g in a header) from the call to
    /// [`Writer::nominal_length()`].
    pub fn new(nominal_length: usize, io: I) -> Self {
        Self {
            nominal_length: NonZeroUsize::new(nominal_length),
            io,
            hasher: (),
            longest_distance: None,
            _phantom: PhantomData,
        }
    }
    /// Intern a `hasher` to allow using [`Reader::get`] to retrieve values.
    ///
    /// This must be the same hasher used by the [`Writer`].
    pub fn with_hasher<K, H>(self, hasher: H) -> Reader<I, V, K, H> {
        let Self {
            nominal_length,
            io,
            hasher: (),
            longest_distance,
            _phantom: _,
        } = self;
        Reader {
            nominal_length,
            io,
            hasher,
            longest_distance,
            _phantom: PhantomData,
        }
    }
}

impl<I, V, K, H> Reader<I, V, K, H> {
    /// Get a reference to the inner io
    pub fn get_io_ref(&self) -> &I {
        &self.io
    }
    // a bit of a hack...
    pub fn map_io<T>(self, f: impl FnOnce(I) -> T) -> Reader<T, V, K, H> {
        let Self {
            nominal_length,
            io,
            hasher,
            longest_distance,
            _phantom,
        } = self;
        Reader {
            nominal_length,
            io: f(io),
            hasher,
            longest_distance,
            _phantom,
        }
    }
    pub fn nominal_length(&self) -> usize {
        self.nominal_length
            .map(NonZeroUsize::get)
            .unwrap_or_default()
    }
    /// Add metadata that allows the reader to shrink lookup times.
    /// This is an optimisation.
    ///
    /// This must be preserved (e.g in a header) from the call to
    /// [`Writer::longest_distance()`].
    pub fn with_longest_distance(mut self, distance: usize) -> Self {
        self.longest_distance = Some(distance);
        self
    }
    /// Iterate all values in this map.
    pub fn iter_values(&self) -> impl Iterator<Item = io::Result<V>> + '_
    where
        I: ReadAt + Size,
        V: AsBytes + FromBytes + Copy,
    {
        self.iter_slots().map_ok(|slot| slot.value)
    }
    pub fn len(&self) -> io::Result<usize>
    where
        I: Size,
    {
        len::<Slot<V>>(&self.io)
    }
    pub fn iter_slots(&self) -> impl Iterator<Item = io::Result<Slot<V>>> + '_
    where
        I: ReadAt + Size,
        V: AsBytes + FromBytes + Copy,
    {
        // this `iter::once(..).map_ok(..).flatten_ok().map(..)` pattern allows us
        // to return a simple iterator, rather than an `io::Result<impl Iterator<..>>`
        iter::once(self.len())
            .map_ok(|len| {
                (0..len)
                    .map(|ix| match get_at::<Slot<V>>(&self.io, ix) {
                        Ok(Some(it)) => Ok(it),
                        Ok(None) => Err(io::Error::other(
                            "underlying io was truncated during iteration",
                        )),
                        Err(e) => Err(e),
                    })
                    .filter_map(|res| match res {
                        Ok(slot) => slot.as_nonempty().transpose(),
                        Err(e) => Some(Err(e)),
                    })
            })
            .flatten_ok()
            .map(|nested| nested.and_then(|flatten| flatten))
    }
    /// Retrieve a group of values by hash.
    pub fn get_by_hash(&self, hash: u64) -> impl Iterator<Item = io::Result<V>> + '_
    where
        I: ReadAt + Size,
        V: AsBytes + FromBytes + Copy,
    {
        let needle = hash.saturating_sub(1);
        let Some(nominal_length) = self.nominal_length else {
            return Either::Left(iter::empty());
        };
        let ideal_slot_ix = ideal_slot_ix(needle, nominal_length);
        let check_ixs = match self.longest_distance {
            Some(distance) => Ok(ideal_slot_ix..=ideal_slot_ix + distance),
            None => len::<Slot<V>>(&self.io).map(|len| ideal_slot_ix..=len),
        };
        Either::Right(
            iter::once(check_ixs)
                .map_ok(move |check_ixs| {
                    SearchBucket::new(
                        check_ixs.filter_map(|ix| get_at(&self.io, ix).transpose()),
                        needle,
                    )
                })
                .flatten_ok()
                .map(|nested| nested.and_then(|flatten| flatten)),
        )
    }
    /// Retrieve a superset of values for this key.
    ///
    /// If there is a key for this value, it is guaranteed to be in the items that
    /// are returned.
    pub fn get(&self, key: K) -> impl Iterator<Item = io::Result<V>> + '_
    where
        I: ReadAt + Size,
        V: Copy + AsBytes + FromBytes,
        H: Fn(K) -> u64, // not FnMut because `fn get(&mut self, ..)` feels wrong
    {
        self.get_by_hash((self.hasher)(key))
    }

    pub fn validate(&self) -> io::Result<()>
    where
        I: Size + ReadAt,
        V: AsBytes + FromBytes + Copy,
    {
        macro_rules! ensure {
            ($cond:expr, $msg:expr) => {
                if !$cond {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, $msg));
                }
            };
        }

        let len = len::<Slot<V>>(&self.io)?;
        ensure!(
            len >= self.nominal_length(),
            "io does not meet length requirements"
        );
        let mut prev_slot = None;
        for (position, ix) in (0..len).with_position() {
            let slot = get_at::<Slot<V>>(&self.io, ix)?
                .ok_or(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "io was truncated during validation",
                ))?
                .as_nonempty()?;
            match position {
                Position::Last | Position::Only => {
                    ensure!(slot.is_none(), "final slot must be empty")
                }
                Position::First => prev_slot = slot,
                Position::Middle => match (&mut prev_slot, slot) {
                    (_, None) => {} // if the rhs is none, then nothing to check
                    (None, Some(right)) => prev_slot = Some(right), // first
                    (Some(left), Some(right)) => {
                        ensure!(
                            left.hash.get() <= right.hash.get(),
                            format!("slot at index {} is out of order", ix)
                        );
                        *left = right;
                    }
                },
            }
        }
        Ok(())
    }
}

/// Assuming that `I` is a sorted iterator of [`Slot<T>`]s, this iterator scans
/// `I`, returning slots with the same hash as `needle`, stopping at any errors
/// or when it has passed the needle.
struct SearchBucket<I> {
    inner: Option<I>, // fuse
    needle: u64,
}

impl<I: Iterator> SearchBucket<I> {
    pub fn new(inner: I, needle: u64) -> Self {
        Self {
            inner: Some(inner),
            needle,
        }
    }
}

impl<I, T> Iterator for SearchBucket<I>
where
    I: Iterator<Item = io::Result<Slot<T>>>,
    T: AsBytes + FromZeroes + Copy,
{
    type Item = io::Result<T>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.inner.as_mut().and_then(Iterator::next) {
                Some(Ok(slot)) => match slot.as_nonempty() {
                    Ok(Some(it)) => match it.hash.get().cmp(&self.needle) {
                        Ordering::Less => continue,                    // skip forwards
                        Ordering::Equal => break Some(Ok(slot.value)), // found a match
                        Ordering::Greater => {
                            self.inner = None;
                            break None; // end of bucket
                        }
                    },
                    Ok(None) => {
                        self.inner = None;
                        break None; // end of bucket
                    }
                    Err(e) => {
                        self.inner = None;
                        break Some(Err(e));
                    }
                },
                Some(Err(e)) => {
                    self.inner = None; // fuse at the first error
                    break Some(Err(e));
                }
                None => {
                    self.inner = None; // fuse at EOF
                    break None;
                }
            }
        }
    }
}

/////////////////////////////
// Hash table fundamentals //
/////////////////////////////

/// Desired slot for a hash with a given table length
///
/// See: <https://lemire.me/blog/2016/06/27/a-fast-alternative-to-the-modulo-reduction/>
fn ideal_slot_ix(hash: u64, nominal_length: NonZeroUsize) -> usize {
    // One could simply write `self.0 as usize % buckets` but that involves
    // a relatively slow division.
    // Splitting the hash into chunks and mapping them linearly to buckets is much faster.
    // On modern computers, this mapping can be done with a single multiplication
    // (the right shift is optimized away).

    // break 0..=u64::MAX into 'buckets' chunks and map each chunk to 0..len.
    // if buckets=2, 0..(u64::MAX/2) maps to 0, and (u64::MAX/2)..=u64::MAX maps to 1.
    usize::try_from((hash as u128 * nominal_length.get() as u128) >> 64).unwrap()
}

fn load_factor2nominal_length(occupied_len: usize, load_factor: f64) -> Option<NonZeroUsize> {
    NonZeroUsize::new(cmp::max(
        (occupied_len as f64 / load_factor) as usize,
        occupied_len,
    ))
}

///////////////////////////////////
// Treat an impl ReadAt as a [T] //
///////////////////////////////////

fn index2pos<T>(index: usize) -> u64 {
    index
        .checked_mul(mem::size_of::<T>())
        .and_then(|it| u64::try_from(it).ok())
        .expect("couldn't calculate offset into backing io")
}

/// Threating io as a `[T]`, get the item at `index`.
///
/// Returns [`Ok(None)`] if the index is past the end of the io.
fn get_at<T>(reader: impl ReadAt, index: usize) -> io::Result<Option<T>>
where
    T: AsBytes + FromBytes,
{
    let mut buf = T::new_zeroed();
    match reader.read_exact_at(index2pos::<T>(index), buf.as_bytes_mut()) {
        Ok(()) => Ok(Some(buf)),
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
        Err(e) => Err(e),
    }
}

/// Treating io as a `[T]`, how many items would it contain?
pub fn len<T>(reader: impl Size) -> io::Result<usize> {
    let Some(len) = reader.size()? else {
        return Err(io::Error::other("backing io has no length"));
    };
    let Ok(len) = usize::try_from(len) else {
        return Err(io::Error::other(
            "backing io is too long to represent as &[u8]",
        ));
    };
    if len % mem::size_of::<T>() != 0 {
        return Err(io::Error::other(
            "backing io cannot be evenly divided into array items",
        ));
    }
    Ok(len / mem::size_of::<T>())
}

#[cfg(test)]
mod tests {
    use ahash::{HashMap, HashSet};
    use hashers::null::{NullHasher, PassThroughHasher};
    use siphasher::sip::SipHasher24;
    use std::{
        fmt::Debug,
        hash::{BuildHasher, BuildHasherDefault, Hash, Hasher},
    };

    use quickcheck::{quickcheck, Arbitrary};

    use super::*;

    fn do_hashmap<K, V>(
        reference: HashMap<K, V>,
        hasher: impl Fn(K) -> u64,
        load_factor: f64,
        use_longest_distance: bool,
    ) where
        K: Clone,
        V: AsBytes + FromBytes + Copy + Debug + Hash + Eq,
    {
        let reader = hashmap2imperfect(
            reference.clone(),
            &hasher,
            load_factor,
            use_longest_distance,
        );
        check_equal(reference.clone(), reader);
    }

    fn check_equal<K, V>(
        reference: HashMap<K, V>,
        reader: Reader<impl ReadAt + Size, V, K, impl Fn(K) -> u64>,
    ) where
        K: Clone,
        V: AsBytes + FromBytes + Copy + Debug + Hash + Eq,
    {
        for (key, expected) in reference.clone() {
            let candidates = reader.get(key).collect::<Result<Vec<_>, _>>().unwrap();
            assert!(candidates.contains(&expected))
        }

        let reference = reference.into_values().collect::<HashSet<_>>();
        let subject = reader
            .iter_values()
            .collect::<Result<HashSet<_>, _>>()
            .unwrap();
        // check subset because hashmap cannot contain duplicate keys, but reader can
        assert!(reference.is_subset(&subject));
    }

    fn hashmap2imperfect<K, V, F>(
        reference: HashMap<K, V>,
        hasher: F,
        load_factor: f64,
        use_longest_distance: bool,
    ) -> Reader<Vec<u8>, V, K, F>
    where
        F: Fn(K) -> u64,
        V: AsBytes + FromBytes + Copy + Eq,
    {
        let mut disk = vec![];
        let writer = Builder::new(&hasher)
            .with_values(reference)
            .into_writer(load_factor);
        writer.write_into(&mut disk).unwrap();
        assert_eq!(writer.written_len(), disk.len());
        let reader = Reader::new(writer.nominal_length(), disk).with_hasher(hasher);
        reader.validate().unwrap();
        match use_longest_distance {
            true => reader.with_longest_distance(writer.longest_distance()),
            false => reader,
        }
    }

    fn merge_readers<I, K, V, F>(
        left: Reader<I, V, K, F>,
        right: Reader<I, V, K, F>,
        use_longest_distance: bool,
    ) -> Reader<Vec<u8>, V>
    where
        V: AsBytes + FromBytes + Copy,
        I: ReadAt + Size,
    {
        let mut disk = vec![];
        let nominal_length = left.nominal_length() + right.nominal_length();
        let mut longest_distance = 0;
        for slot in merge(
            nominal_length,
            [left.iter_slots(), right.iter_slots()],
            &mut longest_distance,
        ) {
            disk.write_all(slot.unwrap().as_bytes()).unwrap();
        }
        let reader = Reader::new(nominal_length, disk);
        reader.validate().unwrap();
        match use_longest_distance {
            true => reader.with_longest_distance(longest_distance),
            false => reader,
        }
    }

    fn do_hashmap_with_hasher<K, V>(
        reference: HashMap<K, V>,
        hasher: impl BuildHasher,
        load_factor: f64,
        use_longest_distance: bool,
    ) where
        K: Clone + Hash,
        V: AsBytes + FromBytes + Copy + Debug + Hash + Eq,
    {
        do_hashmap(
            reference,
            |v| hasher.hash_one(v),
            load_factor,
            use_longest_distance,
        )
    }

    fn do_merge<K, V>(
        left: HashMap<K, V>,
        right: HashMap<K, V>,
        hasher: impl Fn(K) -> u64,
        left_load_factor: f64,
        right_load_factor: f64,
        use_longest_distance: bool,
    ) where
        K: Clone + Hash + Eq,
        V: AsBytes + FromBytes + Copy + Debug + Hash + Eq,
    {
        let merged = merge_readers(
            hashmap2imperfect(left.clone(), &hasher, left_load_factor, false),
            hashmap2imperfect(right.clone(), &hasher, right_load_factor, false),
            use_longest_distance,
        )
        .with_hasher(&hasher);
        let mut reference = left.clone();
        reference.extend(right.clone());
        check_equal(reference, merged);
    }

    fn do_merge_with_hasher<K, V>(
        left: HashMap<K, V>,
        right: HashMap<K, V>,
        hasher: impl BuildHasher,
        left_load_factor: f64,
        right_load_factor: f64,
        use_longest_distance: bool,
    ) where
        K: Clone + Hash + Eq,
        V: AsBytes + FromBytes + Copy + Debug + Hash + Eq,
    {
        do_merge(
            left,
            right,
            |v| hasher.hash_one(v),
            left_load_factor,
            right_load_factor,
            use_longest_distance,
        )
    }

    quickcheck! {
        fn hashmap(reference: HashMap<u32,u32>, load_factor: LoadFactor, use_longest_distance: bool) -> () {
            do_hashmap_with_hasher(reference.clone(), BuildHasherDefault::<SipHasher24>::default(), load_factor.get(), use_longest_distance);
            do_hashmap_with_hasher(reference.clone(), BuildHasherDefault::<NullHasher>::default(), load_factor.get(), use_longest_distance);
            do_hashmap_with_hasher(reference.clone(), BuildPassThroughBits::new(8), load_factor.get(), use_longest_distance);
        }
        fn merge_hashmap(left: HashMap<u32,u32>, right: HashMap<u32,u32>, left_load_factor: LoadFactor, right_load_factor: LoadFactor, use_longest_distance: bool) -> () {
            do_merge_with_hasher(left.clone(), right.clone(), BuildHasherDefault::<SipHasher24>::default(), left_load_factor.get(), right_load_factor.get(), use_longest_distance);
            do_merge_with_hasher(left.clone(), right.clone(), BuildHasherDefault::<NullHasher>::default(), left_load_factor.get(), right_load_factor.get(), use_longest_distance);
            do_merge_with_hasher(left.clone(), right.clone(), BuildPassThroughBits::new(8), left_load_factor.get(), right_load_factor.get(), use_longest_distance);
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct LoadFactor(f64);

    impl LoadFactor {
        pub fn get(self) -> f64 {
            self.0
        }
    }

    impl Arbitrary for LoadFactor {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let ratio = u16::arbitrary(g) as f64 / u16::MAX as f64;
            Self(f64::clamp(ratio, 0.1, f64::INFINITY)) // don't let it get too small...
        }
        fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
            Box::new(f64::shrink(&self.0).map(Self))
        }
    }

    struct BuildPassThroughBits {
        num_bits: u32,
    }

    impl BuildPassThroughBits {
        pub fn new(num_bits: u32) -> Self {
            Self { num_bits }
        }
    }

    impl BuildHasher for BuildPassThroughBits {
        type Hasher = PassThroughBits;

        fn build_hasher(&self) -> Self::Hasher {
            PassThroughBits::new(self.num_bits)
        }
    }

    struct PassThroughBits {
        inner: PassThroughHasher,
        num_bits: u32,
    }

    impl PassThroughBits {
        pub fn new(num_bits: u32) -> Self {
            assert!((0..64).contains(&num_bits));
            Self {
                inner: PassThroughHasher::default(),
                num_bits,
            }
        }
    }

    impl Hasher for PassThroughBits {
        fn finish(&self) -> u64 {
            let mask = (1 << self.num_bits) - 1;
            self.inner.finish() & mask
        }

        fn write(&mut self, bytes: &[u8]) {
            self.inner.write(bytes)
        }
    }

    #[test]
    fn passthrough_bits() {
        assert_eq!(
            BuildPassThroughBits::new(8).hash_one(u64::MAX),
            0x00_00_00_00_00_00_00_FF
        );
    }
}

mod util {
    use std::{cmp::Ordering, fmt, iter::Peekable};

    /// An [`Iterator`] which wraps several child iterators.
    ///
    /// As long as each child is sorted, their items will be yielded in sorted order.
    ///
    /// Ordered from least to greatest.
    pub struct FlattenSorted<I, F>
    where
        I: Iterator, // in definition of `Peekable`...
    {
        inner: Vec<Peekable<I>>,
        cmp: F,
    }

    impl<I, F, T> fmt::Debug for FlattenSorted<I, F>
    where
        I: Iterator<Item = T>,
        T: fmt::Debug, // #[derive(Debug)] doesn't add this bound
        I: fmt::Debug,
        F: fmt::Debug,
    {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("FlattenSorted")
                .field("inner", &self.inner)
                .field("cmp", &self.cmp)
                .finish()
        }
    }

    impl<I: Iterator, T> FlattenSorted<I, fn(&T, &T) -> Ordering> {
        #[allow(unused)]
        pub fn new<II, A: IntoIterator<Item = II>>(iters: A) -> Self
        where
            II: IntoIterator<IntoIter = I>,
            I: Iterator<Item = T>,
            T: Ord,
        {
            assert_iter(Self::from_iter(iters))
        }
    }

    impl<I: Iterator, F> FlattenSorted<I, F> {
        pub fn new_by<T, II, A: IntoIterator<Item = II>>(iters: A, cmp: F) -> Self
        where
            II: IntoIterator<IntoIter = I>,
            I: Iterator<Item = T>,
            F: FnMut(&T, &T) -> Ordering,
        {
            let mut this = FlattenSorted { inner: vec![], cmp };
            this.extend(iters);
            assert_iter(this)
        }
    }

    impl<T, I> Default for FlattenSorted<I, fn(&T, &T) -> Ordering>
    where
        I: Iterator<Item = T>,
        T: Ord,
    {
        fn default() -> Self {
            Self {
                inner: Vec::new(),
                cmp: Ord::cmp,
            }
        }
    }

    impl<I, II, F> Extend<II> for FlattenSorted<I, F>
    where
        II: IntoIterator<IntoIter = I>,
        I: Iterator,
    {
        fn extend<T: IntoIterator<Item = II>>(&mut self, iter: T) {
            self.inner
                .extend(iter.into_iter().map(|it| it.into_iter().peekable()))
        }
    }

    impl<I, II, T> FromIterator<II> for FlattenSorted<I, fn(&T, &T) -> Ordering>
    where
        II: IntoIterator<IntoIter = I>,
        I: Iterator<Item = T>,
        T: Ord,
    {
        fn from_iter<A: IntoIterator<Item = II>>(iter: A) -> Self {
            let mut this = Self::default();
            this.extend(iter);
            this
        }
    }

    impl<T, I, F> Iterator for FlattenSorted<I, F>
    where
        I: Iterator<Item = T>,
        F: FnMut(&T, &T) -> Ordering,
    {
        type Item = T;

        fn next(&mut self) -> Option<Self::Item> {
            let (best, _) =
                self.inner
                    .iter_mut()
                    .enumerate()
                    .reduce(
                        |(lix, left), (rix, right)| match (left.peek(), right.peek()) {
                            (None, None) => (lix, left),     // nothing to compare
                            (None, Some(_)) => (rix, right), // non-empty wins
                            (Some(_), None) => (lix, left),  // non-empty wins
                            (Some(lit), Some(rit)) => match (self.cmp)(lit, rit) {
                                Ordering::Less | Ordering::Equal => (lix, left), // lowest first
                                Ordering::Greater => (rix, right),
                            },
                        },
                    )?;
            self.inner[best].next()
        }
    }

    fn assert_iter<T: Iterator>(t: T) -> T {
        t
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use itertools::Itertools as _;
        use std::collections::BTreeSet;

        quickcheck::quickcheck! {
            fn test(iters: Vec<BTreeSet<usize>>) -> () {
                do_test(iters)
            }
        }

        fn do_test(iters: Vec<BTreeSet<usize>>) {
            let fwd = FlattenSorted::new(iters.clone()).collect::<Vec<_>>();
            for (left, right) in fwd.iter().tuple_windows() {
                assert!(left <= right)
            }

            let rev =
                FlattenSorted::new_by(iters.into_iter().map(|it| it.into_iter().rev()), |l, r| {
                    l.cmp(r).reverse()
                })
                .collect::<Vec<_>>();
            for (left, right) in rev.iter().tuple_windows() {
                assert!(left >= right)
            }
        }
    }
}
