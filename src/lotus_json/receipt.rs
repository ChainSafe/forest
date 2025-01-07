// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ::cid::Cid;
use fvm_ipld_encoding::RawBytes;

use super::*;
use crate::shim::executor::Receipt;

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "Receipt")]
pub struct ReceiptLotusJson {
    exit_code: u32,
    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json")]
    r#return: RawBytes,
    gas_used: u64,
    #[schemars(with = "LotusJson<Option<Cid>>")]
    #[serde(with = "crate::lotus_json", default)] // Lotus still does `"EventsRoot": null`
    events_root: Option<Cid>,
}

impl HasLotusJson for Receipt {
    type LotusJson = ReceiptLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![
            (
                json!({
                    "ExitCode": 0,
                    "Return": "aGVsbG8gd29ybGQh",
                    "GasUsed": 0,
                    "EventsRoot": null,
                }),
                Self::V3(fvm_shared3::receipt::Receipt {
                    exit_code: fvm_shared3::error::ExitCode::new(0),
                    return_data: RawBytes::new(Vec::from_iter(*b"hello world!")),
                    gas_used: 0,
                    events_root: None,
                }),
            ),
            (
                json!({
                    "ExitCode": 0,
                    "Return": "aGVsbG8gd29ybGQh",
                    "GasUsed": 0,
                    "EventsRoot": {
                        "/": "baeaaaaa"
                    }
                }),
                Self::V3(fvm_shared3::receipt::Receipt {
                    exit_code: fvm_shared3::error::ExitCode::new(0),
                    return_data: RawBytes::new(Vec::from_iter(*b"hello world!")),
                    gas_used: 0,
                    events_root: Some(Cid::default()),
                }),
            ),
        ]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        Self::LotusJson {
            exit_code: self.exit_code().value(),
            r#return: self.return_data(),
            gas_used: self.gas_used(),
            events_root: self.events_root(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson {
            exit_code,
            r#return,
            gas_used,
            events_root,
        } = lotus_json;
        Self::V3(fvm_shared3::receipt::Receipt {
            exit_code: fvm_shared3::error::ExitCode::new(exit_code),
            return_data: r#return,
            gas_used,
            events_root,
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
///
/// See <https://github.com/ChainSafe/forest/issues/3459>.
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
        "EventsRoot": null,
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
