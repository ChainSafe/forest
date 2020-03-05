// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_cid::{Cid, Codec, Error, Prefix, Version};
use multihash::{self, Blake2b256, Code, Sha2_256};
use std::collections::HashMap;

#[test]
fn basic_marshalling() {
    let h = Sha2_256::digest(b"beep boop");

    let cid = Cid::new(Codec::DagProtobuf, Version::V1, h);

    let data = cid.to_bytes();
    let out = Cid::from_raw_cid(data).unwrap();

    assert_eq!(cid, out);

    let s = cid.to_string();
    let out2 = Cid::from_raw_cid(&s[..]).unwrap();

    assert_eq!(cid, out2);
}

#[test]
fn empty_string() {
    assert_eq!(Cid::from_raw_cid(""), Err(Error::InputTooShort));
}

#[test]
fn v0_handling() {
    let old = "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n";
    let cid = Cid::from_raw_cid(old).unwrap();

    assert_eq!(cid.version, Version::V0);
    assert_eq!(cid.to_string(), old);
}

#[test]
fn from_str() {
    let cid: Cid = "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n"
        .parse()
        .unwrap();
    assert_eq!(cid.version, Version::V0);

    let bad = "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zIII".parse::<Cid>();
    assert_eq!(bad, Err(Error::ParsingError));
}

#[test]
fn v0_error() {
    let bad = "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zIII";
    assert_eq!(Cid::from_raw_cid(bad), Err(Error::ParsingError));
}

#[test]
fn prefix_roundtrip() {
    let data = b"awesome test content";
    let h = Sha2_256::digest(data);

    let cid = Cid::new(Codec::DagProtobuf, Version::V1, h);
    let prefix = cid.prefix();

    let cid2 = Cid::new_from_prefix(&prefix, data).unwrap();

    assert_eq!(cid, cid2);

    let prefix_bytes = prefix.as_bytes();
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
        let cid = Cid::from_raw_cid(case).unwrap();
        assert_eq!(cid.version, Version::V0);
        assert_eq!(cid.to_string(), the_hash);
    }
}

#[test]
fn test_hash() {
    let data: Vec<u8> = vec![1, 2, 3];
    let prefix = Prefix {
        version: Version::V0,
        codec: Codec::DagProtobuf,
        mh_type: Code::Sha2_256,
    };
    let mut map = HashMap::new();
    let cid = Cid::new_from_prefix(&prefix, &data).unwrap();
    map.insert(cid.clone(), data.clone());
    assert_eq!(&data, map.get(&cid).unwrap());
}

#[test]
fn test_prefix_retrieval() {
    let data: Vec<u8> = vec![1, 2, 3];

    let cid = Cid::new_from_cbor(&data, Blake2b256).unwrap();

    let prefix = cid.prefix();
    assert_eq!(prefix.version, Version::V1);
    assert_eq!(prefix.codec, Codec::DagCBOR);
    assert_eq!(prefix.mh_type, Code::Blake2b256);
}
