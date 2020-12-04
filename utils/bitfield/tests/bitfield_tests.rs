// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::AHashSet;
use forest_bitfield::{bitfield, BitField};
use rand::{Rng, SeedableRng};
use rand_xorshift::XorShiftRng;
use std::iter::FromIterator;

fn random_indices(range: usize, seed: u64) -> Vec<usize> {
    let mut rng = XorShiftRng::seed_from_u64(seed);
    (0..range).filter(|_| rng.gen::<bool>()).collect()
}

#[test]
fn bitfield_slice() {
    let vals = random_indices(10000, 2);
    let bf: BitField = vals.iter().copied().collect();

    let slice = bf.slice(600, 500).unwrap();
    let out_vals: Vec<_> = slice.iter().collect();
    let expected_slice = &vals[600..1100];

    assert_eq!(out_vals[..500], expected_slice[..500]);
}

#[test]
fn bitfield_slice_small() {
    let bf: BitField = bitfield![0, 1, 0, 0, 1, 0, 0, 1, 0, 1, 1, 1, 0, 0];
    let slice = bf.slice(1, 3).unwrap();

    assert_eq!(slice.len(), 3);
    assert_eq!(slice.iter().collect::<Vec<_>>(), &[4, 7, 9]);

    // Test all combinations
    let vals = [1, 5, 6, 7, 10, 11, 12, 15];

    let test_permutations = |start, count: usize| {
        let bf: BitField = vals.iter().copied().collect();
        let sl = bf.slice(start, count).unwrap();
        let exp = &vals[start..start + count];
        let out: Vec<_> = sl.iter().collect();
        assert_eq!(out, exp);
    };

    for i in 0..vals.len() {
        for j in 0..vals.len() - i {
            test_permutations(i, j);
        }
    }
}

fn set_up_test_bitfields() -> (Vec<usize>, Vec<usize>, BitField, BitField) {
    let a = random_indices(100, 1);
    let b = random_indices(100, 2);

    let bf_a: BitField = a.iter().copied().collect();
    let bf_b: BitField = b.iter().copied().collect();

    (a, b, bf_a, bf_b)
}

#[test]
fn bitfield_union() {
    let (a, b, bf_a, bf_b) = set_up_test_bitfields();

    let mut expected: AHashSet<_> = a.iter().copied().collect();
    expected.extend(b);

    let merged = &bf_a | &bf_b;
    assert_eq!(expected, merged.iter().collect());
}

#[test]
fn bitfield_intersection() {
    let (a, b, bf_a, bf_b) = set_up_test_bitfields();

    let hs_a: AHashSet<_> = a.into_iter().collect();
    let hs_b: AHashSet<_> = b.into_iter().collect();
    let expected: AHashSet<_> = hs_a.intersection(&hs_b).copied().collect();

    let merged = &bf_a & &bf_b;
    assert_eq!(expected, merged.iter().collect());
}

#[test]
fn bitfield_difference() {
    let (a, b, bf_a, bf_b) = set_up_test_bitfields();

    let mut expected: AHashSet<_> = a.into_iter().collect();
    for i in b.iter() {
        expected.remove(i);
    }

    let merged = &bf_a - &bf_b;
    assert_eq!(expected, merged.iter().collect());
}

// Ported test from go impl (specs-actors)
#[test]
fn subtract_more() {
    let have = BitField::from_iter(vec![5, 6, 8, 10, 11, 13, 14, 17]);
    let s1 = &BitField::from_iter(vec![5, 6]) - &have;
    let s2 = &BitField::from_iter(vec![8, 10]) - &have;
    let s3 = &BitField::from_iter(vec![11, 13]) - &have;
    let s4 = &BitField::from_iter(vec![14, 17]) - &have;

    let u = BitField::union(&[s1, s2, s3, s4]);
    assert_eq!(u.len(), 0);
}

