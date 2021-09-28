// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod combine;

use combine::{Combine, Cut, Difference, Intersection, SymmetricDifference, Union};
use std::{iter, ops::Range};

/// A trait for iterators over `Range<usize>`.
///
/// Requirements:
/// - all ranges are non-empty
/// - the ranges are in ascending order
/// - no two ranges overlap or touch
pub trait RangeIterator: Iterator<Item = Range<usize>> + Sized {
    /// Returns a new `RangeIterator` over the bits that are in `self`, in `other`, or in both.
    fn union<R: RangeIterator>(self, other: R) -> Combine<Self, R, Union> {
        Combine::new(self, other)
    }

    /// Returns a new `RangeIterator` over the bits that are in both `self` and `other`.
    fn intersection<R: RangeIterator>(self, other: R) -> Combine<Self, R, Intersection> {
        Combine::new(self, other)
    }

    /// Returns a new `RangeIterator` over the bits that are in `self` but not in `other`.
    fn difference<R: RangeIterator>(self, other: R) -> Combine<Self, R, Difference> {
        Combine::new(self, other)
    }

    /// Returns a new `RangeIterator` over the bits that are in `self` or in `other`, but not in both.
    fn symmetric_difference<R: RangeIterator>(
        self,
        other: R,
    ) -> Combine<Self, R, SymmetricDifference> {
        Combine::new(self, other)
    }

    /// Returns a new `RangeIterator` over the bits in `self` that remain after "cutting" out the
    /// bits in `other`, and shifting remaining bits to the left if necessary. For example:
    ///
    /// ```ignore
    /// lhs:     xx-xxx--x
    /// rhs:     -xx-x----
    ///
    /// cut:     x  x x--x
    /// output:  xxx--x
    /// ```
    fn cut<R: RangeIterator>(self, other: R) -> Combine<Self, R, Cut> {
        Combine::new(self, other)
    }

    /// Returns a new `RangeIterator` over the bits in `self` after skipping the first `n` bits.
    fn skip_bits(self, n: usize) -> Skip<Self> {
        Skip {
            iter: self,
            skip: n,
        }
    }

    /// Returns a new `RangeIterator` over the first `n` bits in `self`.
    fn take_bits(self, n: usize) -> Take<Self> {
        Take {
            iter: self,
            take: n,
        }
    }
}

/// A `RangeIterator` that skips over `n` bits of antoher `RangeIterator`.
pub struct Skip<I> {
    iter: I,
    skip: usize,
}

impl<I: RangeIterator> Iterator for Skip<I> {
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let mut range = self.iter.next()?;

            if range.len() > self.skip {
                range.start += self.skip;
                self.skip = 0;
                return Some(range);
            } else {
                self.skip -= range.len();
            }
        }
    }
}

impl<I: RangeIterator> RangeIterator for Skip<I> {}

/// A `RangeIterator` that iterates over the first `n` bits of antoher `RangeIterator`.
pub struct Take<I> {
    iter: I,
    take: usize,
}

impl<I: RangeIterator> Iterator for Take<I> {
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.take == 0 {
            return None;
        }

        let mut range = self.iter.next()?;

        if range.len() > self.take {
            range.end = range.start + self.take;
        }

        self.take -= range.len();
        Some(range)
    }
}

impl<I: RangeIterator> RangeIterator for Take<I> {}

/// A `RangeIterator` that wraps a regular iterator over `Range<usize>` as a way to explicitly
/// indicate that this iterator satisfies the requirements of the `RangeIterator` trait.
pub struct Ranges<I>(I);

impl<I> Ranges<I>
where
    I: Iterator<Item = Range<usize>>,
{
    /// Creates a new `Ranges` instance.
    pub fn new<II>(iter: II) -> Self
    where
        II: IntoIterator<IntoIter = I, Item = Range<usize>>,
    {
        Self(iter.into_iter())
    }
}

