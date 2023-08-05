use crate::shim::sector::PoStProof;

use super::*;

pub struct PoStProofLotusJson {}

impl HasLotusJson for PoStProof {
    type LotusJson = PoStProofLotusJson;
}
