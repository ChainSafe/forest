// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use derive_more::{From, Into};
// re-exports the trait
pub use get_size2::GetSize;
use num_bigint::BigInt;

#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, From, Into)]
pub struct CidWrapper(pub Cid);
impl GetSize for CidWrapper {}

macro_rules! impl_vec_alike_heap_size_with_fn_helper {
    ($name:ident, $t:ty, $get_stack_size: expr, $get_heap_size: expr) => {{
        let mut heap_size = 0;
        // use `____v` to avoid naming conflict
        for ____v in $name.iter() {
            heap_size += $get_stack_size() + $get_heap_size(____v);
        }
        let additional = usize::from($name.capacity()) - usize::from($name.len());
        heap_size += additional * $get_stack_size();
        heap_size
    }};
}

macro_rules! impl_vec_alike_heap_size_helper {
    ($name:ident, $t:ty) => {
        impl_vec_alike_heap_size_with_fn_helper!(
            $name,
            $t,
            <$t>::get_stack_size,
            GetSize::get_heap_size
        )
    };
}

pub fn vec_with_stack_only_item_heap_size_helper<T>(v: &Vec<T>) -> usize {
    v.capacity() * std::mem::size_of::<T>()
}

pub fn vec_heap_size_helper<T: GetSize>(v: &Vec<T>) -> usize {
    impl_vec_alike_heap_size_helper!(v, T)
}

pub fn vec_heap_size_with_fn_helper<T>(v: &Vec<T>, get_heap_size: impl Fn(&T) -> usize) -> usize {
    impl_vec_alike_heap_size_with_fn_helper!(v, T, std::mem::size_of::<T>, get_heap_size)
}

pub fn nunny_vec_heap_size_helper<T: GetSize>(v: &nunny::Vec<T>) -> usize {
    impl_vec_alike_heap_size_helper!(v, T)
}

pub fn nunny_vec_heap_size_with_fn_helper<T>(
    v: &nunny::Vec<T>,
    get_heap_size: impl Fn(&T) -> usize,
) -> usize {
    impl_vec_alike_heap_size_with_fn_helper!(v, T, std::mem::size_of::<T>, get_heap_size)
}

// This is a rough estimation. Use `b.allocation_size()`
// once https://github.com/rust-num/num-bigint/pull/333 is accepted and released.
pub fn big_int_heap_size_helper(b: &BigInt) -> usize {
    b.bits().div_ceil(8) as usize
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
        assert_eq!(
            nunny_vec_heap_size_helper(&keys),
            CidWrapper::get_stack_size() * usize::from(keys.capacity())
        );
    }
}
