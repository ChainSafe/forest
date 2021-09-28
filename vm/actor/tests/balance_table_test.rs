// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use forest_actor::BalanceTable;
use vm::TokenAmount;

// Ported test from specs-actors
#[test]
fn total() {
    let addr1 = Address::new_id(100);
    let addr2 = Address::new_id(101);
    let store = db::MemoryDB::default();
    let mut bt = BalanceTable::new(&store);

    assert_eq!(bt.total().unwrap(), TokenAmount::from(0u8));

    struct TotalTestCase<'a> {
        amount: u64,
        addr: &'a Address,
        total: u64,
    }
    let test_vectors = [
        TotalTestCase {
            amount: 10,
            addr: &addr1,
            total: 10,
        },
        TotalTestCase {
            amount: 20,
            addr: &addr1,
            total: 30,
        },
        TotalTestCase {
            amount: 40,
            addr: &addr2,
            total: 70,
        },
        TotalTestCase {
            amount: 50,
            addr: &addr2,
            total: 120,
        },
    ];

    for t in test_vectors.iter() {
        bt.add(t.addr, &TokenAmount::from(t.amount)).unwrap();

        assert_eq!(bt.total().unwrap(), TokenAmount::from(t.total));
    }
}

#[test]
fn balance_subtracts() {
    let addr = Address::new_id(100);
    let store = db::MemoryDB::default();
    let mut bt = BalanceTable::new(&store);

    bt.add(&addr, &TokenAmount::from(80u8)).unwrap();
    assert_eq!(bt.get(&addr).unwrap(), TokenAmount::from(80u8));
    // Test subtracting past minimum only subtracts correct amount
    assert_eq!(
        bt.subtract_with_minimum(&addr, &TokenAmount::from(20u8), &TokenAmount::from(70u8))
            .unwrap(),
        TokenAmount::from(10u8)
    );
    assert_eq!(bt.get(&addr).unwrap(), TokenAmount::from(70u8));

    // Test subtracting to limit
    assert_eq!(
        bt.subtract_with_minimum(&addr, &TokenAmount::from(10u8), &TokenAmount::from(60u8))
            .unwrap(),
        TokenAmount::from(10u8)
    );
    assert_eq!(bt.get(&addr).unwrap(), TokenAmount::from(60u8));

    // Test must subtract success
    bt.must_subtract(&addr, &TokenAmount::from(10u8)).unwrap();
    assert_eq!(bt.get(&addr).unwrap(), TokenAmount::from(50u8));

    // Test subtracting more than available
    assert!(bt.must_subtract(&addr, &TokenAmount::from(100u8)).is_err());
}
