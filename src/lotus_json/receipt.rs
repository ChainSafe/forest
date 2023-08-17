// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_ipld_encoding::RawBytes;

use super::*;
use crate::shim::executor::Receipt;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ReceiptLotusJson {
    exit_code: LotusJson<u32>,
    r#return: LotusJson<RawBytes>,
    gas_used: LotusJson<u64>,
}

impl HasLotusJson for Receipt {
    type LotusJson = ReceiptLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "ExitCode": 0,
                "Return": "aGVsbG8gd29ybGQh",
                "GasUsed": 0,
            }),
            Self::V3(fvm_shared3::receipt::Receipt {
                exit_code: fvm_shared3::error::ExitCode::new(0),
                return_data: RawBytes::new(Vec::from_iter(*b"hello world!")),
                gas_used: 0,
                events_root: None,
            }),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        Self::LotusJson {
            exit_code: self.exit_code().value().into(),
            r#return: self.return_data().into(),
            gas_used: self.gas_used().into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson {
            exit_code,
            r#return,
            gas_used,
        } = lotus_json;
        Self::V3(fvm_shared3::receipt::Receipt {
            exit_code: fvm_shared3::error::ExitCode::new(exit_code.into_inner()),
            return_data: r#return.into_inner(),
            gas_used: gas_used.into_inner(),
            events_root: None,
        })
    }
}

#[test]
fn shapshots() {
    assert_all_snapshots::<Receipt>()
}

/// [Receipt] knows if it is `V2` or `V3`, but there's no way for
/// the serialized representation to retain that information,
/// so [`assert_unchanged_via_json`] tests with arbitrary input will fail.
///
/// This can only be fixed by rewriting [Receipt].
#[test]
#[should_panic = "cannot serialize to v2 AND v3 from the same input"]
fn cannot_call_arbitrary_tests_on_receipt() {
    use pretty_assertions::assert_eq;

    let v2 = Receipt::V2(fvm_shared2::receipt::Receipt {
        exit_code: fvm_shared2::error::ExitCode::new(0),
        return_data: RawBytes::new(Vec::from_iter(*b"hello world!")),
        gas_used: 0,
    });
    let v3 = Receipt::V3(fvm_shared3::receipt::Receipt {
        exit_code: fvm_shared3::error::ExitCode::new(0),
        return_data: RawBytes::new(Vec::from_iter(*b"hello world!")),
        gas_used: 0,
        events_root: None,
    });
    let json = json!({
        "ExitCode": 0,
        "Return": "aGVsbG8gd29ybGQh",
        "GasUsed": 0,
    });

    // they serialize to the same thing...
    assert_eq!(
        serde_json::to_value(v2.clone().into_lotus_json()).unwrap(),
        json
    );
    assert_eq!(
        serde_json::to_value(v3.clone().into_lotus_json()).unwrap(),
        json
    );

    // both of these cannot pass at the same time...
    assert_eq!(
        v2,
        serde_json::from_value::<LotusJson<_>>(json.clone())
            .unwrap()
            .into_inner(),
        "cannot serialize to v2 AND v3 from the same input"
    );
    assert_eq!(
        v3,
        serde_json::from_value::<LotusJson<_>>(json)
            .unwrap()
            .into_inner(),
        "cannot serialize to v2 AND v3 from the same input"
    );
}
