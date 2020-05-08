// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_ipld::selector::{PathSegment, Selector};
use forest_ipld::Ipld;
use serde::Deserialize;
use std::fs::File;
use std::io::BufReader;

#[derive(Deserialize)]
struct ExploreParams {
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

fn process_vector(
    initial_selector: Selector,
    params: Vec<ExploreParams>,
    expected_result: Option<Selector>,
    description: &str,
) {
    let mut current = Some(initial_selector);
    for p in params {
        current = current.unwrap().explore(&p.ipld, &p.path_segment);
    }

    assert_eq!(current, expected_result, "{}", description);
}

#[test]
fn selector_explore_tests() {
    let file = File::open("./tests/selector_explore.json").unwrap();
    let reader = BufReader::new(file);

    let vectors: Vec<TestVector> =
        serde_json::from_reader(reader).expect("Test vector deserialization failed");
    for tv in vectors {
        process_vector(
            tv.initial_selector,
            tv.explore,
            tv.result_selector,
            &tv.description.unwrap_or("Unnamed test case".to_owned()),
        );
    }
}
