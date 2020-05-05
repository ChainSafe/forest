// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod empty_map;
mod path_segment;

use super::Ipld;
pub use path_segment::PathSegment;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Selectors are expressions that identify and select a subset of data from an IPLD DAG.
/// Selectors are themselves IPLD and can be serialized and deserialized as such.
// TODO usage docs when API solidified
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Selector {
    /// Matcher marks a node to be included in the "result" set.
    /// (All nodes traversed by a selector are in the "covered" set (which is a.k.a.
    /// "the merkle proof"); the "result" set is a subset of the "covered" set.)
    ///
    /// In libraries using selectors, the "result" set is typically provided to
    /// some user-specified callback.
    ///
    /// A selector tree with only "explore*"-type selectors and no Matcher selectors
    /// is valid; it will just generate a "covered" set of nodes and no "result" set.
    #[serde(rename = ".", with = "empty_map")]
    Matcher,

    /// ExploreAll is similar to a `*` -- it traverses all elements of an array,
    /// or all entries in a map, and applies a next selector to the reached nodes.
    #[serde(rename = "a")]
    ExploreAll {
        #[serde(rename = ">")]
        next: Box<Selector>,
    },

    /// ExploreFields traverses named fields in a map (or equivalently, struct, if
    /// traversing on typed/schema nodes) and applies a next selector to the
    /// reached nodes.
    ///
    /// Note that a concept of exploring a whole path (e.g. "foo/bar/baz") can be
    /// represented as a set of three nexted ExploreFields selectors, each
    /// specifying one field.
    #[serde(rename = "f")]
    ExploreFields {
        #[serde(rename = "f>")]
        fields: BTreeMap<String, Selector>,
    },

    /// ExploreIndex traverses a specific index in a list, and applies a next
    /// selector to the reached node.
    #[serde(rename = "i")]
    ExploreIndex {
        #[serde(rename = "i")]
        index: usize,
        #[serde(rename = ">")]
        next: Box<Selector>,
    },

    /// ExploreRange traverses a list, and for each element in the range specified,
    /// will apply a next selector to those reached nodes.
    #[serde(rename = "r")]
    ExploreRange {
        #[serde(rename = "^")]
        start: usize,
        #[serde(rename = "$")]
        end: usize,
        #[serde(rename = ">")]
        next: Box<Selector>,
    },

    /// ExploreRecursive traverses some structure recursively.
    /// To guide this exploration, it uses a "sequence", which is another Selector
    /// tree; some leaf node in this sequence should contain an ExploreRecursiveEdge
    /// selector, which denotes the place recursion should occur.
    ///
    /// In implementation, whenever evaluation reaches an ExploreRecursiveEdge marker
    /// in the recursion sequence's Selector tree, the implementation logically
    /// produces another new Selector which is a copy of the original
    /// ExploreRecursive selector, but with a decremented depth parameter for limit
    /// (if limit is of type depth), and continues evaluation thusly.
    ///
    /// It is not valid for an ExploreRecursive selector's sequence to contain
    /// no instances of ExploreRecursiveEdge; it *is* valid for it to contain
    /// more than one ExploreRecursiveEdge.
    ///
    /// ExploreRecursive can contain a nested ExploreRecursive!
    /// This is comparable to a nested for-loop.
    /// In these cases, any ExploreRecursiveEdge instance always refers to the
    /// nearest parent ExploreRecursive (in other words, ExploreRecursiveEdge can
    /// be thought of like the 'continue' statement, or end of a for-loop body;
    /// it is *not* a 'goto' statement).
    ///
    /// Be careful when using ExploreRecursive with a large depth limit parameter;
    /// it can easily cause very large traversals (especially if used in combination
    /// with selectors like ExploreAll inside the sequence).
    ///
    /// limit is a union type -- it can have an integer depth value (key "depth") or
    /// no value (key "none"). If limit has no value it is up to the
    /// implementation library using selectors to identify an appropriate max depth
    /// as neccesary so that recursion is not infinite
    #[serde(rename = "R")]
    ExploreRecursive {
        #[serde(rename = ":>")]
        sequence: Box<Selector>,
        #[serde(rename = "l")]
        limit: RecursionLimit,
        /// if a node matches, we won't match it nor explore its children.
        #[serde(rename = "!")]
        stop_at: Option<Condition>,
        #[serde(skip_deserializing)]
        /// Used to index current
        // TODO determine if this can be a reference to a selector
        current: Option<Box<Selector>>,
    },

    /// ExploreUnion allows selection to continue with two or more distinct selectors
    /// while exploring the same tree of data.
    ///
    /// ExploreUnion can be used to apply a Matcher on one node (causing it to
    /// be considered part of a (possibly labelled) result set), while simultaneously
    /// continuing to explore deeper parts of the tree with another selector,
    /// for example.
    #[serde(rename = "|")]
    ExploreUnion(Vec<Selector>),

    /// ExploreRecursiveEdge is a special sentinel value which is used to mark
    /// the end of a sequence started by an ExploreRecursive selector: the recursion
    /// goes back to the initial state of the earlier ExploreRecursive selector,
    /// and proceeds again (with a decremented maxDepth value).
    ///
    /// An ExploreRecursive selector that doesn't contain an ExploreRecursiveEdge
    /// is nonsensical.  Containing more than one ExploreRecursiveEdge is valid.
    /// An ExploreRecursiveEdge without an enclosing ExploreRecursive is an error.
    #[serde(rename = "@", with = "empty_map")]
    ExploreRecursiveEdge,
    //* No conditional explore impl exists, ignore for now
    // #[serde(rename = "&")]
    // ExploreConditional {
    //     #[serde(rename = "&")]
    //     condition: Option<Condition>,
    //     #[serde(rename = ">")]
    //     next: Box<Selector>,
    // },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Copy)]
pub enum RecursionLimit {
    #[serde(rename = "none", with = "empty_map")]
    None,
    #[serde(rename = "depth")]
    Depth(u64),
}

/// Condition is expresses a predicate with a boolean result.
///
/// Condition clauses are used several places:
///   - in Matcher, to determine if a node is selected.
///   - in ExploreRecursive, to halt exploration.
///   - in ExploreConditional,
///
///
/// TODO -- Condition is very skeletal and incomplete.
/// The place where Condition appears in other structs is correct;
/// the rest of the details inside it are not final nor even completely drafted.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Copy)]
pub enum Condition {
    #[serde(rename = "hasField")]
    HasField,
    #[serde(rename = "=")]
    HasValue,
    #[serde(rename = "%")]
    HasKind,
    #[serde(rename = "/")]
    IsLink,
    #[serde(rename = "greaterThan")]
    GreaterThan,
    #[serde(rename = "lessThan")]
    LessThan,
    #[serde(rename = "and")]
    And,
    #[serde(rename = "or")]
    Or,
}

impl Selector {
    /// Returns a vector of all sectors of interest, `None` variant is synonymous with all.
    pub fn interests(&self) -> Option<Vec<PathSegment>> {
        use Selector::*;
        match self {
            ExploreAll { .. } => None,
            ExploreFields { fields } => {
                Some(fields.keys().cloned().map(PathSegment::from).collect())
            }
            ExploreIndex { index, .. } => Some(vec![(*index).into()]),
            ExploreRange { start, end, .. } => {
                if end < start {
                    return None;
                }
                let mut inter = Vec::with_capacity(end - start);
                for i in *start..*end {
                    inter.push(PathSegment::from(i));
                }
                Some(inter)
            }
            ExploreRecursive {
                current, sequence, ..
            } => {
                if let Some(selector) = current {
                    selector.interests()
                } else {
                    sequence.interests()
                }
            }
            ExploreRecursiveEdge => {
                // Should never be called on this variant
                panic!("Traversed explore recursive edge node with no parent")
            }
            ExploreUnion(selectors) => {
                let mut segs = Vec::new();
                for m in selectors {
                    if let Some(i) = m.interests() {
                        segs.extend_from_slice(&i);
                    } else {
                        // if any member has all interests, union will as well
                        return None;
                    }
                }
                Some(segs)
            }
            Matcher => {
                // Intentionally an empty vector
                Some(vec![])
            }
        }
    }

    /// Processes and returns resultant selector node
    pub fn explore(self, _ipld: &Ipld, _p: &PathSegment) -> Option<Selector> {
        // TODO
        todo!()
    }

    /// Returns true if matcher, false otherwise
    pub fn decide(&self) -> bool {
        use Selector::*;
        match self {
            Matcher => true,
            ExploreUnion(selectors) => {
                for s in selectors {
                    if s.decide() {
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }
}
