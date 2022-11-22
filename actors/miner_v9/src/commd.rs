use cid::{Cid, Version};
use fil_actors_runtime_v9::{actor_error, ActorError};
use fvm_shared::commcid::{FIL_COMMITMENT_UNSEALED, SHA2_256_TRUNC254_PADDED};
use fvm_shared::sector::RegisteredSealProof;
use multihash::Multihash;
use serde::{Deserialize, Serialize};

/// CompactCommD represents a Cid with compact representation of context dependant zero value
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
#[serde(transparent)]
pub struct CompactCommD(pub Option<Cid>);

impl CompactCommD {
    pub fn new(commd: Option<Cid>) -> Self {
        CompactCommD(commd)
    }
    pub fn get_cid(&self, seal_proof: RegisteredSealProof) -> Result<Cid, ActorError> {
        match self.0 {
            Some(ref x) => Ok(*x),
            None => zero_commd(seal_proof),
        }
    }
}

/// Prefix for unsealed sector CIDs (CommD).
pub fn is_unsealed_sector(c: &Cid) -> bool {
    c.version() == Version::V1
        && c.codec() == FIL_COMMITMENT_UNSEALED
        && c.hash().code() == SHA2_256_TRUNC254_PADDED
        && c.hash().size() == 32
}

const ZERO_COMMD_HASH: [[u8; 32]; 5] = [
    [
        252, 126, 146, 130, 150, 229, 22, 250, 173, 233, 134, 178, 143, 146, 212, 74, 79, 36, 185,
        53, 72, 82, 35, 55, 106, 121, 144, 39, 188, 24, 248, 51,
    ],
    [
        57, 86, 14, 123, 19, 169, 59, 7, 162, 67, 253, 39, 32, 255, 167, 203, 62, 29, 46, 80, 90,
        179, 98, 158, 121, 244, 99, 19, 81, 44, 218, 6,
    ],
    [
        101, 242, 158, 93, 152, 210, 70, 195, 139, 56, 140, 252, 6, 219, 31, 107, 2, 19, 3, 197,
        162, 137, 0, 11, 220, 232, 50, 169, 195, 236, 66, 28,
    ],
    [
        7, 126, 95, 222, 53, 197, 10, 147, 3, 165, 80, 9, 227, 73, 138, 78, 190, 223, 243, 156, 66,
        183, 16, 183, 48, 216, 236, 122, 199, 175, 166, 62,
    ],
    [
        230, 64, 5, 166, 191, 227, 119, 121, 83, 184, 173, 110, 249, 63, 15, 202, 16, 73, 178, 4,
        22, 84, 242, 164, 17, 247, 112, 39, 153, 206, 206, 2,
    ],
];

fn zero_commd(seal_proof: RegisteredSealProof) -> Result<Cid, ActorError> {
    let mut seal_proof = seal_proof;
    seal_proof.update_to_v1();
    let i = match seal_proof {
        RegisteredSealProof::StackedDRG2KiBV1P1 => 0,
        RegisteredSealProof::StackedDRG512MiBV1P1 => 1,
        RegisteredSealProof::StackedDRG8MiBV1P1 => 2,
        RegisteredSealProof::StackedDRG32GiBV1P1 => 3,
        RegisteredSealProof::StackedDRG64GiBV1P1 => 4,
        _ => {
            return Err(actor_error!(illegal_argument, "unknown SealProof"));
        }
    };
    let hash = Multihash::wrap(SHA2_256_TRUNC254_PADDED, &ZERO_COMMD_HASH[i])
        .map_err(|_| actor_error!(assertion_failed, "static commd payload invalid"))?;
    Ok(Cid::new_v1(FIL_COMMITMENT_UNSEALED, hash))
}
