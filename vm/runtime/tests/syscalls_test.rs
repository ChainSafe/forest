// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::{multihash, Cid, Codec};
use commcid::FilecoinMultihashCode;
use interpreter::DefaultSyscalls;
use runtime::*;
use vm::{RegisteredProof, SealVerifyInfo};

#[test]
fn verify_seal_test() {
    let db = db::MemoryDB::default();
    let sys = DefaultSyscalls::new(&db);
    let mut vi = SealVerifyInfo::default();

    let data: &[u8; 32] = &[2; 32];
    let mh_sealed = multihash::wrap(
        multihash::Code::Custom(FilecoinMultihashCode::SealedV1 as u64),
        data,
    );
    let mh_unsealed = multihash::wrap(
        multihash::Code::Custom(FilecoinMultihashCode::UnsealedV1 as u64),
        data,
    );

    vi.on_chain.sealed_cid = Cid::new_v1(Codec::Raw, mh_sealed);
    vi.unsealed_cid = Cid::new_v1(Codec::Raw, mh_unsealed);
    vi.on_chain.registered_proof = RegisteredProof::StackedDRG2KiBSeal;
    // TODO currently captures an error resulting from rust-fil-proofs; need to revisit
    assert_eq!(sys.verify_seal(&vi).is_err(), true);
}
