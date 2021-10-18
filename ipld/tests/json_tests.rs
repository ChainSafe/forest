// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "json")]

use forest_ipld::{
    ipld,
    json::{self, IpldJson, IpldJsonRef},
    Ipld,
};
use serde::{Deserialize, Serialize};
use serde_json::{from_str, to_string};

#[test]
fn deserialize_json_symmetric() {
    let test_json = r#"
    {
        "link": {
            "/": "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n"
        },
        "bytes": {
            "/": { "bytes": "mVGhlIHF1aQ" }
        },
        "string": "Some data",
        "float": 10.5,
        "integer": 8,
        "neg_integer": -20,
        "null": null,
        "list": [null, { "/": "bafy2bzaceaa466o2jfc4g4ggrmtf55ygigvkmxvkr5mvhy4qbwlxetbmlkqjk" }, 1]
    }
    "#;
    let expected = ipld!({
        "link": Link("QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n".parse().unwrap()),
        "bytes": Bytes(vec![0x54, 0x68, 0x65, 0x20, 0x71, 0x75, 0x69]),
        "string": "Some data",
        "float": 10.5,
        "integer": 8,
        "neg_integer": -20,
        "null": null,
        "list": [
            null,
            Link("bafy2bzaceaa466o2jfc4g4ggrmtf55ygigvkmxvkr5mvhy4qbwlxetbmlkqjk".parse().unwrap()),
            1,
        ],
    });

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
        one: ipld!([
            Link(
                "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n"
                    .parse()
                    .unwrap()
            ),
            null,
            8
        ]),
        other: "Some data".to_owned(),
    };

    assert_eq!(from_str::<TestStruct>(test_json).unwrap(), expected);
}

#[test]
fn link_edge_case() {
    // Test ported from go-ipld-prime (making sure edge case is handled)
    let test_json = r#"{"/":{"/":"QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n"}}"#;
    let expected =
        ipld!({"/": Link("QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n".parse().unwrap())});

    let IpldJson(ipld_d) = from_str(test_json).unwrap();
    assert_eq!(&ipld_d, &expected, "Deserialized ipld does not match");
}
