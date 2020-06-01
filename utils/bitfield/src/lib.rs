// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod bitvec_serde;
pub mod rleplus;
pub use bitvec;

use bitvec::prelude::*;
use core::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not};
use fnv::FnvHashSet;
use std::iter::FromIterator;

type BitVec = bitvec::prelude::BitVec<Lsb0, u8>;
type Result<T> = std::result::Result<T, &'static str>;

/// Represents a bitfield to track bits set at indexes in the range of `u64`.
#[derive(Debug)]
pub enum BitField {
    Encoded {
        bv: BitVec,
        set: FnvHashSet<u64>,
        unset: FnvHashSet<u64>,
    },
    // TODO would be beneficial in future to only keep encoded bitvec in memory, but comes at a cost
    Decoded(BitVec),
}

impl Default for BitField {
    fn default() -> Self {
        Self::Decoded(BitVec::new())
    }
}

impl BitField {
    pub fn new() -> Self {
        Self::default()
    }

    /// Generates a new bitfield with a slice of all indexes to set.
    pub fn new_from_set(set_bits: &[u64]) -> Self {
        let mut vec = match set_bits.iter().max() {
            Some(&max) => bitvec![_, u8; 0; max as usize + 1],
            None => return Self::new(),
        };

        // Set all bits in bitfield
        for b in set_bits {
            vec.set(*b as usize, true);
        }

        Self::Decoded(vec)
    }

    /// Sets bit at bit index provided
    pub fn set(&mut self, bit: u64) {
        match self {
            BitField::Encoded { set, unset, .. } => {
                unset.remove(&bit);
                set.insert(bit);
            }
            BitField::Decoded(bv) => {
                let index = bit as usize;
                if bv.len() <= index {
                    bv.resize(index + 1, false);
                }
                bv.set(index, true);
            }
        }
    }

    /// Removes bit at bit index provided
    pub fn unset(&mut self, bit: u64) {
        match self {
            BitField::Encoded { set, unset, .. } => {
                set.remove(&bit);
                unset.insert(bit);
            }
            BitField::Decoded(bv) => {
                let index = bit as usize;
                if bv.len() <= index {
                    return;
                }
                bv.set(index, false);
            }
        }
    }

