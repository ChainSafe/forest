// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! General-purpose principled filtering logic
//! - Exact matching.
//!   No support for `glob`s or `regex`es.
//! - Allow-by-default, with a default block-list.
//! - User can either:
//!   - Append to and/or override the default block-list.
//!   - Select directly from the corpus.
//! - Checks for invalid entries in any parameters.

#![allow(clippy::disallowed_types)]

use std::{
    collections::HashSet,
    fmt::Display,
    hash::{BuildHasher, Hash, RandomState},
    iter,
};

use anyhow::bail;
use itertools::Either;
use tracing::warn;

pub enum Apply<T, S = RandomState> {
    Supplement {
        /// Make additions to the default block-list.
        block: HashSet<T, S>,
        /// Opt-in to entries that may be blocked in the default block-list.
        allow: HashSet<T, S>,
    },
    /// Only select these entries in the corpus.
    Select(HashSet<T, S>),
}

/// See [module documentation](mod@self) for more.
pub fn apply<T, S>(
    corpus: &HashSet<T, S>,
    mut default_block: HashSet<T, S>,
    apply: Option<&Apply<T, S>>,
) -> anyhow::Result<HashSet<T, S>>
where
    T: Eq + Hash + Display + Clone,
    S: BuildHasher + Default,
{
    let checkme = match apply {
        None => Either::Left(Either::Left(iter::empty())),
        Some(Apply::Supplement { block, allow }) => {
            Either::Left(Either::Right(block.iter().chain(allow.iter())))
        }
        Some(Apply::Select(it)) => Either::Right(it.iter()),
    }
    .chain(&default_block)
    .cloned()
    .collect::<HashSet<_, S>>();
    if let Some(it) = checkme.difference(corpus).collect1::<Vec<_>>() {
        // warn rather than bail because corpus may be dynamic, but block-list
        // shouldn't be
        warn!(
            "the following items were not actually present in the corpus: {}",
            itertools::join(it, ", ")
        );
    }
    Ok(match apply {
        None => corpus.difference(&default_block).cloned().collect(),
        Some(Apply::Supplement { block, allow }) => {
            if let Some(it) = block.intersection(allow).collect1::<Vec<_>>() {
                bail!(
                    "the following items are block _and_ allowed in supplementary args: {}",
                    itertools::join(it, ", ")
                );
            }
            default_block.extend(block.iter().cloned());
            let subtractme = default_block
                .difference(allow)
                .cloned()
                .collect::<HashSet<_, S>>();
            corpus.difference(&subtractme).cloned().collect()
        }
        Some(Apply::Select(it)) => corpus.intersection(it).cloned().collect(),
    })
}
trait IteratorExt: Iterator {
    fn collect1<I>(self) -> Option<I>
    where
        I: FromIterator<Self::Item>,
        Self: Sized,
    {
        let mut iter = self.into_iter().peekable();
        match iter.peek().is_some() {
            true => Some(iter.collect()),
            false => None,
        }
    }
}
impl<T> IteratorExt for T where T: Iterator {}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use super::*;

    type HashSet<T> = super::HashSet<T>;

    fn unwrap_unique<T>(iter: impl IntoIterator<Item = T>) -> impl Iterator<Item = T>
    where
        T: Hash + Eq,
    {
        let mut counts = itertools::Itertools::counts(iter.into_iter());
        let mut found_dup = false;
        counts.retain(|_, v| match *v == 1 {
            true => true,
            false => {
                found_dup = true;
                false
            }
        });
        match found_dup {
            true => panic!("iterator contained duplicate elements"),
            false => counts.into_keys(),
        }
    }

    #[test]
    fn no_apply() {
        do_test(["hello", "world"], [], None, ["hello", "world"]);
        do_test(["hello", "world"], ["hello"], None, ["world"]);
    }
    #[test]
    fn with_supplement() {
        do_test([1, 2, 3], [1], &supplement([], []), [2, 3]);
        do_test([1, 2, 3], [1], &supplement([2], []), [3]);
        do_test([1, 2, 3], [1], &supplement([], [1]), [1, 2, 3]);
        do_test([1, 2, 3], [1], &supplement([2], [1]), [1, 3]);

        do_test([1, 2, 3], [], &supplement([], []), [1, 2, 3]);
        do_test([1, 2, 3], [], &supplement([2], []), [1, 3]);
        do_test([1, 2, 3], [], &supplement([], [1]), [1, 2, 3]);
        do_test([1, 2, 3], [], &supplement([2], [1]), [1, 3]);
    }

    #[test]
    fn with_select() {
        do_test([1, 2, 3], [1], &select([]), []);
        do_test([1, 2, 3], [1], &select([1]), [1]);
        do_test([1, 2, 3], [1], &select([2]), [2]);

        do_test([1, 2, 3], [], &select([]), []);
        do_test([1, 2, 3], [], &select([1]), [1]);
        do_test([1, 2, 3], [], &select([2]), [2]);
    }

    fn supplement<T, S>(
        block: impl IntoIterator<Item = T>,
        allow: impl IntoIterator<Item = T>,
    ) -> Apply<T, S>
    where
        T: Hash + Eq,
        S: BuildHasher + Default,
    {
        Apply::Supplement {
            block: unwrap_unique(block.into_iter()).collect(),
            allow: unwrap_unique(allow.into_iter()).collect(),
        }
    }

    fn select<T, S>(it: impl IntoIterator<Item = T>) -> Apply<T, S>
    where
        T: Hash + Eq,
        S: BuildHasher + Default,
    {
        Apply::Select(unwrap_unique(it.into_iter()).collect())
    }

    #[track_caller]
    fn do_test<'a, T: 'a>(
        corpus: impl IntoIterator<Item = T>,
        default_block: impl IntoIterator<Item = T>,
        apply: impl Into<Option<&'a Apply<T>>>,
        expected: impl IntoIterator<Item = T>,
    ) where
        T: Eq + Hash + Display + Clone + Debug,
    {
        let actual = super::apply(
            &unwrap_unique(corpus.into_iter()).collect(),
            unwrap_unique(default_block.into_iter()).collect(),
            apply.into(),
        )
        .unwrap();
        let expected = unwrap_unique(expected.into_iter()).collect::<HashSet<_>>();
        assert_eq!(expected, actual);
    }
}
