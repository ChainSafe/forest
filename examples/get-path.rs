#![allow(clippy::disallowed_types)]
use self::PathChange::{Apply, Revert};
use itertools::{
    EitherOrBoth::{Both, Left, Right},
    Itertools as _, PeekingNext,
};
#[cfg(test)]
use pretty_assertions::assert_eq;
use std::{cmp, collections::HashMap, iter};
use thiserror::Error;

pub type NodeStore<'a> = HashMap<&'a str, Node<'a>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Node<'a> {
    pub id: &'a str,
    pub parent_id: Option<&'a str>,
    pub height: usize,
}

impl<'a> Node<'a> {
    pub fn new(id: &'a str, parent: impl Into<Option<&'a str>>, height: usize) -> Self {
        Self {
            id,
            parent_id: parent.into(),
            height,
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Debug, Default, Error)]
#[error("node not found in the blockstore")]
pub struct NodeNotFound;

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub enum PathChange<'a> {
    Revert(&'a str),
    Apply(&'a str),
}

fn lineage<'a>(
    store: &'a NodeStore,
    id: &str,
) -> impl Iterator<Item = Result<Node<'a>, NodeNotFound>> + 'a {
    let first = match store.get(id) {
        Some(it) => Ok(*it),
        None => Err(NodeNotFound),
    };
    iter::successors(Some(first), |prev| match prev {
        Ok(it) => match it.parent_id {
            Some(parent) => match store.get(parent) {
                Some(child) => Some(Ok(*child)),
                None => Some(Err(NodeNotFound)),
            },
            None => None, // end of lineage
        },
        Err(_) => None, // fuse on first error
    })
}

#[test]
fn test_lineage() {
    let mut store = NodeStore::new();
    chain! {
        in store;
        gen -> a -> b
    };
    assert_eq!(
        lineage(&store, "b")
            .map_ok(|it| it.id)
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        ["b", "a", "gen"]
    );
}

/// after calling this, `iter` will start from `height`.
///
/// Assumes epochs decrement by one in `iter`.
fn scroll<'a, E>(
    mut iter: impl PeekingNext<Item = Result<Node<'a>, E>>,
    height: usize,
) -> Result<Vec<&'a str>, E> {
    iter.peeking_take_while(|it| match it {
        Ok(it) => it.height > height,
        Err(_) => true, // bubble
    })
    .map_ok(|it| it.id)
    .collect()
}

#[test]
fn test_scroll() {
    let mut store = NodeStore::new();
    chain! {
        in store;
        gen -> a -> b
    };
    assert_eq!(
        scroll(lineage(&store, "b").peekable(), 0).unwrap(),
        ["b", "a"]
    );
    assert_eq!(
        scroll(lineage(&store, "b").peekable(), 2).unwrap(),
        Vec::<&str>::new()
    );
}

#[cfg(test)]
macro_rules! chain {
    (in $store:expr; $root:ident $(-> $next:ident)*) => {
        // in the (likely) case that `$store` is ExprIdent("store"),
        // we'll rebind the variable, which is NOT what we want, so use __store
        let __store: &mut NodeStore = &mut $store;
        let root_id = stringify!($root);
        __store.entry(root_id).or_insert(Node::new(root_id, None, 0));

        let mut parent_id = root_id;
        $(
            let node = Node::new(stringify!($next), Some(parent_id), __store[parent_id].height + 1);
            let clobbered = __store.insert(node.id, node);
            assert!(clobbered.is_none());
            parent_id = node.id;
        )*
        let _ = parent_id;
    };
}
#[cfg(test)]
pub(crate) use chain;

#[test]
fn test_chain() {
    let mut store = NodeStore::new();
    chain! {
        in store;
        gen -> a -> b
    };
    assert_eq!(store.len(), 3);
    assert_eq!(store["gen"], Node::new("gen", None, 0));
    assert_eq!(store["a"], Node::new("a", "gen", 1));
    assert_eq!(store["b"], Node::new("b", "a", 2));

    chain! {
        in store;
        gen -> a2 -> b2
    };
    assert_eq!(store.len(), 5);
    assert_eq!(store["a2"], Node::new("a2", "gen", 1));
    assert_eq!(store["b2"], Node::new("b2", "a2", 2));
}

