// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Combining two range iterators into a single new range iterator.
//!
//! This file contains the inner workings of the `BitField` combinators like
//! `merge` and `intersection`. The `Combinator` trait specifies how two range
//! iterators should be combined, and the `Combine` iterator lazily computes the
//! output ranges.
//!
//! The `Combine` iterator works at follows:
//! 1. it inspects the first range of each of the two input ranges
//! 2. it asks the corresponding combinator how these two ranges should be combined
//! 3. it discards the range with the lowest upper bound, and goes back to step 1
//!
//! For example, given the iterators over the following ranges:
//!
//! ```ignore
//! lhs: -xx-xx
//! rhs: xxxxx-
//! ```
//!
//! First `-xx---` and `xxxxx-` are passed to the combinator. Then `-xx---` is
//! discarded because it has the lowest upper bound, after which we are left with
//!
//! ```ignore
//! lhs: ----xx
//! rhs: xxxxx-
//! ```
//!
//! Now `----xx` and `xxxxx-` are passed to the combinator. Finally, `xxxxx-` is
//! discarded, and the only remaining range `----xx` is passed to the combinator as
//! well.
//!
//! It is up to the specific combinator to decide which ranges to produce. For
//! example, the `Intersection` combinator would produce the following outputs
//! given the inputs from above:
//!
//! ```ignore
//! xxx---
//! xxxxx-
//! ----xx
//! ```
//!
//! These ranges are combined into a proper range iterator by merging overlapping
//! ranges.

use super::RangeIterator;
use std::{cmp, iter, ops::Range};

/// A trait for defining how two range iterators can be combined into a single new range iterator.
///
/// When returning a range, it is required that the lower bound of that range isn't smaller than
/// any previously returned range. The logic for stitching overlapping ranges together relies on
/// the lower bounds of the returned ranges to form a monotonically increasing sequence.
pub trait Combinator: Default {
    /// Produces an output range for the two given input ranges.
    ///
    /// - It is guaranteed that `lhs.end <= rhs.end`.
    /// - The `rhs` range can be mutated if necessary.
    /// - Can return an empty range, those will be filtered out.
    fn advance_lhs(&mut self, lhs: Range<usize>, rhs: &mut Range<usize>) -> Range<usize>;

    /// Produces an output range for the two given input ranges.
    ///
    /// - It is guaranteed that `lhs.end > rhs.end`.
    /// - The `lhs` range can be mutated if necessary.
    /// - Can return an empty range, those will be filtered out.
    fn advance_rhs(&mut self, lhs: &mut Range<usize>, rhs: Range<usize>) -> Range<usize>;

    /// Produces an output range for the given input range. Called only when the
    /// second input range iterator is empty.
    fn advance_lhs_tail(&mut self, lhs: Range<usize>) -> Option<Range<usize>>;

    /// Produces an output range for the given input range. Called only when the
    /// first input range iterator is empty.
    fn advance_rhs_tail(&mut self, rhs: Range<usize>) -> Option<Range<usize>>;
}

/// The union combinator.
///
/// Produces ranges over the bits that are in one or both of the input range iterators.
#[derive(Default)]
pub struct Union;

impl Combinator for Union {
    fn advance_lhs(&mut self, lhs: Range<usize>, rhs: &mut Range<usize>) -> Range<usize> {
        // the returned range needs to start from the minimum lower bound of the two ranges,
        // to ensure that the lower bounds are monotonically increasing
        //
        // e.g. `--xx--`, `xxxxxx` should first produce
        // `xxxx--` and then `xxxxxx`, not
        // `--xx--` and then `xxxxxx`
        //
        // lhs:     xx----      xxxx--      --xx--
        // rhs:     ----xx  or  --xxxx  or  xxxxxx
        // output:  xx----      xxxx--      xxxx--

        cmp::min(lhs.start, rhs.start)..lhs.end
    }

