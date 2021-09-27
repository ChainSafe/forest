// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "json")]

use forest_cid::{
    json::{self, CidJson, CidJsonRef},
    Cid,
};
use serde::{Deserialize, Serialize};
use serde_json::{from_str, to_string};

#[test]
fn symmetric_json_serialization() {
    let cid: Cid = "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n"
        .parse()
        .unwrap();
    let cid_json = r#"{"/":"QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n"}"#;

    // Deserialize
    let CidJson(cid_d) = from_str(cid_json).unwrap();
    assert_eq!(&cid_d, &cid, "Deserialized cid does not match");

    // Serialize
    let ser_cid = to_string(&CidJsonRef(&cid_d)).unwrap();
    assert_eq!(ser_cid, cid_json);
}

#[test]
fn annotating_struct_json() {
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestStruct {
        #[serde(with = "json")]
        cid_one: Cid,
        #[serde(with = "json")]
        cid_two: Cid,
        other: String,
    }
    let test_json = r#"
            {
                "cid_one": {
                    "/": "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n"
                },
                "cid_two": {
                    "/": "bafy2bzaceaa466o2jfc4g4ggrmtf55ygigvkmxvkr5mvhy4qbwlxetbmlkqjk"
                },
                "other": "Some data"
            }
        "#;
    let expected = TestStruct {
        cid_one: "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n"
            .parse()
            .unwrap(),
        cid_two: "bafy2bzaceaa466o2jfc4g4ggrmtf55ygigvkmxvkr5mvhy4qbwlxetbmlkqjk"
            .parse()
            .unwrap(),
        other: "Some data".to_owned(),
    };

    assert_eq!(from_str::<TestStruct>(test_json).unwrap(), expected);
}