#[derive(Debug, Error, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GetPathError {
    #[error(transparent)]
    NodeNotFound(#[from] NodeNotFound),
    #[error("no common ancestor between these nodes")]
    NoCommonAncestor,
}

pub fn get_path<'a>(
    store: &'a NodeStore<'a>,
    from: &str,
    to: &str,
) -> Result<Vec<PathChange<'a>>, GetPathError> {
    let mut from_lineage = lineage(store, from).peekable();
    let mut to_lineage = lineage(store, to).peekable();

    let common_height = cmp::min(
        from_lineage
            .peek()
            .map_or(Err(NodeNotFound), Clone::clone)?
            .height,
        to_lineage
            .peek()
            .map_or(Err(NodeNotFound), Clone::clone)?
            .height,
    );

    let mut reverts = scroll(&mut from_lineage, common_height)?;
    let mut applies = scroll(&mut to_lineage, common_height)?;

    for step in from_lineage.zip_longest(to_lineage) {
        match step {
            Both(from, to) => {
                let from = from?;
                let to = to?;
                if from == to {
                    return Ok(reverts
                        .into_iter()
                        .map(Revert)
                        .chain(applies.into_iter().rev().map(Apply))
                        .collect());
                } else {
                    reverts.push(from.id);
                    applies.push(to.id)
                }
            }
            Left(it) | Right(it) => {
                it?;
                break;
            }
        }
    }
    Err(GetPathError::NoCommonAncestor)
}

#[test]
fn impossible() {
    let mut store = NodeStore::new();
    chain! {
        in store;
        gen1 -> a1
    };
    chain! {
        in store;
        gen2 -> a2
    };
    assert_eq!(
        get_path(&store, "gen1", "gen2").unwrap_err(),
        GetPathError::NoCommonAncestor
    );
    assert_eq!(
        get_path(&store, "a1", "a2").unwrap_err(),
        GetPathError::NoCommonAncestor
    );
    assert_eq!(
        get_path(&store, "a1", "gen2").unwrap_err(),
        GetPathError::NoCommonAncestor
    );
}

#[test]
fn revert_to_ancestor_linear() {
    let mut store = NodeStore::new();
    chain! {
        in store;
        gen -> a -> b -> c
    };
    assert_eq!(get_path(&store, "c", "b").unwrap(), [Revert("c")]);
    assert_eq!(
        get_path(&store, "c", "a").unwrap(),
        [Revert("c"), Revert("b")]
    );
}

#[test]
fn apply_to_descendant_linear() {
    let mut store = NodeStore::new();
    chain! {
        in store;
        gen -> a -> b -> c
    };
    assert_eq!(get_path(&store, "b", "c").unwrap(), [Apply("c")]);
    assert_eq!(
        get_path(&store, "a", "c").unwrap(),
        [Apply("b"), Apply("c")]
    );
}

#[test]
fn noop() {
    let mut store = NodeStore::new();
    chain! {
        in store;
        gen -> a
    };
    assert_eq!(get_path(&store, "a", "a").unwrap(), []);
    assert_eq!(get_path(&store, "gen", "gen").unwrap(), []);
}

#[test]
fn cross_fork() {
    let mut store = NodeStore::new();
    chain! {
        in store;
        gen -> a -> b1 -> c1
    };
    chain! {
        in store;
        a -> b2 -> c2
    };

    // same height
    assert_eq!(
        get_path(&store, "b1", "b2").unwrap(),
        [Revert("b1"), Apply("b2")]
    );
    assert_eq!(
        get_path(&store, "c1", "c2").unwrap(),
        [Revert("c1"), Revert("b1"), Apply("b2"), Apply("c2")]
    );

    // jagged
    assert_eq!(
        get_path(&store, "b1", "c2").unwrap(),
        [Revert("b1"), Apply("b2"), Apply("c2")]
    );
    assert_eq!(
        get_path(&store, "c1", "b2").unwrap(),
        [Revert("c1"), Revert("b1"), Apply("b2")]
    );
}
