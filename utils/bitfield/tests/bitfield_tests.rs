// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use bitfield::*;
use bitvec::*;
use fnv::FnvHashSet;
use rand::{Rng, SeedableRng};
use rand_xorshift::XorShiftRng;

fn gen_random_index_set(range: u64, seed: u8) -> Vec<u64> {
    let mut rng = XorShiftRng::from_seed([seed; 16]);

    (0..range).filter(|_| rng.gen::<bool>()).collect()
}

#[test]
fn bitfield_slice() {
    let vals = gen_random_index_set(10000, 2);

    let mut bf = BitField::new_from_set(&vals);

    let mut slice = bf.slice(600, 500).unwrap();
    let out_vals = slice.all(10000).unwrap();
    let expected_slice = &vals[600..1100];

    assert_eq!(out_vals[..500], expected_slice[..500]);
}

#[test]
fn bitfield_slice_small() {
    let mut bf = BitField::from(bitvec![Lsb0, u8; 0, 1, 0, 0, 1, 0, 0, 1, 0, 1, 1, 1, 0, 0]);
    let mut slice = bf.slice(1, 3).unwrap();

    assert_eq!(slice.count().unwrap(), 3);
    assert_eq!(slice.all(10).unwrap(), &[4, 7, 9]);

    // Test all combinations
    let vals = [1, 5, 6, 7, 10, 11, 12, 15];

    let test_permutations = |start, count: usize| {
        let mut bf = BitField::new_from_set(&vals);
        let mut sl = bf.slice(start as u64, count as u64).unwrap();
        let exp = &vals[start..start + count];
        let out = sl.all(10000).unwrap();
        assert_eq!(out, exp);
    };

    for i in 0..vals.len() {
        for j in 0..vals.len() - i {
            println!("{}, {}", i, j);
            test_permutations(i, j);
        }
    }
}

fn set_up_test_bitfields() -> (Vec<u64>, Vec<u64>, BitField, BitField) {
    let a = gen_random_index_set(100, 1);
    let b = gen_random_index_set(100, 2);

    let bf_a = BitField::new_from_set(&a);
    let bf_b = BitField::new_from_set(&b);

    (a, b, bf_a, bf_b)
}

#[test]
fn bitfield_union() {
    let (a, b, bf_a, bf_b) = set_up_test_bitfields();

    let mut expected: FnvHashSet<u64> = a.iter().copied().collect();
    expected.extend(b);

    let mut merged = bf_a.merge(&bf_b).unwrap();

    assert_eq!(expected, merged.all_set(100).unwrap());
}

#[test]
fn bitfield_intersection() {
    let (a, b, bf_a, bf_b) = set_up_test_bitfields();

    let hs_a: FnvHashSet<u64> = a.into_iter().collect();
    let hs_b: FnvHashSet<u64> = b.into_iter().collect();
    let expected: FnvHashSet<u64> = hs_a.intersection(&hs_b).copied().collect();

    let mut merged = bf_a.intersect(&bf_b).unwrap();

    assert_eq!(expected, merged.all_set(100).unwrap());
}

#[test]
fn bitfield_subtraction() {
    let (a, b, bf_a, bf_b) = set_up_test_bitfields();

    let mut expected: FnvHashSet<u64> = a.into_iter().collect();
    for i in b.iter() {
        expected.remove(i);
    }

    let mut merged = bf_a.subtract(&bf_b).unwrap();
    assert_eq!(expected, merged.all_set(100).unwrap());
}

// Ported test from go impl (specs-actors)
#[test]
fn subtract_more() {
    let have = BitField::new_from_set(&[5, 6, 8, 10, 11, 13, 14, 17]);
    let s1 = BitField::new_from_set(&[5, 6]).subtract(&have).unwrap();
    let s2 = BitField::new_from_set(&[8, 10]).subtract(&have).unwrap();
    let s3 = BitField::new_from_set(&[11, 13]).subtract(&have).unwrap();
    let s4 = BitField::new_from_set(&[14, 17]).subtract(&have).unwrap();

    let mut u = BitField::union(&[s1, s2, s3, s4]).unwrap();
    assert_eq!(u.count().unwrap(), 0);
}

#[test]
fn contains_any() {
    assert_eq!(
        BitField::new_from_set(&[0, 4])
            .contains_any(&mut BitField::new_from_set(&[1, 3, 5]))
            .unwrap(),
        false
    );

    assert_eq!(
        BitField::new_from_set(&[0, 2, 5, 6])
            .contains_any(&mut BitField::new_from_set(&[1, 3, 5]))
            .unwrap(),
        true
    );
}

#[test]
fn contains_all() {
    assert_eq!(
        BitField::new_from_set(&[0, 2, 4])
            .contains_all(&mut BitField::new_from_set(&[0, 2, 4, 5]))
            .unwrap(),
        false
    );

    assert_eq!(
        BitField::new_from_set(&[0, 2, 4, 5])
            .contains_all(&mut BitField::new_from_set(&[0, 2, 4]))
            .unwrap(),
        true
    );

    assert_eq!(
        BitField::new_from_set(&[1, 2, 3])
            .contains_any(&mut BitField::new_from_set(&[1, 2, 3]))
            .unwrap(),
        true
    );
}

#[test]
fn bit_ops() {
    let mut a = BitField::new_from_set(&[1, 2, 3]) & BitField::new_from_set(&[1, 3, 4]);
    assert_eq!(a.all(5).unwrap(), &[1, 3]);

    let mut a = BitField::new_from_set(&[1, 2, 3]);
    a &= BitField::new_from_set(&[1, 3, 4]);
    assert_eq!(a.all(5).unwrap(), &[1, 3]);

    let mut a = BitField::new_from_set(&[1, 2, 3]) | BitField::new_from_set(&[1, 3, 4]);
    assert_eq!(a.all(5).unwrap(), &[1, 2, 3, 4]);

    let mut a = BitField::new_from_set(&[1, 2, 3]);
    a |= BitField::new_from_set(&[1, 3, 4]);
    assert_eq!(a.all(5).unwrap(), &[1, 2, 3, 4]);

    assert_eq!(
        (!BitField::from(bitvec![Lsb0, u8; 1, 0, 1, 0]))
            .all(5)
            .unwrap(),
        &[1, 3]
    );
}
