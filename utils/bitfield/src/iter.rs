// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{iter, ops::Range};

/// A trait for iterators over `Range<usize>`.
///
/// Requirements:
/// - all ranges are non-empty
/// - the ranges are in ascending order
/// - no two ranges overlap or touch
pub trait RangeIterator: Iterator<Item = Range<usize>> + Sized {
    /// Returns a new `RangeIterator` over the bits that are in `self`, in `other`, or in both.
    fn merge<R: RangeIterator>(self, other: R) -> Union<Self, R> {
        Union {
            a: Peekable::new(self),
            b: Peekable::new(other),
        }
    }

    /// Returns a new `RangeIterator` over the bits that are in both `self` and `other`.
    fn intersection<R: RangeIterator>(self, other: R) -> Intersection<Self, R> {
        Intersection {
            a: Peekable::new(self),
            b: Peekable::new(other),
        }
    }

    /// Returns a new `RangeIterator` over the bits that are in `self` but not in `other`.
    fn difference<R: RangeIterator>(self, other: R) -> Difference<Self, R> {
        Difference {
            a: Peekable::new(self),
            b: Peekable::new(other),
        }
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

/// A wrapper over a `RangeIterator` that lets you "peek" at the next range.
///
/// Like `std::iter::Peekable`, but only for `RangeIterator`s, and with
/// the ability to get a mutable reference to the peeked range. Used
/// by the `Union`/`Intersection`/`Difference` range iterators.
struct Peekable<I> {
    iter: I,
    /// Stores the peeked range. `None` means that no range was peeked, and
    /// `Some(None)` means that `peek` was called but the iterator was empty.
    peeked: Option<Option<Range<usize>>>,
}

impl<I: RangeIterator> Peekable<I> {
    fn new(iter: I) -> Self {
        Self { iter, peeked: None }
    }

    fn peek(&mut self) -> Option<&mut Range<usize>> {
        // the borrow checker needs this to be stored in a separate variable
        let iter = &mut self.iter;
        self.peeked.get_or_insert_with(|| iter.next()).as_mut()
    }
}

impl<I: RangeIterator> Iterator for Peekable<I> {
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        self.peeked.take().unwrap_or_else(|| self.iter.next())
    }
}

impl<I: RangeIterator> RangeIterator for Peekable<I> {}

/// A `RangeIterator` over the bits that represent the union of two other `RangeIterator`s.
pub struct Union<A, B> {
    a: Peekable<A>,
    b: Peekable<B>,
}

impl<A: RangeIterator, B: RangeIterator> Iterator for Union<A, B> {
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (a, b) = match (self.a.peek(), self.b.peek()) {
                (Some(a), Some(b)) => (a, b),
                _ => return self.a.next().or_else(|| self.b.next()),
            };

            if a.start <= b.start {
                if a.end < b.start {
                    // a.start < a.end < b.start < b.end
                    //
                    // a: -xxx-----
                    // b: -----xxx-

                    return self.a.next();
                } else if a.end < b.end {
                    // a.start <= b.start <= a.end < b.end
                    //
                    // a: -xx--- or -xxxx--- or -xxx----
                    // b: -xxxx-    ---xxxx-    ----xxx-

                    // we resize `b` to be the union of `a` and `b`, but don't
                    // return it yet because it might overlap with another range
                    // in `a_iter`
                    b.start = a.start;
                    self.a.next();
                } else {
                    // a.start <= b.start < b.end <= a.end
                    //
                    // a: -xxx- or -xxxx- or -xxxx- or -xxxxxx-
                    // b: -xxx-    ---xx-    -xx---    ---xx---

                    self.b.next();
                }
            } else {
                // b.start < a.start
                //
                // the union operator is symmetric, so this does exactly
                // the same as above but with `a` and `b` swapped

                if b.end < a.start {
                    // b.start < b.end < a.start < a.end
                    //
                    // a: -----xxx-
                    // b: -xxx-----

                    return self.b.next();
                } else if b.end < a.end {
                    // b.start < a.start <= b.end < a.end
                    //
                    // a: ----xxx- or ---xxxx-
                    // b: -xxx----    -xxxx---

                    a.start = b.start;
                    self.b.next();
                } else {
                    // b.start < a.start < a.end <= b.end
                    //
                    // a: ---xx- or ---xx---
                    // b: -xxxx-    -xxxxxx-

                    self.a.next();
                }
            }
        }
    }
}

impl<A: RangeIterator, B: RangeIterator> RangeIterator for Union<A, B> {}

/// A `RangeIterator` over the bits that represent the intersection of two other `RangeIterator`s.
pub struct Intersection<A, B> {
    a: Peekable<A>,
    b: Peekable<B>,
}

impl<A: RangeIterator, B: RangeIterator> Iterator for Intersection<A, B> {
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (a, b) = match (self.a.peek(), self.b.peek()) {
                (Some(a), Some(b)) => (a, b),
                _ => return None,
            };

