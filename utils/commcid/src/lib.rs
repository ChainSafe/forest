// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::{
    multihash::{self, Code},
    Cid, Codec,
};
use filecoin_proofs_api::Commitment;

/// Multihash code for Sha2 256 trunc254 padded used in data commitments.
pub const SHA2_256_TRUNC254_PADDED: Code = Code::Custom(0x1012);
/// Multihash code for Poseidon BLS replica commitments.
pub const POSEIDON_BLS12_381_A1_FC1: Code = Code::Custom(0xb401);

/// CommitmentToCID converts a raw commitment hash to a CID
/// by adding:
/// - the given filecoin codec type
/// - the given filecoin hash type
pub fn commitment_to_cid(
    mc: Codec,
    mh: Code,
    commitment: &Commitment,
) -> Result<Cid, &'static str> {
    validate_filecoin_cid_segments(mc, mh, commitment)?;

    let mh = multihash::wrap(mh, commitment);

    Ok(Cid::new_v1(mc, mh))
}

/// CIDToCommitment extracts the raw commitment bytes, the FilMultiCodec and
/// FilMultiHash from a CID, after validating that the codec and hash type are
/// consistent
pub fn cid_to_commitment(c: &Cid) -> Result<(Codec, Code, Commitment), &'static str> {
    validate_filecoin_cid_segments(c.codec, c.hash.algorithm(), c.hash.digest())?;

    let mut comm = Commitment::default();
    comm.copy_from_slice(c.hash.digest());

    Ok((c.codec, c.hash.algorithm(), comm))
}

/// DataCommitmentV1ToCID converts a raw data commitment to a CID
/// by adding:
/// - codec: cid.FilCommitmentUnsealed
/// - hash type: multihash.SHA2_256_TRUNC254_PADDED
pub fn data_commitment_v1_to_cid(comm_d: &Commitment) -> Result<Cid, &'static str> {
    commitment_to_cid(
        Codec::FilCommitmentUnsealed,
        SHA2_256_TRUNC254_PADDED,
        comm_d,
    )
}

/// cid_to_data_commitment_v1 extracts the raw data commitment from a CID
/// assuming that it has the correct hashing function and
/// serialization types
pub fn cid_to_data_commitment_v1(c: &Cid) -> Result<Commitment, &'static str> {
    let (codec, _, comm_d) = cid_to_commitment(c)?;

    if codec != Codec::FilCommitmentUnsealed {
        return Err("data commitment codec must be Unsealed");
    }

    Ok(comm_d)
}

/// ReplicaCommitmentV1ToCID converts a raw data commitment to a CID
/// by adding:
/// - codec: cid.FilCommitmentSealed
/// - hash type: multihash.POSEIDON_BLS12_381_A1_FC1
pub fn replica_commitment_v1_to_cid(comm_r: &Commitment) -> Result<Cid, &'static str> {
    commitment_to_cid(
        Codec::FilCommitmentSealed,
        POSEIDON_BLS12_381_A1_FC1,
        comm_r,
    )
}

/// cid_to_replica_commitment_v1 extracts the raw replica commitment from a CID
/// assuming that it has the correct hashing function and
/// serialization types
pub fn cid_to_replica_commitment_v1(c: &Cid) -> Result<Commitment, &'static str> {
    let (codec, _, comm_r) = cid_to_commitment(c)?;

    if codec != Codec::FilCommitmentSealed {
        return Err("data commitment codec must be Sealed");
    }

    Ok(comm_r)
}

/// ValidateFilecoinCidSegments returns an error if the provided CID parts
/// conflict with each other.
fn validate_filecoin_cid_segments(mc: Codec, mh: Code, comm_x: &[u8]) -> Result<(), &'static str> {
    match mc {
        Codec::FilCommitmentUnsealed => {
            if mh != SHA2_256_TRUNC254_PADDED {
                return Err("Incorrect hash function for unsealed commitment");
            }
        }
        Codec::FilCommitmentSealed => {
            if mh != POSEIDON_BLS12_381_A1_FC1 {
                return Err("Incorrect hash function for sealed commitment");
            }
        }
        _ => return Err("Invalid Codec, expected sealed or unsealed commitment codec"),
    }

    if comm_x.len() != 32 {
        Err("commitments must be 32 bytes long")
    } else {
        Ok(())
    }
}

/// piece_commitment_v1_to_cid converts a comm_p to a CID
/// -- it is just a helper function that is equivalent to
/// data_commitment_v1_to_cid.
pub fn piece_commitment_v1_to_cid(comm_p: &Commitment) -> Result<Cid, &'static str> {
    data_commitment_v1_to_cid(comm_p)
}

/// cid_to_piece_commitment_v1 converts a CID to a comm_p
/// -- it is just a helper function that is equivalent to
/// cid_to_data_commitment_v1.
pub fn cid_to_piece_commitment_v1(c: &Cid) -> Result<Commitment, &'static str> {
    cid_to_data_commitment_v1(c)
}
