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

// #[cfg(test)]
// quickcheck! {
//     fn quickcheck(val: Receipt) -> () {
//         assert_unchanged_via_json(val)
//     }
// }