            if a.start <= b.start {
                if a.end <= b.start {
                    // a.start < a.end <= b.start < b.end
                    //
                    // a: -xxx---- or -xxx-----
                    // b: ----xxx-    -----xxx-

                    self.a.next();
                } else if a.end < b.end {
                    // a.start <= b.start < a.end < b.end
                    //
                    // a: -xx--- or -xxxx---
                    // b: -xxxx-    ---xxxx-

                    let intersection = b.start..a.end;
                    self.a.next();
                    return Some(intersection);
                } else {
                    // a.start <= b.start < b.end <= a.end
                    //
                    // a: -xxx- or -xxxx- or -xxxx- or -xxxxxx-
                    // b: -xxx-    ---xx-    -xx---    ---xx---

                    return self.b.next();
                }
            } else {
                // b.start < a.start
                //
                // the intersection operator is symmetric, so this does exactly
                // the same as above but with `a` and `b` swapped

                if b.end <= a.start {
                    // b.start < b.end <= a.start < a.end
                    //
                    // a: ----xxx- or -----xxx-
                    // b: -xxx----    -xxx-----

                    self.b.next();
                } else if b.end < a.end {
                    // b.start < a.start < b.end < a.end
                    //
                    // a: ---xxxx-
                    // b: -xxxx---

                    let intersection = a.start..b.end;
                    self.b.next();
                    return Some(intersection);
                } else {
                    // b.start < a.start < a.end <= b.end
                    //
                    // a: ---xx- or ---xx---
                    // b: -xxxx-    -xxxxxx-

                    return self.a.next();
                }
            }
        }
    }
}

impl<A: RangeIterator, B: RangeIterator> RangeIterator for Intersection<A, B> {}

/// A `RangeIterator` over the bits that represent the difference between two other `RangeIterator`s.
pub struct Difference<A, B> {
    a: Peekable<A>,
    b: Peekable<B>,
}

impl<A: RangeIterator, B: RangeIterator> Iterator for Difference<A, B> {
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (a, b) = match (self.a.peek(), self.b.peek()) {
                (Some(a), Some(b)) => (a, b),
                _ => return self.a.next(),
            };

            if a.start < b.start {
                if a.end <= b.start {
                    // a.start < a.end <= b.start < b.end
                    //
                    // a: -xxx---- or -xxx-----
                    // b: ----xxx-    -----xxx-

                    return self.a.next();
                } else if b.end < a.end {
                    // a.start < b.start < b.end < a.end
                    //
                    // a: -xxxxxx-
                    // b: ---xx---

                    let difference = a.start..b.start;
                    a.start = b.end;
                    self.b.next();
                    return Some(difference);
                } else {
                    // a.start < b.start < a.end <= b.end
                    //
                    // a: -xxxx- or -xxxx---
                    // b: ---xx-    ---xxxx-

                    let difference = a.start..b.start;
                    self.a.next();
                    return Some(difference);
                }
            } else {
                // b.start <= a.start

                if b.end <= a.start {
                    // b.start < b.end <= a.start < a.end
                    //
                    // a: ----xxx- or -----xxx-
                    // b: -xxx----    -xxx-----

                    self.b.next();
                } else if a.end <= b.end {
                    // b.start <= a.start < a.end <= b.end
                    //
                    // a: -xxx- or ---xx- or -xx--- or ---xx---
                    // b: -xxx-    -xxxx-    -xxxx-    -xxxxxx-

                    self.a.next();
                } else {
                    // b.start <= a.start < b.end < a.end
                    //
                    // a: -xxxx- or ---xxxx-
                    // b: -xx---    -xxxx---

                    a.start = b.end;
                    self.b.next();
                }
            }
        }
    }
}

impl<A: RangeIterator, B: RangeIterator> RangeIterator for Difference<A, B> {}

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
        }

        for &Case {
            lhs,
            rhs,
            union,
            intersection,
            difference,
        } in &[
            Case {
                lhs: &[2..5],
                rhs: &[],
                union: &[2..5],
                intersection: &[],
                difference: &[2..5],
            },
            Case {
                lhs: &[0..3, 10..13],
                rhs: &[5..8],
                union: &[0..3, 5..8, 10..13],
                intersection: &[],
                difference: &[0..3, 10..13],
            },
            Case {
                lhs: &[0..3, 8..11],
                rhs: &[2..5],
                union: &[0..5, 8..11],
                intersection: &[2..3],
                difference: &[0..2, 8..11],
            },
            Case {
                lhs: &[0..3, 4..7, 8..11],
                rhs: &[2..5, 6..9, 10..13],
                union: &[0..13],
                intersection: &[2..3, 4..5, 6..7, 8..9, 10..11],
                difference: &[0..2, 5..6, 9..10],
            },
            Case {
                lhs: &[0..6],
                rhs: &[1..3],
                union: &[0..6],
                intersection: &[1..3],
                difference: &[0..1, 3..6],
            },
            Case {
                lhs: &[0..6],
                rhs: &[1..3, 5..7, 9..11],
                union: &[0..7, 9..11],
                intersection: &[1..3, 5..6],
                difference: &[0..1, 3..5],
            },
            Case {
                lhs: &[3..6],
                rhs: &[0..2, 4..5, 8..10],
                union: &[0..2, 3..6, 8..10],
                intersection: &[4..5],
                difference: &[3..4, 5..6],
            },
            Case {
                lhs: &[3..6, 8..10],
                rhs: &[2..7, 8..11],
                union: &[2..7, 8..11],
                intersection: &[3..6, 8..10],
                difference: &[],
            },
            Case {
                lhs: &[3..6, 8..10],
                rhs: &[2..4],
                union: &[2..6, 8..10],
                intersection: &[3..4],
                difference: &[4..6, 8..10],
            },
        ] {
            assert_eq!(ranges(lhs).merge(ranges(rhs)).collect::<Vec<_>>(), union);
            assert_eq!(ranges(rhs).merge(ranges(lhs)).collect::<Vec<_>>(), union);

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
