// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "json")]

use forest_ipld::{
    json::{self, IpldJson, IpldJsonRef},
    Ipld,
};
use serde::{Deserialize, Serialize};
use serde_json::{from_str, to_string};
use std::collections::BTreeMap;
use std::iter::FromIterator;

#[test]
fn deserialize_json_symmetric() {
    let test_json = r#"
    {
        "link": {
            "/": "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n"
        },
        "bytes": {
            "/": { "base64": "VGhlIHF1aQ==" }
        },
        "string": "Some data",
        "float": 10.5,
        "integer": 8,
        "neg_integer": -20,
        "null": null,
        "list": [null, { "/": "bafy2bzaceaa466o2jfc4g4ggrmtf55ygigvkmxvkr5mvhy4qbwlxetbmlkqjk" }, 1]
    }
    "#;
    let expected = Ipld::Map(BTreeMap::from_iter(
        [
            (
                "link".to_owned(),
                Ipld::Link(
                    "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n"
                        .parse()
                        .unwrap(),
                ),
            ),
            (
                "bytes".to_owned(),
                Ipld::Bytes([0x54, 0x68, 0x65, 0x20, 0x71, 0x75, 0x69].to_vec()),
            ),
            ("string".to_owned(), Ipld::String("Some data".to_owned())),
            ("float".to_owned(), Ipld::Float(10.5)),
            ("integer".to_owned(), Ipld::Integer(8)),
            ("neg_integer".to_owned(), Ipld::Integer(-20)),
            ("null".to_owned(), Ipld::Null),
            (
                "list".to_owned(),
                Ipld::List(vec![
                    Ipld::Null,
                    Ipld::Link(
                        "bafy2bzaceaa466o2jfc4g4ggrmtf55ygigvkmxvkr5mvhy4qbwlxetbmlkqjk"
                            .parse()
                            .unwrap(),
                    ),
                    Ipld::Integer(1),
                ]),
            ),
        ]
        .to_vec()
        .into_iter(),
    ));

    // Assert deserializing into expected Ipld
    let IpldJson(ipld_d) = from_str(test_json).unwrap();
    assert_eq!(&ipld_d, &expected, "Deserialized ipld does not match");

    // Symmetric tests
    let ser_json = to_string(&IpldJsonRef(&expected)).unwrap();
    let IpldJson(ipld_d) = from_str(&ser_json).unwrap();
    assert_eq!(&ipld_d, &expected, "Deserialized ipld does not match");
}

#[test]
fn annotating_struct_json() {
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestStruct {
        #[serde(with = "json")]
        one: Ipld,
        other: String,
    }
    let test_json = r#"
            {
                "one": [{ "/": "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n" }, null, 8],
                "other": "Some data"
            }
        "#;
    let expected = TestStruct {
        one: Ipld::List(vec![
            Ipld::Link(
                "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n"
                    .parse()
                    .unwrap(),
            ),
            Ipld::Null,
            Ipld::Integer(8),
        ]),
        other: "Some data".to_owned(),
    };

    assert_eq!(from_str::<TestStruct>(test_json).unwrap(), expected);
}

#[test]
fn link_edge_case() {
    // Test ported from go-ipld-prime (making sure edge case is handled)
    let test_json = r#"{"/":{"/":"QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n"}}"#;
    let expected = Ipld::Map(BTreeMap::from_iter(
        [(
            "/".to_owned(),
            Ipld::Link(
                "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n"
                    .parse()
                    .unwrap(),
            ),
        )]
        .to_vec(),
    ));

    let IpldJson(ipld_d) = from_str(test_json).unwrap();
    assert_eq!(&ipld_d, &expected, "Deserialized ipld does not match");
}
