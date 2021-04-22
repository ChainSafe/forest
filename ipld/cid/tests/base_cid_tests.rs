// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_cid::{Cid, Code, Error, Prefix, Version, DAG_CBOR};
use multihash::{self, MultihashDigest};
use std::collections::HashMap;
use std::convert::TryFrom;

#[test]
fn basic_marshalling() {
    let h = Code::Blake2b256.digest(b"beep boop");

    let cid = Cid::new_v1(DAG_CBOR, h);

    let data = cid.to_bytes();
    let out = Cid::try_from(data).unwrap();

    assert_eq!(cid, out);

    let s = cid.to_string();
    let out2 = Cid::try_from(&s[..]).unwrap();

    assert_eq!(cid, out2);
}

#[test]
fn empty_string() {
    assert!(matches!(Cid::try_from(""), Err(Error::InputTooShort)));
}

#[test]
fn v0_handling() {
    let old = "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n";
    let cid = Cid::try_from(old).unwrap();

    assert_eq!(cid.version(), Version::V0);
    assert_eq!(cid.to_string(), old);
}

#[test]
fn from_str() {
    let cid: Cid = "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n"
        .parse()
        .unwrap();
    assert_eq!(cid.version(), Version::V0);

    let bad = "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zIII".parse::<Cid>();
    assert!(matches!(bad, Err(Error::ParsingError)));
}

#[test]
fn v0_error() {
    let bad = "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zIII";
    assert!(matches!(Cid::try_from(bad), Err(Error::ParsingError)));
}

#[test]
fn prefix_roundtrip() {
    let data = b"awesome test content";
    let h = Code::Blake2b256.digest(data);

    let cid = Cid::new_v1(DAG_CBOR, h);
    let prefix = Prefix::from(cid);

    let cid2 = forest_cid::new_from_prefix(&prefix, data).unwrap();

    assert_eq!(cid, cid2);

    let prefix_bytes = prefix.to_bytes();
    let prefix2 = Prefix::new_from_bytes(&prefix_bytes).unwrap();

    assert_eq!(prefix, prefix2);
}

#[test]
fn from() {
    let the_hash = "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n";

    let cases = vec![
        format!("/ipfs/{:}", &the_hash),
        format!("https://ipfs.io/ipfs/{:}", &the_hash),
        format!("http://localhost:8080/ipfs/{:}", &the_hash),
    ];

    for case in cases {
        let cid = Cid::try_from(case).unwrap();
        assert_eq!(cid.version(), Version::V0);
        assert_eq!(cid.to_string(), the_hash);
    }
}

#[test]
fn test_hash() {
    let data: Vec<u8> = vec![1, 2, 3];
    let prefix = Prefix {
        version: Version::V1,
        codec: DAG_CBOR,
        mh_type: Code::Blake2b256.into(),
        mh_len: 32,
    };
    let mut map = HashMap::new();
    let cid = forest_cid::new_from_prefix(&prefix, &data).unwrap();
    map.insert(cid, data.clone());
    assert_eq!(&data, map.get(&cid).unwrap());
}

#[test]
fn test_prefix_retrieval() {
    let data: Vec<u8> = vec![1, 2, 3];

    let cid = forest_cid::new_from_cbor(&data, Code::Blake2b256);

    let prefix = Prefix::from(cid);
    assert_eq!(prefix.version, Version::V1);
    assert_eq!(prefix.codec, DAG_CBOR);
    assert_eq!(prefix.mh_type, Code::Blake2b256.into());
}
