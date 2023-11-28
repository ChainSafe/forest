use std::{cmp::Ordering, fmt, iter::Peekable};

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
