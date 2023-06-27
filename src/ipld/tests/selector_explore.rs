// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::ipld::{json, selector::Selector, Ipld};
use serde::Deserialize;

#[derive(Deserialize)]
struct ExploreParams {
    #[serde(with = "json")]
    ipld: Ipld,
    path_segment: String,
}

#[derive(Deserialize)]
struct TestVector {
    description: Option<String>,
    initial_selector: Selector,
    explore: Vec<ExploreParams>,
    result_selector: Option<Selector>,
}

// Just needed because cannot deserialize the current selector position in
// recursive selectors
fn test_equal(s1: &Option<Selector>, s2: &Option<Selector>) -> bool {
    use Selector::*;
    if let (
        &Some(ExploreRecursive {
            current: _,
            sequence: s1,
            limit: l1,
            stop_at: st1,
        }),
        &Some(ExploreRecursive {
            current: _,
            sequence: s2,
            limit: l2,
            stop_at: st2,
        }),
    ) = (&s1, &s2)
    {
        s1 == s2 && l1 == l2 && st1 == st2
    } else {
        s1 == s2
    }
}

fn process_vector(initial_selector: Selector, params: Vec<ExploreParams>) -> Option<Selector> {
    let mut current = Some(initial_selector);
    for p in params {
        current = current?.explore(&p.ipld, &p.path_segment);
    }
    current
}

#[test]
fn selector_explore_tests() {
    let s = include_str!("ipld-traversal-vectors/selector_explore.json");
    let vectors: Vec<TestVector> =
        serde_json::from_str(s).expect("Test vector deserialization failed");
    for tv in vectors {
        let result = process_vector(tv.initial_selector, tv.explore);
        assert!(
            test_equal(&result, &tv.result_selector),
            "({}) Failed:\nExpected: {:?}\nFound: {:?}",
            tv.description
                .unwrap_or_else(|| "Unnamed test case".to_owned()),
            tv.result_selector,
            result
        );
    }
}