impl<I> Iterator for Ranges<I>
where
    I: Iterator<Item = Range<usize>>,
{
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<I> RangeIterator for Ranges<I> where I: Iterator<Item = Range<usize>> {}

/// Returns a `RangeIterator` which ranges contain the values from the provided iterator.
/// The values need to be in ascending order — if not, the returned iterator may not satisfy
/// all `RangeIterator` requirements.
pub fn ranges_from_bits(bits: impl IntoIterator<Item = usize>) -> impl RangeIterator {
    let mut iter = bits.into_iter().peekable();

    Ranges::new(iter::from_fn(move || {
        let start = iter.next()?;
        let mut end = start + 1;
        while iter.peek() == Some(&end) {
            end += 1;
            iter.next();
        }
        Some(start..end)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges(slice: &[Range<usize>]) -> impl RangeIterator + '_ {
        Ranges::new(slice.iter().cloned())
    }

    #[test]
    fn test_combinators() {
        struct Case<'a> {
            lhs: &'a [Range<usize>],
            rhs: &'a [Range<usize>],
            union: &'a [Range<usize>],
            intersection: &'a [Range<usize>],
            difference: &'a [Range<usize>],
            symmetric_difference: &'a [Range<usize>],
            cut: &'a [Range<usize>],
        }

        for &Case {
            lhs,
            rhs,
            union,
            intersection,
            difference,
            symmetric_difference,
            cut,
        } in &[
            Case {
                // --xxx
                lhs: &[2..5],

                // -----
                rhs: &[],

                // --xxx
                union: &[2..5],

                // -----
                intersection: &[],

                // --xxx
                difference: &[2..5],

                // --xxx
                symmetric_difference: &[2..5],

                // --xxx
                cut: &[2..5],
            },
            Case {
                // xxx-------xxx
                lhs: &[0..3, 10..13],

                // -----xxx-----
                rhs: &[5..8],

                // xxx--xxx--xxx
                union: &[0..3, 5..8, 10..13],

                // -------------
                intersection: &[],

                // xxx-------xxx
                difference: &[0..3, 10..13],

                // xxx--xxx--xxx
                symmetric_difference: &[0..3, 5..8, 10..13],

                // xxx--   --xxx
                // xxx----xxx
                cut: &[0..3, 7..10],
            },
            Case {
                // xxx-----xxx
                lhs: &[0..3, 8..11],

                // --xxx------
                rhs: &[2..5],

                // xxxxx---xxx
                union: &[0..5, 8..11],

                // --x--------
                intersection: &[2..3],

                // xx------xxx
                difference: &[0..2, 8..11],

                // xx-xx---xxx
                symmetric_difference: &[0..2, 3..5, 8..11],

                // xx   ---xxx
                // xx---xxx
                cut: &[0..2, 5..8],
            },
            Case {
                // xxx-xxx-xxx--
                lhs: &[0..3, 4..7, 8..11],

                // --xxx-xxx-xxx
                rhs: &[2..5, 6..9, 10..13],

                // xxxxxxxxxxxxx
                union: &[0..13],

                // --x-x-x-x-x--
                intersection: &[2..3, 4..5, 6..7, 8..9, 10..11],

                // xx---x---x---
                difference: &[0..2, 5..6, 9..10],

                // xx-x-x-x-x-xx
                symmetric_difference: &[0..2, 3..4, 5..6, 7..8, 9..10, 11..13],

                // xx   x   x
                // xxxx
                cut: &[0..4],
            },
            Case {
                // xxxxxx
                lhs: &[0..6],

                // -xx---
                rhs: &[1..3],

                // xxxxxx
                union: &[0..6],

                // -xx---
                intersection: &[1..3],

                // x--xxx
                difference: &[0..1, 3..6],

                // x--xxx
                symmetric_difference: &[0..1, 3..6],

                // x  xxx
                // xxxx
                cut: &[0..4],
            },
            Case {
                // xxxxxx-----
                lhs: &[0..6],

                // -xx--xx--xx
                rhs: &[1..3, 5..7, 9..11],

                // xxxxxxx--xx
                union: &[0..7, 9..11],

                // -xx--x-----
                intersection: &[1..3, 5..6],

                // x--xx------
                difference: &[0..1, 3..5],

                // x--xx-x--xx
                symmetric_difference: &[0..1, 3..5, 6..7, 9..11],

                // x  xx  --
                // xxx--
                cut: &[0..3],
            },
            Case {
                // ---xxx----
                lhs: &[3..6],

                // xx--x---xx
                rhs: &[0..2, 4..5, 8..10],

                // xx-xxx--xx
                union: &[0..2, 3..6, 8..10],

                // ----x-----
                intersection: &[4..5],

                // ---x-x----
                difference: &[3..4, 5..6],

                // xx-x-x--xx
                symmetric_difference: &[0..2, 3..4, 5..6, 8..10],

                //   -x x--
                // -xx--
                cut: &[1..3],
            },
            Case {
                // ---xxx--xx-
                lhs: &[3..6, 8..10],

                // --xxxxx-xxx
                rhs: &[2..7, 8..11],

                // --xxxxx-xxx
                union: &[2..7, 8..11],

                // ---xxx--xx-
                intersection: &[3..6, 8..10],

                // -----------
                difference: &[],

                // --x---x---x
                symmetric_difference: &[2..3, 6..7, 10..11],

                // --     -
                // ---
                cut: &[],
            },
            Case {
                // ---xxx--xx
                lhs: &[3..6, 8..10],

                // --xx------
                rhs: &[2..4],

                // --xxxx--xx
                union: &[2..6, 8..10],

                // ---x------
                intersection: &[3..4],

                // ----xx--xx
                difference: &[4..6, 8..10],

                // --x-xx--xx
                symmetric_difference: &[2..3, 4..6, 8..10],

                // --  xx--xx
                // --xx--xx
                cut: &[2..4, 6..8],
            },
        ] {
            assert_eq!(ranges(lhs).union(ranges(rhs)).collect::<Vec<_>>(), union);
            assert_eq!(ranges(rhs).union(ranges(lhs)).collect::<Vec<_>>(), union);

            assert_eq!(
                ranges(lhs).intersection(ranges(rhs)).collect::<Vec<_>>(),
                intersection
            );
            assert_eq!(
                ranges(rhs).intersection(ranges(lhs)).collect::<Vec<_>>(),
                intersection
            );

            assert_eq!(
                ranges(lhs).difference(ranges(rhs)).collect::<Vec<_>>(),
                difference
            );

            assert_eq!(
                ranges(lhs)
                    .symmetric_difference(ranges(rhs))
                    .collect::<Vec<_>>(),
                symmetric_difference
            );

            assert_eq!(ranges(lhs).cut(ranges(rhs)).collect::<Vec<_>>(), cut);
        }
    }

    #[test]
    fn test_ranges_from_bits() {
        struct Case<'a> {
            input: &'a [usize],
            output: &'a [Range<usize>],
        }
        for &Case { input, output } in &[
            Case {
                input: &[],
                output: &[],
            },
            Case {
                input: &[10],
                output: &[10..11],
            },
            Case {
                input: &[2, 3, 4, 7, 9, 11, 12],
                output: &[2..5, 7..8, 9..10, 11..13],
            },
        ] {
            assert_eq!(
                ranges_from_bits(input.iter().copied()).collect::<Vec<_>>(),
                output
            );
        }
    }

    #[test]
    fn test_skip_take() {
        struct Case<'a> {
            input: &'a [Range<usize>],
            n: usize,
            skip: &'a [Range<usize>],
            take: &'a [Range<usize>],
        }

        for &Case {
            input,
            n,
            skip,
            take,
        } in &[
            Case {
                input: &[],
                n: 0,
                skip: &[],
                take: &[],
            },
            Case {
                input: &[],
                n: 3,
                skip: &[],
                take: &[],
            },
            Case {
                input: &[1..3, 4..6],
                n: 0,
                skip: &[1..3, 4..6],
                take: &[],
            },
            Case {
                input: &[1..3, 4..6],
                n: 1,
                skip: &[2..3, 4..6],
                take: &[1..2],
            },
            Case {
                input: &[1..3, 4..6],
                n: 2,
                skip: &[4..6],
                take: &[1..3],
            },
            Case {
                input: &[1..3, 4..6],
                n: 3,
                skip: &[5..6],
                take: &[1..3, 4..5],
            },
        ] {
            assert_eq!(ranges(input).skip_bits(n).collect::<Vec<_>>(), skip);
            assert_eq!(ranges(input).take_bits(n).collect::<Vec<_>>(), take);
        }
    }
}
