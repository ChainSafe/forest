// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{vec_u8::VecU8LotusJson, *};
use fvm_ipld_encoding::RawBytes;

#[test]
fn snapshots() {
    assert_all_snapshots::<fvm_ipld_encoding::RawBytes>();
}

#[cfg(test)]
quickcheck! {
    fn quickcheck(val: Vec<u8>) -> () {
        assert_unchanged_via_json(RawBytes::new(val))
    }
}

impl HasLotusJson for RawBytes {
    type LotusJson = VecU8LotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!("aGVsbG8gd29ybGQh"),
            RawBytes::new(Vec::from_iter(*b"hello world!")),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        Vec::from(self).into_lotus_json()
    }

    fn from_lotus_json(value: Self::LotusJson) -> Self {
        Self::from(Vec::from_lotus_json(value))
    }
}