#[test]
fn contains_any() {
    assert_eq!(
        BitField::from_iter(vec![0, 4]).contains_any(&BitField::from_iter(vec![1, 3, 5])),
        false
    );

    assert_eq!(
        BitField::from_iter(vec![0, 2, 5, 6]).contains_any(&BitField::from_iter(vec![1, 3, 5])),
        true
    );

    assert_eq!(
        BitField::from_iter(vec![1, 2, 3]).contains_any(&BitField::from_iter(vec![1, 2, 3])),
        true
    );
}

#[test]
fn contains_all() {
    assert_eq!(
        BitField::from_iter(vec![0, 2, 4]).contains_all(&BitField::from_iter(vec![0, 2, 4, 5])),
        false
    );

    assert_eq!(
        BitField::from_iter(vec![0, 2, 4, 5]).contains_all(&BitField::from_iter(vec![0, 2, 4])),
        true
    );

    assert_eq!(
        BitField::from_iter(vec![1, 2, 3]).contains_all(&BitField::from_iter(vec![1, 2, 3])),
        true
    );
}

#[test]
fn bit_ops() {
    let a = &BitField::from_iter(vec![1, 2, 3]) & &BitField::from_iter(vec![1, 3, 4]);
    assert_eq!(a.iter().collect::<Vec<_>>(), &[1, 3]);

    let mut a = BitField::from_iter(vec![1, 2, 3]);
    a &= &BitField::from_iter(vec![1, 3, 4]);
    assert_eq!(a.iter().collect::<Vec<_>>(), &[1, 3]);

    let a = &BitField::from_iter(vec![1, 2, 3]) | &BitField::from_iter(vec![1, 3, 4]);
    assert_eq!(a.iter().collect::<Vec<_>>(), &[1, 2, 3, 4]);

    let mut a = BitField::from_iter(vec![1, 2, 3]);
    a |= &BitField::from_iter(vec![1, 3, 4]);
    assert_eq!(a.iter().collect::<Vec<_>>(), &[1, 2, 3, 4]);
}

#[test]
fn ranges() {
    let mut bit_field = bitfield![0, 0, 1, 1, 1, 0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0];

    assert_eq!(bit_field.ranges().count(), 4);
    bit_field.set(5);
    assert_eq!(bit_field.ranges().count(), 3);
    bit_field.unset(4);
    assert_eq!(bit_field.ranges().count(), 4);
    bit_field.unset(2);
    assert_eq!(bit_field.ranges().count(), 4);
}

#[test]
fn serialize_node_symmetric() {
    let bit_field = bitfield![0, 1, 0, 1, 1, 1, 1, 1, 1];
    let cbor_bz = encoding::to_vec(&bit_field).unwrap();
    let deserialized: BitField = encoding::from_slice(&cbor_bz).unwrap();
    assert_eq!(deserialized.len(), 7);
    assert_eq!(deserialized, bit_field);
}

#[test]
// ported test from specs-actors `bitfield_test.go` with added vector
fn bit_vec_unset_vector() {
    let mut bf = BitField::new();
    bf.set(1);
    bf.set(2);
    bf.set(3);
    bf.set(4);
    bf.set(5);

    bf.unset(3);

    assert_eq!(bf.get(3), false);
    assert_eq!(bf.len(), 4);

    // Test cbor marshal and unmarshal
    let cbor_bz = encoding::to_vec(&bf).unwrap();
    assert_eq!(&cbor_bz, &[0x42, 0xa8, 0x54]);

    let deserialized: BitField = encoding::from_slice(&cbor_bz).unwrap();
    assert_eq!(deserialized.len(), 4);
    assert_eq!(bf.get(3), false);
}

#[test]
fn padding() {
    // bits: 0 1 0 1
    // rle+: 0 0 0 1 1 1 1
    // when deserialized it will have an extra 0 at the end for padding,
    // which is not part of a block prefix

    let mut bf = BitField::new();
    bf.set(1);
    bf.set(3);

    let cbor = encoding::to_vec(&bf).unwrap();
    let deserialized: BitField = encoding::from_slice(&cbor).unwrap();
    assert_eq!(deserialized, bf);
}
