// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::BalanceTable;
use address::Address;
use vm::TokenAmount;

// Ported test from specs-actors
#[test]
fn add_create() {
    let addr = Address::new_id(100).unwrap();
    let store = db::MemoryDB::default();
    let mut bt = BalanceTable::new_empty(&store);

    assert_eq!(bt.has(&addr), Ok(false));

    bt.add_create(&addr, TokenAmount::new(10)).unwrap();
    assert_eq!(bt.get(&addr), Ok(TokenAmount::new(10)));

    bt.add_create(&addr, TokenAmount::new(20)).unwrap();
    assert_eq!(bt.get(&addr), Ok(TokenAmount::new(30)));
}

// Ported test from specs-actors
#[test]
fn total() {
    let addr1 = Address::new_id(100).unwrap();
    let addr2 = Address::new_id(101).unwrap();
    let store = db::MemoryDB::default();
    let mut bt = BalanceTable::new_empty(&store);

    assert_eq!(bt.total(), Ok(TokenAmount::new(0)));

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
        bt.add_create(t.addr, TokenAmount::new(t.amount)).unwrap();

        assert_eq!(bt.total(), Ok(TokenAmount::new(t.total)));
    }
}

#[test]
fn balance_subtracts() {
    let addr = Address::new_id(100).unwrap();
    let store = db::MemoryDB::default();
    let mut bt = BalanceTable::new_empty(&store);

    bt.set(&addr, TokenAmount::new(80)).unwrap();
    assert_eq!(bt.get(&addr), Ok(TokenAmount::new(80)));
    // Test subtracting past minimum only subtracts correct amount
    assert_eq!(
        bt.subtract_with_minimum(&addr, &TokenAmount::new(20), &TokenAmount::new(70)),
        Ok(TokenAmount::new(10))
    );
    assert_eq!(bt.get(&addr), Ok(TokenAmount::new(70)));

    // Test subtracting to limit
    assert_eq!(
        bt.subtract_with_minimum(&addr, &TokenAmount::new(10), &TokenAmount::new(60)),
        Ok(TokenAmount::new(10))
    );
    assert_eq!(bt.get(&addr), Ok(TokenAmount::new(60)));

    // Test must subtract success
    bt.must_subtract(&addr, &TokenAmount::new(10)).unwrap();
    assert_eq!(bt.get(&addr), Ok(TokenAmount::new(50)));

    // Test subtracting more than available
    assert!(bt.must_subtract(&addr, &TokenAmount::new(100)).is_err());
}

#[test]
fn remove() {
    let addr = Address::new_id(100).unwrap();
    let store = db::MemoryDB::default();
    let mut bt = BalanceTable::new_empty(&store);

    bt.set(&addr, TokenAmount::new(1)).unwrap();
    assert_eq!(bt.get(&addr), Ok(TokenAmount::new(1)));
    bt.remove(&addr).unwrap();
    assert!(bt.get(&addr).is_err());
}
