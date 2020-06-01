// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "json")]

use forest_blocks::header::json::{BlockHeaderJson, BlockHeaderJsonRef};
use serde_json::{from_str, to_string};

#[test]
fn iden() {
    let header_json = r#"{"Miner":"t01234","Ticket":{"VRFProof":"Ynl0ZSBhcnJheQ=="},"ElectionProof":{"VRFProof":"Ynl0ZSBhcnJheQ=="},"BeaconEntries":null,"WinPoStProof":null,"Parents":null,"ParentWeight":"0","Height":10101,"ParentStateRoot":{"/":"bafy2bzacea3wsdh6y3a36tb3skempjoxqpuyompjbmfeyf34fi3uy6uue42v4"},"ParentMessageReceipts":{"/":"bafy2bzacea3wsdh6y3a36tb3skempjoxqpuyompjbmfeyf34fi3uy6uue42v4"},"Messages":{"/":"bafy2bzacea3wsdh6y3a36tb3skempjoxqpuyompjbmfeyf34fi3uy6uue42v4"},"BLSAggregate":{"Type":2,"Data":"Ynl0ZSBhcnJheQ=="},"Timestamp":42,"BlockSig":{"Type":2,"Data":"Ynl0ZSBhcnJheQ=="},"ForkSignaling":42}"#;

    // Deserialize
    let BlockHeaderJson(cid_d) = from_str(header_json).unwrap();

    // Serialize
    let ser_cid = to_string(&BlockHeaderJsonRef(&cid_d)).unwrap();
    assert_eq!(ser_cid, header_json);
}
