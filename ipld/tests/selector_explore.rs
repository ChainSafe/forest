// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "submodule_tests")]

use forest_ipld::selector::Selector;
use forest_ipld::{json, Ipld, PathSegment};
use serde::Deserialize;
use std::fs::File;
use std::io::BufReader;

#[derive(Deserialize)]
struct ExploreParams {
    #[serde(with = "json")]
    ipld: Ipld,
    path_segment: PathSegment,
}

#[derive(Deserialize)]
struct TestVector {
    description: Option<String>,
    initial_selector: Selector,
    explore: Vec<ExploreParams>,
    result_selector: Option<Selector>,
}

// Just needed because cannot deserialize the current selector position in recursive selectors
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
        let b = s1 == s2;
        b
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
    let file = File::open("./tests/ipld-traversal-vectors/selector_explore.json").unwrap();
    let reader = BufReader::new(file);
    let vectors: Vec<TestVector> =
        serde_json::from_reader(reader).expect("Test vector deserialization failed");
    for tv in vectors {
        let result = process_vector(tv.initial_selector, tv.explore);
        assert!(
            test_equal(&result, &tv.result_selector),
            "({}) Failed:\nExpected: {:?}\nFound: {:?}",
            tv.description.unwrap_or("Unnamed test case".to_owned()),
            tv.result_selector,
            result
        );
    }
}