    /// Gets the bit at the given index.
    // TODO this probably should not require mut self and RLE decode bits
    pub fn get(&mut self, index: u64) -> Result<bool> {
        match self {
            BitField::Encoded { set, unset, .. } => {
                if set.contains(&index) {
                    return Ok(true);
                }

                if unset.contains(&index) {
                    return Ok(false);
                }

                // Check in encoded for the given bit
                // This can be changed to not flush changes
                if let Some(true) = self.as_mut_flushed()?.get(index as usize) {
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            BitField::Decoded(bv) => {
                if let Some(true) = bv.get(index as usize) {
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
        }
    }

    /// Retrieves the index of the first set bit, and error if invalid encoding or no bits set.
    pub fn first(&mut self) -> Result<u64> {
        for (i, b) in (0..).zip(self.as_mut_flushed()?.iter()) {
            if b == &true {
                return Ok(i);
            }
        }
        // Return error if none found, not ideal but no reason not to match
        Err("Bitfield has no set bits")
    }

    fn retrieve_set_indices<B: FromIterator<u64>>(&mut self, max: usize) -> Result<B> {
        let flushed = self.as_mut_flushed()?;
        if flushed.count_ones() > max {
            return Err("Bits set exceeds max in retrieval");
        }

        Ok((0..)
            .zip(self.as_mut_flushed()?.iter())
            .filter_map(|(i, b)| if b == &true { Some(i) } else { None })
            .collect())
    }

    /// Returns a vector of indexes of all set bits
    pub fn all(&mut self, max: usize) -> Result<Vec<u64>> {
        self.retrieve_set_indices(max)
    }

    /// Returns a Hash set of indexes of all set bits
    pub fn all_set(&mut self, max: usize) -> Result<FnvHashSet<u64>> {
        self.retrieve_set_indices(max)
    }

    /// Returns true if there are no bits set, false if the bitfield is empty.
    pub fn is_empty(&mut self) -> Result<bool> {
        for b in self.as_mut_flushed()?.iter() {
            if b == &true {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Returns a slice of the bitfield with the start index of set bits
    /// and number of bits to include in slice.
    pub fn slice(&mut self, start: u64, count: u64) -> Result<BitField> {
        if count == 0 {
            return Ok(BitField::default());
        }

        // These conversions aren't ideal, but we aren't supporting 32 bit targets
        let mut start = start as usize;
        let mut count = count as usize;

        let bitvec = self.as_mut_flushed()?;
        let mut start_idx: usize = 0;
        let mut range: usize = 0;
        if start != 0 {
            for (i, v) in bitvec.iter().enumerate() {
                if v == &true {
                    start -= 1;
                    if start == 0 {
                        start_idx = i + 1;
                        break;
                    }
                }
            }
        }

        for (i, v) in bitvec[start_idx..].iter().enumerate() {
            if v == &true {
                count -= 1;
                if count == 0 {
                    range = i + 1;
                    break;
                }
            }
        }

        if count > 0 {
            return Err("Not enough bits to index the slice");
        }

        let mut slice = BitVec::with_capacity(start_idx + range);
        slice.resize(start_idx, false);
        slice.extend_from_slice(&bitvec[start_idx..start_idx + range]);
        Ok(BitField::Decoded(slice))
    }

    /// Retrieves number of set bits in the bitfield
    ///
    /// This function requires a mutable reference for now to be able to handle the cached
    /// changes in the case of an RLE encoded bitfield.
    pub fn count(&mut self) -> Result<usize> {
        Ok(self.as_mut_flushed()?.count_ones())
    }

    fn flush(&mut self) -> Result<()> {
        if let BitField::Encoded { bv, set, unset } = self {
            *self = BitField::Decoded(decode_and_apply_cache(bv, set, unset)?);
        }

        Ok(())
    }

    fn into_flushed(mut self) -> Result<BitVec> {
        self.flush()?;
        match self {
            BitField::Decoded(bv) => Ok(bv),
            // Unreachable because flushed before this.
            _ => unreachable!(),
        }
    }

    fn as_mut_flushed(&mut self) -> Result<&mut BitVec> {
        self.flush()?;
        match self {
            BitField::Decoded(bv) => Ok(bv),
            // Unreachable because flushed before this.
            _ => unreachable!(),
        }
    }

    /// Merges to bitfields together (equivalent of bitwise OR `|` operator)
    pub fn merge(mut self, other: &Self) -> Result<Self> {
        self.merge_assign(other)?;
        Ok(self)
    }

    /// Merges to bitfields into `self` (equivalent of bitwise OR `|` operator)
    pub fn merge_assign(&mut self, other: &Self) -> Result<()> {
        let a = self.as_mut_flushed()?;
        match other {
            BitField::Encoded { bv, set, unset } => {
                let v = decode_and_apply_cache(bv, set, unset)?;
                bit_or(a, v.into_iter())
            }
            BitField::Decoded(bv) => bit_or(a, bv.iter().copied()),
        }

        Ok(())
    }

    /// Intersection of two bitfields (equivalent of bit AND `&`)
    pub fn intersect(mut self, other: &Self) -> Result<Self> {
        self.intersect_assign(other)?;
        Ok(self)
    }

    /// Intersection of two bitfields and assigns to self (equivalent of bit AND `&`)
    pub fn intersect_assign(&mut self, other: &Self) -> Result<()> {
        match other {
            BitField::Encoded { bv, set, unset } => {
                *self.as_mut_flushed()? &= decode_and_apply_cache(bv, set, unset)?
            }
            BitField::Decoded(bv) => *self.as_mut_flushed()? &= bv.iter().copied(),
        }
        Ok(())
    }

    /// Subtract other bitfield from self (equivalent of `a & !b`)
    pub fn subtract(mut self, other: &Self) -> Result<Self> {
        self.subtract_assign(other)?;
        Ok(self)
    }

    /// Subtract other bitfield from self (equivalent of `a & !b`)
    pub fn subtract_assign(&mut self, other: &Self) -> Result<()> {
        match other {
            BitField::Encoded { bv, set, unset } => {
                *self.as_mut_flushed()? &= !decode_and_apply_cache(bv, set, unset)?
            }
            BitField::Decoded(bv) => *self.as_mut_flushed()? &= bv.iter().copied().map(|b| !b),
        }
        Ok(())
    }

    /// Creates a bitfield which is a union of a vector of bitfields.
    pub fn union(bit_fields: &[Self]) -> Result<Self> {
        let mut ret = Self::default();
        for bf in bit_fields.iter() {
            ret.merge_assign(bf)?;
        }
        Ok(ret)
    }

    /// Returns true if BitFields have any overlapping bits.
    pub fn contains_any(&mut self, other: &mut BitField) -> Result<bool> {
        for (&a, &b) in self
            .as_mut_flushed()?
            .iter()
            .zip(other.as_mut_flushed()?.iter())
        {
            if a && b {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Returns true if the self `BitField` has all the bits set in the other `BitField`.
    pub fn contains_all(&mut self, other: &mut BitField) -> Result<bool> {
        let a_bf = self.as_mut_flushed()?;
        let b_bf = other.as_mut_flushed()?;

        // Checking lengths should be sufficient in most cases, but does not take into account
        // decoded bitfields with extra 0 bits. This makes sure there are no extra bits in the
        // extension.
        if b_bf.len() > a_bf.len() && b_bf[a_bf.len()..].count_ones() > 0 {
            return Ok(false);
        }

        for (a, b) in a_bf.iter().zip(b_bf.iter()) {
            if *b && !a {
                return Ok(false);
            }
        }

        Ok(true)
    }
}

fn bit_or<I>(a: &mut BitVec, mut b: I)
where
    I: Iterator<Item = bool>,
{
    for mut a_i in a.iter_mut() {
        match b.next() {
            Some(true) => *a_i = true,
            Some(false) => (),
            None => return,
        }
    }

    a.extend(b);
}

fn decode_and_apply_cache(
    bit_vec: &BitVec,
    set: &FnvHashSet<u64>,
    unset: &FnvHashSet<u64>,
) -> Result<BitVec> {
    let mut decoded = rleplus::decode(bit_vec)?;

    // Resize before setting any values
    if let Some(&max) = set.iter().max() {
        let max = max as usize;
        if max >= bit_vec.len() {
            decoded.resize(max + 1, false);
        }
    };

    // Set all values in the cache
    for &b in set.iter() {
        decoded.set(b as usize, true);
    }

    // Unset all values from the encoded cache
    for &b in unset.iter() {
        decoded.set(b as usize, false);
    }

    Ok(decoded)
}

impl AsRef<BitField> for BitField {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl From<BitVec> for BitField {
    fn from(b: BitVec) -> Self {
        Self::Decoded(b)
    }
}

impl<B> BitOr<B> for BitField
where
    B: AsRef<Self>,
{
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: B) -> Self {
        self.merge(rhs.as_ref()).unwrap()
    }
}

impl<B> BitOrAssign<B> for BitField
where
    B: AsRef<Self>,
{
    #[inline]
    fn bitor_assign(&mut self, rhs: B) {
        self.merge_assign(rhs.as_ref()).unwrap()
    }
}

impl<B> BitAnd<B> for BitField
where
    B: AsRef<Self>,
{
    type Output = Self;

    #[inline]
    fn bitand(self, rhs: B) -> Self::Output {
        self.intersect(rhs.as_ref()).unwrap()
    }
}

impl<B> BitAndAssign<B> for BitField
where
    B: AsRef<Self>,
{
    #[inline]
    fn bitand_assign(&mut self, rhs: B) {
        self.intersect_assign(rhs.as_ref()).unwrap()
    }
}

impl Not for BitField {
    type Output = Self;

    #[inline]
    fn not(self) -> Self::Output {
        Self::Decoded(!self.into_flushed().unwrap())
    }
}
