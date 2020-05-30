use bitfield::*;
use bitvec::*;
use fnv::FnvHashSet;
use rand::{Rng, SeedableRng};
use rand_xorshift::XorShiftRng;

fn gen_random_index_set(range: u64, seed: u8) -> Vec<u64> {
    let mut rng = XorShiftRng::from_seed([seed; 16]);

    let mut ret = Vec::new();
    for i in 0..range {
        if rng.gen::<bool>() {
            ret.push(i);
        }
    }
    ret
}

#[test]
fn bitfield_slice() {
    let vals = gen_random_index_set(10000, 2);

    let mut bf = BitField::new_from_set(&vals);

    let mut slice = bf.to_slice(600, 500).unwrap();
    let out_vals = slice.to_all(10000).unwrap();
    let expected_slice = &vals[600..1100];

    assert_eq!(out_vals[..500], expected_slice[..500]);
}

#[test]
fn bitfield_slice_small() {
    let mut bf = BitField::from(bitvec![Lsb0, u8; 0, 1, 0, 0, 1, 0, 0, 1, 0, 1, 1, 1, 0, 0]);
    let mut slice = bf.to_slice(1, 3).unwrap();

    assert_eq!(slice.count().unwrap(), 3);
    assert_eq!(slice.to_all(10).unwrap(), &[4, 7, 9]);

    // Test all combinations
    let vals = [1, 5, 6, 7, 10, 11, 12, 15];

    let test_permutations = |start, count: usize| {
        let mut bf = BitField::new_from_set(&vals);
        let mut sl = bf.to_slice(start as u64, count as u64).unwrap();
        let exp = &vals[start..start + count];
        let out = sl.to_all(10000).unwrap();
        assert_eq!(out, exp);
    };

    for i in 0..vals.len() {
        for j in 0..vals.len() - i {
            println!("{}, {}", i, j);
            test_permutations(i, j);
        }
    }
}

#[test]
fn bitfield_union() {
    let a = gen_random_index_set(100, 1);
    let b = gen_random_index_set(100, 2);

    let bf_a = BitField::new_from_set(&a);
    let bf_b = BitField::new_from_set(&b);

    let mut expected: FnvHashSet<u64> = a.iter().copied().collect();
    expected.extend(b);

    let mut merged = bf_a.merge(bf_b).unwrap();

    assert_eq!(expected, merged.to_all_set(100).unwrap());
}