    fn advance_rhs(&mut self, lhs: &mut Range<usize>, rhs: Range<usize>) -> Range<usize> {
        cmp::min(lhs.start, rhs.start)..rhs.end
    }

    fn advance_lhs_tail(&mut self, lhs: Range<usize>) -> Option<Range<usize>> {
        // the union of a range and an empty range is just that range
        Some(lhs)
    }

    fn advance_rhs_tail(&mut self, rhs: Range<usize>) -> Option<Range<usize>> {
        Some(rhs)
    }
}

/// The intersection combinator.
///
/// Produces ranges over the bits that are in both of the input range iterators.
#[derive(Default)]
pub struct Intersection;

impl Combinator for Intersection {
    fn advance_lhs(&mut self, lhs: Range<usize>, rhs: &mut Range<usize>) -> Range<usize> {
        // lhs:     xx----      xxxx--      --xx--
        // rhs:     ----xx  or  --xxxx  or  xxxxxx
        // output:  ------      --xx--      --xx--

        cmp::max(lhs.start, rhs.start)..lhs.end
    }

    fn advance_rhs(&mut self, lhs: &mut Range<usize>, rhs: Range<usize>) -> Range<usize> {
        cmp::max(lhs.start, rhs.start)..rhs.end
    }

    fn advance_lhs_tail(&mut self, _lhs: Range<usize>) -> Option<Range<usize>> {
        // the intersection of a range and an empty range is an empty range
        None
    }

    fn advance_rhs_tail(&mut self, _rhs: Range<usize>) -> Option<Range<usize>> {
        None
    }
}

/// The difference combinator.
///
/// Produces ranges over the bits that are in the `lhs` range iterator, but not in the `rhs`.
#[derive(Default)]
pub struct Difference;

impl Combinator for Difference {
    fn advance_lhs(&mut self, lhs: Range<usize>, rhs: &mut Range<usize>) -> Range<usize> {
        // lhs:     xx----      xxxx--      --xx--
        // rhs:     ----xx  or  --xxxx  or  xxxxxx
        // output:  xx----      xx----      ------

        lhs.start..cmp::min(lhs.end, rhs.start)
    }

    fn advance_rhs(&mut self, lhs: &mut Range<usize>, rhs: Range<usize>) -> Range<usize> {
        // since we're advancing the rhs, we need to potentially shorten the lhs
        // to avoid it from returning invalid bits in the next iteration
        //
        // e.g. `--xxxx`, `xxxx--` should first produce
        // `------` and then `----xx`, not
        // `------` and then `--xxxx`
        //
        // lhs:      ----xx      --xxxx      xxxxxx
        // rhs:      xx----  or  xxxx--  or  --xx--
        // output:   ------      ------      xx----
        // new lhs:  ----xx      ----xx      ----xx

        let difference = lhs.start..cmp::min(lhs.end, rhs.start);
        lhs.start = cmp::max(lhs.start, rhs.end);
        difference
    }

    fn advance_lhs_tail(&mut self, lhs: Range<usize>) -> Option<Range<usize>> {
        // the difference between a range and an empty range is just that range
        Some(lhs)
    }

    fn advance_rhs_tail(&mut self, _rhs: Range<usize>) -> Option<Range<usize>> {
        // the difference between an empty range and a range is an empty range
        None
    }
}

/// The symmetric difference combinator.
///
/// Produces ranges over the bits that are in one of the input range iterators, but not in both.
#[derive(Default)]
pub struct SymmetricDifference;

impl SymmetricDifference {
    /// Returns the symmetric difference of the two ranges where `left.end <= right.end`.
    /// Adjusts `rhs` to not return invalid bits in the next iteration.
    fn advance(left: Range<usize>, right: &mut Range<usize>) -> Range<usize> {
        if left.start <= right.start {
            // left:       xxxx--      xx----
            // right:      --xxxx  or  ----xx
            // output:     xx----      xx----
            // new right:  ----xx      ----xx

            let difference = left.start..cmp::min(left.end, right.start);
            right.start = cmp::max(right.start, left.end);
            difference
        } else {
            // left:       --xx--
            // right:      xxxxxx
            // output:     xx----
            // new right:  ----xx

            let difference = right.start..left.start;
            right.start = left.end;
            difference
        }
    }
}

