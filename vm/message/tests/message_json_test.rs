// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "json")]

use forest_message::unsigned_message::json::{UnsignedMessageJson, UnsignedMessageJsonRef};
use serde::{Deserialize, Serialize};
use serde_json::{from_str, to_string};

#[test]
fn symmetric_json_serialization() {
    let message_json = r#"{"Version":9,"To":"t01234","From":"t01234","Nonce":42,"Value":"0","GasPrice":"0","GasLimit":9,"Method":1,"Params":"Ynl0ZSBhcnJheQ=="}"#;

    // Deserialize
    let UnsignedMessageJson(cid_d) = from_str(message_json).unwrap();

    // Serialize
    let ser_cid = to_string(&UnsignedMessageJsonRef(&cid_d)).unwrap();
    assert_eq!(ser_cid, message_json);
}

#[test]
fn annotating_struct_json() {
    // #[derive(Serialize, Deserialize, Debug, PartialEq)]
    // struct TestStruct {
    //     #[serde(with = "json")]
    //     cid_one: Cid,
    //     #[serde(with = "json")]
    //     cid_two: Cid,
    //     other: String,
    // }
    // let test_json = r#"
    //         {
    //             "cid_one": {
    //                 "/": "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n"
    //             },
    //             "cid_two": {
    //                 "/": "bafy2bzaceaa466o2jfc4g4ggrmtf55ygigvkmxvkr5mvhy4qbwlxetbmlkqjk"
    //             },
    //             "other": "Some data"
    //         }
    //     "#;
    // let expected = TestStruct {
    //     cid_one: "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n"
    //         .parse()
    //         .unwrap(),
    //     cid_two: "bafy2bzaceaa466o2jfc4g4ggrmtf55ygigvkmxvkr5mvhy4qbwlxetbmlkqjk"
    //         .parse()
    //         .unwrap(),
    //     other: "Some data".to_owned(),
    // };

    // assert_eq!(from_str::<TestStruct>(test_json).unwrap(), expected);
}
