// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::{multihash, Cid, Codec};
use filecoin_proofs_api::Commitment;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

// Multicodec index that identifies a multihash type for Filecoin
#[derive(PartialEq, Eq, Copy, Clone, FromPrimitive, Debug, Hash)]
#[repr(u64)]
pub enum FilecoinMultihashCode {
    /// v1 hashing algorithm used in constructing merkleproofs of unsealed data
    UnsealedV1 = 0xfc1,
    /// v1 hashing algorithm used in constructing merkleproofs of sealed replicated data
    SealedV1 = 0xfc2,
    /// Reserved3 is reserved for future use
    Reserved3 = 0xfc3,
    /// Reserved4 is reserved for future use
    Reserved4 = 0xfc4,
    /// Reserved5 is reserved for future use
    Reserved5 = 0xfc5,
    /// Reserved6 is reserved for future use
    Reserved6 = 0xfc6,
    /// Reserved7 is reserved for future use
    Reserved7 = 0xfc7,
    /// Reserved8 is reserved for future use
    Reserved8 = 0xfc8,
    /// Reserved9 is reserved for future use
    Reserved9 = 0xfc9,
    /// Reserved10 is reserved for future use
    Reserved10 = 0xfca,
}

/// Converts a raw commitment hash to a CID
/// by adding:
/// - serialization type of raw
/// - the given filecoin hash type
pub fn commitment_to_cid(commitment: Commitment, code: FilecoinMultihashCode) -> Cid {
    let mh = multihash::wrap(multihash::Code::Custom(code as u64), &commitment);

    Cid::new_v1(Codec::Raw, mh)
}

/// cid_to_commitment extracts the raw data commitment from a CID
/// assuming that it has the correct hashing function and
/// serialization types
pub fn cid_to_commitment(c: Cid) -> Result<(Commitment, FilecoinMultihashCode), &'static str> {
    if c.codec != Codec::Raw {
        return Err("codec for all commitments is raw");
    }

    let code = match c.hash.algorithm() {
        multihash::Code::Custom(code) => {
            FromPrimitive::from_u64(code).ok_or("Invalid custom code")?
        }
        _ => return Err("Invalid Cid hash algorithm"),
    };

    let mut comm = Commitment::default();
    comm.copy_from_slice(c.hash.digest());

    Ok((comm, code))
}

/// data_commitment_v1_to_cid converts a raw data commitment to a CID
/// by adding:
/// - serialization type of raw
/// - hashing type of Filecoin unsealed hashing function v1 (0xfc2)
pub fn data_commitment_v1_to_cid(comm_d: Commitment) -> Cid {
    commitment_to_cid(comm_d, FilecoinMultihashCode::UnsealedV1)
}

/// cid_to_data_commitment_v1 extracts the raw data commitment from a CID
/// assuming that it has the correct hashing function and
/// serialization types
pub fn cid_to_data_commitment_v1(c: Cid) -> Result<Commitment, &'static str> {
    let (comm_d, code) = cid_to_commitment(c)?;

    if code != FilecoinMultihashCode::UnsealedV1 {
        return Err("incorrect hashing function for data commitment");
    }

    Ok(comm_d)
}

/// replica_commitment_v1_to_cid converts a raw data commitment to a CID
/// by adding:
/// - serialization type of raw
/// - hashing type of Filecoin sealed hashing function v1 (0xfc2)
pub fn replica_commitment_v1_to_cid(comm_r: Commitment) -> Cid {
    commitment_to_cid(comm_r, FilecoinMultihashCode::SealedV1)
}

/// cid_to_replica_commitment_v1 extracts the raw replica commitment from a CID
/// assuming that it has the correct hashing function and
/// serialization types
pub fn cid_to_replica_commitment_v1(c: Cid) -> Result<Commitment, &'static str> {
    let (comm_r, hash) = cid_to_commitment(c)?;

    if hash != FilecoinMultihashCode::SealedV1 {
        return Err("incorrect hashing function for data commitment");
    }

    Ok(comm_r)
}

/// piece_commitment_v1_to_cid converts a comm_p to a CID
/// -- it is just a helper function that is equivalent to
/// data_commitment_v1_to_cid.
pub fn piece_commitment_v1_to_cid(comm_p: Commitment) -> Cid {
    data_commitment_v1_to_cid(comm_p)
}

/// cid_to_piece_commitment_v1 converts a CID to a comm_p
/// -- it is just a helper function that is equivalent to
/// cid_to_data_commitment_v1.
pub fn cid_to_piece_commitment_v1(c: Cid) -> Result<Commitment, &'static str> {
    cid_to_data_commitment_v1(c)
}
