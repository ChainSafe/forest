use crate::blocks::Ticket;

use super::*;

#[derive(Serialize, Deserialize)]
pub struct TicketLotusJson {
    #[serde(rename = "VRFProof")]
    vrfproof: VRFProofLotusJson,
}

impl HasLotusJson for Ticket {
    type LotusJson = TicketLotusJson;
}

impl From<Ticket> for TicketLotusJson {
    fn from(value: Ticket) -> Self {
        let Ticket { vrfproof } = value;
        Self {
            vrfproof: vrfproof.into(),
        }
    }
}

impl From<TicketLotusJson> for Ticket {
    fn from(value: TicketLotusJson) -> Self {
        let TicketLotusJson { vrfproof } = value;
        Self {
            vrfproof: vrfproof.into(),
        }
    }
}

#[test]
fn test() {
    assert_snapshot(
        json!({"VRFProof": "aGVsbG8gd29ybGQh"}),
        Ticket {
            // TODO(aatifsyed): why does this domain struct live in crate::json??
            vrfproof: crate::json::vrf::VRFProof(Vec::from_iter(*b"hello world!")),
        },
    );
}

#[cfg(test)]
quickcheck! {
    fn round_trip(val: Vec<u8>) -> bool {
        assert_via_json(Ticket {
            // TODO(aatifsyed): why does this domain struct live in crate::json??
            vrfproof: crate::json::vrf::VRFProof(val),
        });
        true
    }
}
