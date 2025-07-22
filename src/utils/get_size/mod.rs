// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use derive_more::{From, Into};
use get_size2::GetSize;

pub fn vec_alike_get_size<V, T>(slice: &V) -> usize
where
    V: AsRef<[T]>,
{
    std::mem::size_of_val(slice.as_ref())
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, From, Into)]
pub struct CidWrapper(pub Cid);
impl GetSize for CidWrapper {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::multihash::MultihashCode;
    use fvm_ipld_encoding::DAG_CBOR;
    use multihash_derive::MultihashDigest as _;

    #[test]
    fn test_cid() {
        let cid = Cid::new_v1(DAG_CBOR, MultihashCode::Blake2b256.digest(&[0, 1, 2, 3]));
        let wrapper = CidWrapper(cid);
        assert_eq!(std::mem::size_of_val(&cid), wrapper.get_size());
    }
}
