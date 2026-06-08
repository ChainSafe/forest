// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use derive_more::{From, Into};
// re-exports the trait
pub use get_size2::GetSize;
use num_bigint::BigInt;

#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, From, Into)]
pub struct CidWrapper(pub Cid);
impl GetSize for CidWrapper {}

impl quick_cache::Equivalent<CidWrapper> for Cid {
    fn equivalent(&self, other: &CidWrapper) -> bool {
        self == &other.0
    }
}

macro_rules! impl_vec_alike_heap_size_with_fn_helper {
    ($name:ident, $tracker:ident, $t:ty, $get_stack_size: expr, $get_heap_size_with_tracker: expr) => {{
        let mut heap_size = 0;
        let mut tr = $tracker;
        // use `____v` to avoid naming conflict
        for ____v in $name.iter() {
            let (_s, _tr) = $get_heap_size_with_tracker(____v, tr);
            heap_size += $get_stack_size() + _s;
            tr = _tr;
        }
        let additional = usize::from($name.capacity()) - usize::from($name.len());
        heap_size += additional * $get_stack_size();
        (heap_size, tr)
    }};
}

macro_rules! impl_vec_alike_heap_size_helper {
    ($name:ident, $tracker:ident, $t:ty) => {
        impl_vec_alike_heap_size_with_fn_helper!(
            $name,
            $tracker,
            $t,
            <$t>::get_stack_size,
            GetSize::get_heap_size_with_tracker
        )
    };
}

pub fn vec_heap_size_with_fn_helper<T, Tr: get_size2::GetSizeTracker>(
    v: &Vec<T>,
    tracker: Tr,
    get_heap_size_with_tracker: impl Fn(&T, Tr) -> (usize, Tr),
) -> (usize, Tr) {
    impl_vec_alike_heap_size_with_fn_helper!(
        v,
        tracker,
        T,
        std::mem::size_of::<T>,
        get_heap_size_with_tracker
    )
}

pub fn nunny_vec_heap_size_helper<T: GetSize, Tr: get_size2::GetSizeTracker>(
    v: &nunny::Vec<T>,
    tracker: Tr,
) -> (usize, Tr) {
    impl_vec_alike_heap_size_helper!(v, tracker, T)
}

// This is a rough estimation. Use `b.allocation_size()`
// once https://github.com/rust-num/num-bigint/pull/333 is accepted and released.
pub fn big_int_heap_size_helper(b: &BigInt) -> usize {
    b.bits().div_ceil(8) as usize
}

pub fn raw_bytes_heap_size_helper(b: &fvm_ipld_encoding::RawBytes) -> usize {
    // Note: this is a cheap but inaccurate estimation,
    // the correct implementation should be `Vec<u8>.from(b.clone()).get_heap_size()`,
    // or `b.bytes.get_heap_size()` if `bytes` is made public.
    b.bytes().get_heap_size()
}

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

    #[test]
    fn test_heap_size_helper() {
        let keys: nunny::Vec<CidWrapper> = nunny::vec![Cid::default().into(); 3];
        // It's likely > 3 (4 on my laptop)
        println!("keys.capacity() = {}", keys.capacity());
        let tracker = get_size2::StandardTracker::new();
        assert_eq!(
            nunny_vec_heap_size_helper(&keys, tracker).0,
            CidWrapper::get_stack_size() * usize::from(keys.capacity())
        );
    }

    #[test]
    fn test_derive_macro() {
        #[derive(GetSize)]
        struct A {
            #[get_size(ignore)]
            _cid: Cid,
        }

        #[derive(GetSize)]
        struct B {
            #[get_size(size = 0)]
            _cid: Cid,
        }

        #[derive(GetSize)]
        struct C {
            #[get_size(size = 8)]
            _cid: Cid,
        }

        let _cid = Cid::default();
        let a = vec![A { _cid }];
        assert_eq!(
            a.get_heap_size(),
            std::mem::size_of_val(&_cid) * a.capacity()
        );

        let b = vec![B { _cid }];
        assert_eq!(
            b.get_heap_size(),
            std::mem::size_of_val(&_cid) * b.capacity()
        );

        let c = vec![C { _cid }];
        assert_eq!(
            c.get_heap_size(),
            (std::mem::size_of_val(&_cid) + 8) * c.capacity()
        );
    }
}
