// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::{
    multihash::{self, MultihashDigest},
    Cid,
    Code::{self, PoseidonBls12381A1Fc1, Sha2256Truncated256Padded},
    Codec,
};
use commcid::*;
use filecoin_proofs_api::Commitment;
use rand::thread_rng;
use rand::Rng;

fn rand_comm() -> Commitment {
    let mut rng = thread_rng();

    let mut comm = Commitment::default();
    for b in comm.iter_mut() {
        *b = rng.gen();
    }
    comm
}

#[test]
fn comm_d_to_cid() {
    let comm = rand_comm();

    let cid = data_commitment_v1_to_cid(&comm).unwrap();

    assert_eq!(cid.codec, Codec::FilCommitmentUnsealed);
    assert_eq!(cid.hash.code(), u64::from(Sha2256Truncated256Padded));
    assert_eq!(cid.hash.digest(), comm);
}

#[test]
fn cid_to_comm_d() {
    let comm = rand_comm();

    // Correct hash format
    let mh = multihash::Multihash::wrap(u64::from(Sha2256Truncated256Padded), &comm).unwrap();
    let c = Cid::new_v1(Codec::FilCommitmentUnsealed, mh.clone());
    let decoded = cid_to_data_commitment_v1(&c).unwrap();
    assert_eq!(decoded, comm);

    // Should fail with incorrect codec
    let c = Cid::new_v1(Codec::FilCommitmentSealed, mh);
    assert!(cid_to_data_commitment_v1(&c).is_err());

    // Incorrect hash format
    let mh = Code::Blake2b256.digest(&comm);
    let c = Cid::new_v1(Codec::FilCommitmentUnsealed, mh);
    assert!(cid_to_data_commitment_v1(&c).is_err());
}

#[test]
fn comm_r_to_cid() {
    let comm = rand_comm();

    let cid = replica_commitment_v1_to_cid(&comm).unwrap();

    assert_eq!(cid.codec, Codec::FilCommitmentSealed);
    assert_eq!(cid.hash.code(), u64::from(PoseidonBls12381A1Fc1));
    assert_eq!(cid.hash.digest(), comm);
}

#[test]
fn cid_to_comm_r() {
    let comm = rand_comm();

    // Correct hash format
    let mh = multihash::Multihash::wrap(PoseidonBls12381A1Fc1.into(), &comm).unwrap();
    let c = Cid::new_v1(Codec::FilCommitmentSealed, mh.clone());
    let decoded = cid_to_replica_commitment_v1(&c).unwrap();
    assert_eq!(decoded, comm);

    // Should fail with incorrect codec
    let c = Cid::new_v1(Codec::FilCommitmentUnsealed, mh);
    assert!(cid_to_replica_commitment_v1(&c).is_err());

    // Incorrect hash format
    let mh = Code::Blake2b256.digest(&comm);
    let c = Cid::new_v1(Codec::FilCommitmentSealed, mh);
    assert!(cid_to_replica_commitment_v1(&c).is_err());
}

#[test]
fn symmetric_conversion() {
    let comm = rand_comm();

    // data
    let cid = data_commitment_v1_to_cid(&comm).unwrap();
    assert_eq!(
        cid_to_commitment(&cid).unwrap(),
        (
            Codec::FilCommitmentUnsealed,
            u64::from(Sha2256Truncated256Padded),
            comm
        )
    );

    // replica
    let cid = replica_commitment_v1_to_cid(&comm).unwrap();
    assert_eq!(
        cid_to_commitment(&cid).unwrap(),
        (
            Codec::FilCommitmentSealed,
            u64::from(PoseidonBls12381A1Fc1),
            comm
        )
    );

    // piece
    let cid = piece_commitment_v1_to_cid(&comm).unwrap();
    assert_eq!(
        cid_to_commitment(&cid).unwrap(),
        (
            Codec::FilCommitmentUnsealed,
            u64::from(Sha2256Truncated256Padded),
            comm
        )
    );
}
