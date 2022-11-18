use std::mem;

use cid::multihash::Multihash;
use cid::Cid;
use fvm_ipld_encoding::DAG_CBOR;
use fvm_shared::crypto::hash::SupportedHashes;

const fn const_unwrap<T: Copy, E>(r: Result<T, E>) -> T {
    let v = match r {
        Ok(r) => r,
        Err(_) => panic!(),
    };
    mem::forget(r);
    v
}

// 45b0cfc220ceec5b7c1c62c4d4193d38e4eba48e8815729ce75f9c0ab0e4c1c0
const EMPTY_ARR_HASH_DIGEST: &[u8] = &[
    0x45, 0xb0, 0xcf, 0xc2, 0x20, 0xce, 0xec, 0x5b, 0x7c, 0x1c, 0x62, 0xc4, 0xd4, 0x19, 0x3d, 0x38,
    0xe4, 0xeb, 0xa4, 0x8e, 0x88, 0x15, 0x72, 0x9c, 0xe7, 0x5f, 0x9c, 0x0a, 0xb0, 0xe4, 0xc1, 0xc0,
];

// bafy2bzacebc3bt6cedhoyw34drrmjvazhu4oj25er2ebk4u445pzycvq4ta4a
pub const EMPTY_ARR_CID: Cid = Cid::new_v1(
    DAG_CBOR,
    const_unwrap(Multihash::wrap(
        SupportedHashes::Blake2b256 as u64,
        EMPTY_ARR_HASH_DIGEST,
    )),
);

#[test]
fn test_empty_arr_cid() {
    use cid::multihash::{Code, MultihashDigest};
    use fvm_ipld_encoding::to_vec;

    let empty = to_vec::<[(); 0]>(&[]).unwrap();
    let expected = Cid::new_v1(DAG_CBOR, Code::Blake2b256.digest(&empty));
    assert_eq!(EMPTY_ARR_CID, expected);
}
