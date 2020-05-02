// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_ipld::{
    json::{self, IpldJson, IpldJsonRef},
    Ipld,
};
use serde::{Deserialize, Serialize};
use serde_json::{from_str, to_string, Value};
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

// #[derive(Serialize, Deserialize, Clone)]
// struct TestStruct {
//     name: String,
//     details: Cid,
// }

// #[test]
// fn encode_new_type() {
//     let details = Cid::new_from_cbor(&[1, 2, 3], Blake2b256);
//     let name = "Test".to_string();
//     let t_struct = TestStruct {
//         name: name.clone(),
//         details: details.clone(),
//     };
//     let struct_encoded = to_vec(&t_struct).unwrap();

//     // Test to make sure struct can be encoded and decoded without IPLD
//     let struct_decoded: TestStruct = from_slice(&struct_encoded).unwrap();
//     assert_eq!(&struct_decoded.name, &name);
//     assert_eq!(&struct_decoded.details, &details.clone());

//     // Test ipld decoding
//     let ipld_decoded: Ipld = from_slice(&struct_encoded).unwrap();
//     let mut e_map = BTreeMap::<String, Ipld>::new();
//     e_map.insert("details".to_string(), Ipld::Link(details));
//     e_map.insert("name".to_string(), Ipld::String(name));
//     assert_eq!(&ipld_decoded, &Ipld::Map(e_map));
// }
