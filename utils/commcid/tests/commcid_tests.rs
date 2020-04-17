// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::{multihash, Cid, Codec};
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

    let cid = data_commitment_v1_to_cid(comm);

    assert_eq!(cid.codec, Codec::Raw);
    assert_eq!(
        cid.hash.algorithm(),
        multihash::Code::Custom(FilecoinMultihashCode::UnsealedV1 as u64)
    );
    assert_eq!(cid.hash.digest(), comm);
}

#[test]
fn cid_to_comm_d() {
    let comm = rand_comm();

    // Correct hash format
    let mh = multihash::wrap(
        multihash::Code::Custom(FilecoinMultihashCode::UnsealedV1 as u64),
        &comm,
    );
    let c = Cid::new_v1(Codec::Raw, mh.clone());
    let decoded = cid_to_data_commitment_v1(c).unwrap();
    assert_eq!(decoded, comm);

    // Should fail with incorrect codec
    let c = Cid::new_v1(Codec::DagCBOR, mh);
    assert!(cid_to_data_commitment_v1(c).is_err());

    // Incorrect hash format
    let mh = multihash::Sha2_256::digest(&comm);
    let c = Cid::new_v1(Codec::Raw, mh);
    assert!(cid_to_data_commitment_v1(c).is_err());
}

#[test]
fn comm_r_to_cid() {
    let comm = rand_comm();

    let cid = replica_commitment_v1_to_cid(comm);

    assert_eq!(cid.codec, Codec::Raw);
    assert_eq!(
        cid.hash.algorithm(),
        multihash::Code::Custom(FilecoinMultihashCode::SealedV1 as u64)
    );
    assert_eq!(cid.hash.digest(), comm);
}

#[test]
fn cid_to_comm_r() {
    let comm = rand_comm();

    // Correct hash format
    let mh = multihash::wrap(
        multihash::Code::Custom(FilecoinMultihashCode::SealedV1 as u64),
        &comm,
    );
    let c = Cid::new_v1(Codec::Raw, mh.clone());
    let decoded = cid_to_replica_commitment_v1(c).unwrap();
    assert_eq!(decoded, comm);

    // Should fail with incorrect codec
    let c = Cid::new_v1(Codec::DagCBOR, mh);
    assert!(cid_to_replica_commitment_v1(c).is_err());

    // Incorrect hash format
    let mh = multihash::Sha2_256::digest(&comm);
    let c = Cid::new_v1(Codec::Raw, mh);
    assert!(cid_to_replica_commitment_v1(c).is_err());
}

#[test]
fn symmetric_conversion() {
    use FilecoinMultihashCode::*;
    let comm = rand_comm();

    // data
    let cid = data_commitment_v1_to_cid(comm);
    assert_eq!(cid_to_commitment(cid).unwrap(), (comm, UnsealedV1));

    // replica
    let cid = replica_commitment_v1_to_cid(comm);
    assert_eq!(cid_to_commitment(cid).unwrap(), (comm, SealedV1));

    // data
    let cid = data_commitment_v1_to_cid(comm);
    assert_eq!(cid_to_commitment(cid).unwrap(), (comm, UnsealedV1));
}