impl Combinator for SymmetricDifference {
    fn advance_lhs(&mut self, lhs: Range<usize>, rhs: &mut Range<usize>) -> Range<usize> {
        Self::advance(lhs, rhs)
    }

    fn advance_rhs(&mut self, lhs: &mut Range<usize>, rhs: Range<usize>) -> Range<usize> {
        Self::advance(rhs, lhs)
    }

    fn advance_lhs_tail(&mut self, lhs: Range<usize>) -> Option<Range<usize>> {
        // the symmetric difference of a range and an empty range is just that range
        Some(lhs)
    }

    fn advance_rhs_tail(&mut self, rhs: Range<usize>) -> Option<Range<usize>> {
        Some(rhs)
    }
}

/// The cut combinator.
///
/// Produces ranges over the bits that remain after cutting the set bits of the `rhs`
/// out of the `lhs`, and shifting bits to the left to fill those gaps.
#[derive(Default)]
pub struct Cut {
    /// Stores the number of bits that have been cut out so far, i.e. the number of bits
    /// each output range needs to be shifted to the left by.
    offset: usize,
}

impl Cut {
    /// Offsets an output range by the current offset.
    fn offset(&self, range: Range<usize>) -> Range<usize> {
        (range.start - self.offset)..(range.end - self.offset)
    }
}

impl Combinator for Cut {
    fn advance_lhs(&mut self, lhs: Range<usize>, rhs: &mut Range<usize>) -> Range<usize> {
        // apart from the offset, these implementations are identical to those of the `Difference` combinator
        self.offset(lhs.start..cmp::min(lhs.end, rhs.start))
    }

    fn advance_rhs(&mut self, lhs: &mut Range<usize>, rhs: Range<usize>) -> Range<usize> {
        let cut = self.offset(lhs.start..cmp::min(lhs.end, rhs.start));
        lhs.start = cmp::max(lhs.start, rhs.end);
        self.offset += rhs.len();
        cut
    }

    fn advance_lhs_tail(&mut self, lhs: Range<usize>) -> Option<Range<usize>> {
        Some(self.offset(lhs))
    }

    fn advance_rhs_tail(&mut self, _rhs: Range<usize>) -> Option<Range<usize>> {
        None
    }
}

/// Combines two range iterators according to the given combinator, and merges
/// the output ranges together.
pub struct Combine<A, B, C>(Merge<_Combine<A, B, C>>)
where
    A: RangeIterator,
    B: RangeIterator,
    C: Combinator;

impl<A, B, C> Combine<A, B, C>
where
    A: RangeIterator,
    B: RangeIterator,
    C: Combinator,
{
    pub fn new(a: A, b: B) -> Self {
        Self(Merge::new(_Combine::new(a, b)))
    }
}

