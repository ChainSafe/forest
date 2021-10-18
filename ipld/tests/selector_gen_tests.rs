// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_ipld::selector::{RecursionLimit, Selector};
use indexmap::IndexMap;
use serde_json::{from_str, to_string};

// For readability of tests
use Selector::*;

fn deserialize_and_check(json: &str, expected: Selector) {
    // Assert deserializing into expected Selector
    let s: Selector = from_str(json).unwrap();
    assert_eq!(s, expected, "Deserialized selector does not match");

    // Test symmetric encoding and decoding
    let ser_json = to_string(&expected).unwrap();
    let selector_d: Selector = from_str(&ser_json).unwrap();
    assert_eq!(&selector_d, &expected, "Symmetric deserialization failed");
}

#[test]
fn gen_matcher() {
    deserialize_and_check(r#"{ ".": { } }"#, Matcher);
}

#[test]
fn gen_explore_recursive_edge() {
    deserialize_and_check(r#"{ "@": { } }"#, ExploreRecursiveEdge);
}

#[test]
fn gen_explore_all() {
    let test_json = r#"
    { 
        "a": { ">": { ".": {} } }
    }
    "#;
    let expected = ExploreAll {
        next: Matcher.into(),
    };

    deserialize_and_check(test_json, expected);
}

#[test]
fn gen_explore_fields() {
    let test_json = r#"
    {
        "f": { 
            "f>": {
                "one": { ".": {} },
                "two": { "@": {} }
            }
        }
    }
    "#;
    let mut fields = IndexMap::new();
    fields.insert("one".to_owned(), Matcher);
    fields.insert("two".to_owned(), ExploreRecursiveEdge);
    let expected = ExploreFields { fields };

    deserialize_and_check(test_json, expected);
}

#[test]
fn gen_explore_index() {
    let test_json = r#"
    {
        "i": {
            "i": 2,
            ">": { "@": {} }
        }
    }
    "#;
    let expected = ExploreIndex {
        index: 2,
        next: ExploreRecursiveEdge.into(),
    };

    deserialize_and_check(test_json, expected);
}

#[test]
fn gen_explore_range() {
    let test_json = r#"
    {
        "r": {
            "^": 1,
            "$": 4,
            ">": { "@": {} }
        }
    }
    "#;
    let expected = ExploreRange {
        start: 1,
        end: 4,
        next: ExploreRecursiveEdge.into(),
    };

    deserialize_and_check(test_json, expected);
}

#[test]
fn gen_explore_recursive() {
    let test_json = r#"
    {
        "R": {
            "l": { "depth": 3 },
            ":>": { ".": {} }
        }
    }
    "#;
    let expected = ExploreRecursive {
        sequence: Matcher.into(),
        limit: RecursionLimit::Depth(3),
        stop_at: None,
        current: None,
    };

    deserialize_and_check(test_json, expected);

    let test_json = r#"
    {
        "R": {
            "l": { "none": {} },
            ":>": { ".": {} }
        }
    }
    "#;
    let expected = ExploreRecursive {
        sequence: Matcher.into(),
        limit: RecursionLimit::None,
        stop_at: None,
        current: None,
    };

    deserialize_and_check(test_json, expected);
}

#[test]
fn gen_explore_union() {
    let test_json = r#"
    {
        "|": [{ ".": {} }, { "@": {} }]
    }
    "#;
    let expected = ExploreUnion(vec![Matcher, ExploreRecursiveEdge]);

    deserialize_and_check(test_json, expected);
}

// #[test]
// fn gen_explore_conditional() {
//     let test_json = r#"
//     {
//         "&": { ">": { ".": {} } }
//     }
//     "#;
//     let expected = ExploreConditional {
//         next: Matcher.into(),
//         condition: None,
//     };

//     deserialize_and_check(test_json, expected);
// }