impl<A, B, C> Iterator for Combine<A, B, C>
where
    A: RangeIterator,
    B: RangeIterator,
    C: Combinator,
{
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<A, B, C> RangeIterator for Combine<A, B, C>
where
    A: RangeIterator,
    B: RangeIterator,
    C: Combinator,
{
}

/// Combines two range iterators according to the given combinator, but does not
/// merge the output ranges together. Since the ranges can overlap, this does not
/// satisfy the `RangeIterator` requirements.
struct _Combine<A, B, C>
where
    A: RangeIterator,
    B: RangeIterator,
{
    lhs: Lookahead<A>,
    rhs: Lookahead<B>,
    combinator: C,
}

impl<A, B, C> _Combine<A, B, C>
where
    A: RangeIterator,
    B: RangeIterator,
    C: Combinator,
{
    fn new(lhs: A, rhs: B) -> Self {
        Self {
            lhs: Lookahead::new(lhs),
            rhs: Lookahead::new(rhs),
            combinator: Default::default(),
        }
    }

    /// Computes the next range by inspecting the next range of each of the input
    /// range iterators and passing them to the combinator. Also advances the range
    /// iterator which corresponding range has the lowest upper bound.
    fn next_range(&mut self) -> Option<Range<usize>> {
        let (range, advance_lhs) = match (self.lhs.peek(), self.rhs.peek()) {
            (Some(lhs), Some(rhs)) => {
                // if both iterators are non-empty, we advance the one whichever's
                // corresponding range has a smaller upper bound
                if lhs.end <= rhs.end {
                    (Some(self.combinator.advance_lhs(lhs.clone(), rhs)), true)
                } else {
                    (Some(self.combinator.advance_rhs(lhs, rhs.clone())), false)
                }
            }
            (Some(lhs), None) => (self.combinator.advance_lhs_tail(lhs.clone()), true),
            (None, Some(rhs)) => (self.combinator.advance_rhs_tail(rhs.clone()), false),
            (None, None) => return None,
        };

        if advance_lhs {
            self.lhs.next();
        } else {
            self.rhs.next();
        }

        range
    }
}

impl<A, B, C> Iterator for _Combine<A, B, C>
where
    A: RangeIterator,
    B: RangeIterator,
    C: Combinator,
{
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        // we repeatedly compute the next range until we find one that is non-empty
        // TODO: use `!range.is_empty()` once it stabilizes in Rust 1.47
        iter::from_fn(|| self.next_range()).find(|range| range.start < range.end)
    }
}

/// A range iterator that wraps an iterator of ranges and merges the overlapping
/// (and touching) ranges together.
///
/// For example, given the ranges:
///
/// ```ignore
/// xx--------
/// xxx-------
/// ---xx-----
/// ---x------
/// -------xx-
/// --------xx
/// ```
///
/// `Merge` will produce
///
/// ```ignore
/// xxxxx--xxx
/// ```
///
/// Since this is done lazily, it's required that the ranges of the underlying
/// iterator increase monotonically (i.e. are non-decreasing) in their lower bound.
/// Also requires that the underlying ranges are non-empty.
struct Merge<I: Iterator> {
    iter: Lookahead<I>,
}

impl<I> Merge<I>
where
    I: Iterator<Item = Range<usize>>,
{
    pub fn new(iter: I) -> Self {
        Self {
            iter: Lookahead::new(iter),
        }
    }
}

impl<I> Iterator for Merge<I>
where
    I: Iterator<Item = Range<usize>>,
{
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut range = self.iter.next()?;

        // as long as the next range overlaps with (or touches) current range,
        // we merge it into the current range
        while let Some(next) = self.iter.peek() {
            if next.start > range.end {
                break;
            }

            range.end = cmp::max(range.end, next.end);
            self.iter.next();
        }

        Some(range)
    }
}

impl<I> RangeIterator for Merge<I> where I: Iterator<Item = Range<usize>> {}

/// An iterator wrapper that stores (and gives mutable access to) the next item of the iterator.
///
/// Similar to `std::iter::Peekable`, but unlike `Peekable`, `Lookahead` stores the next item
/// unconditionally (if there is any).
struct Lookahead<I: Iterator> {
    iter: I,
    next: Option<I::Item>,
}

impl<I: Iterator> Lookahead<I> {
    fn new(mut iter: I) -> Self {
        let next = iter.next();
        Self { iter, next }
    }

    fn peek(&mut self) -> Option<&mut I::Item> {
        self.next.as_mut()
    }
}

impl<I: Iterator> Iterator for Lookahead<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        // `self.next` always stores the next element, so if it is `None`, the iterator is empty
        let next = self.next.take()?;
        self.next = self.iter.next();
        Some(next)
    }
}
