// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
// Copyright 2021-2023 Protocol Labs
// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::ipld_amt::{diff, Amt, Change};
use anyhow::*;
use fvm_ipld_blockstore::MemoryBlockstore;
use itertools::Itertools;
use quickcheck::Arbitrary;
use quickcheck_macros::quickcheck;

/// Tests are ported from <https://github.com/filecoin-project/go-amt-ipld/blob/master/diff_test.go>

#[derive(Debug, Clone)]
struct BitWidth2to18(u32);

impl Arbitrary for BitWidth2to18 {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Self(*g.choose(&(2..=18).collect_vec()).unwrap())
    }
}

#[quickcheck]
fn test_simple_equals(BitWidth2to18(bit_width): BitWidth2to18) -> Result<()> {
    let prev_store = MemoryBlockstore::new();
    let curr_store = MemoryBlockstore::new();
    let mut a: Amt<String, _> = Amt::new_with_bit_width(prev_store, bit_width);
    let mut b: Amt<String, _> = Amt::new_with_bit_width(curr_store, bit_width);

    let changes = diff(&a, &b)?;
    ensure!(changes.is_empty());

    a.set(2, "foo".into())?;
    a.flush()?;
    b.set(2, "foo".into())?;
    b.flush()?;

    let changes = diff(&a, &b)?;
    ensure!(changes.is_empty());

    Ok(())
}

#[quickcheck]
fn test_simple_add(BitWidth2to18(bit_width): BitWidth2to18) -> Result<()> {
    let prev_store = MemoryBlockstore::new();
    let curr_store = MemoryBlockstore::new();
    let mut a: Amt<String, _> = Amt::new_with_bit_width(prev_store, bit_width);
    let mut b: Amt<String, _> = Amt::new_with_bit_width(curr_store, bit_width);
    a.set(2, "foo".into())?;
    a.flush()?;
    b.set(2, "foo".into())?;
    b.set(5, "bar".into())?;
    b.flush()?;

    let changes = diff(&a, &b)?;
    ensure!(changes.len() == 1);
    ensure!(
        changes
            == vec![Change {
                key: 5,
                before: None,
                after: Some("bar".into())
            }]
    );

    Ok(())
}

#[quickcheck]
fn test_simple_remove(BitWidth2to18(bit_width): BitWidth2to18) -> Result<()> {
    let prev_store = MemoryBlockstore::new();
    let curr_store = MemoryBlockstore::new();
    let mut a: Amt<String, _> = Amt::new_with_bit_width(prev_store, bit_width);
    let mut b: Amt<String, _> = Amt::new_with_bit_width(curr_store, bit_width);
    a.set(2, "foo".into())?;
    a.set(5, "bar".into())?;
    a.flush()?;
    b.set(2, "foo".into())?;
    b.flush()?;

    let changes = diff(&a, &b)?;
    ensure!(changes.len() == 1);
    ensure!(
        changes[0]
            == Change {
                key: 5,
                before: Some("bar".into()),
                after: None,
            }
    );

    Ok(())
}

#[quickcheck]
fn test_simple_modify(BitWidth2to18(bit_width): BitWidth2to18) -> Result<()> {
    let prev_store = MemoryBlockstore::new();
    let curr_store = MemoryBlockstore::new();
    let mut a: Amt<String, _> = Amt::new_with_bit_width(prev_store, bit_width);
    let mut b: Amt<String, _> = Amt::new_with_bit_width(curr_store, bit_width);
    a.set(2, "foo".into())?;
    a.flush()?;
    b.set(2, "bar".into())?;
    b.flush()?;

    let changes = diff(&a, &b)?;
    ensure!(changes.len() == 1);
    ensure!(
        changes
            == vec![Change {
                key: 2,
                before: Some("foo".into()),
                after: Some("bar".into()),
            }]
    );

    Ok(())
}

#[quickcheck]
fn test_large_modify(BitWidth2to18(bit_width): BitWidth2to18) -> Result<()> {
    let prev_store = MemoryBlockstore::new();
    let curr_store = MemoryBlockstore::new();
    let mut a: Amt<String, _> = Amt::new_with_bit_width(prev_store, bit_width);
    let mut b: Amt<String, _> = Amt::new_with_bit_width(curr_store, bit_width);
    for i in 0..100 {
        a.set(i, format!("foo{i}"))?;
    }
    a.flush()?;

    let mut expected_changes = vec![];
    for i in (0..100).step_by(2) {
        b.set(i, format!("bar{i}"))?;
        expected_changes.push(Change {
            key: i,
            before: Some(format!("foo{i}")),
            after: Some(format!("bar{i}")),
        });
        expected_changes.push(Change {
            key: i + 1,
            before: Some(format!("foo{}", i + 1)),
            after: None,
        });
    }
    b.flush()?;

    let mut changes = diff(&a, &b)?;
    ensure!(changes.len() == 100);
    changes.sort_by(|a, b| a.key.cmp(&b.key));

    ensure!(changes == expected_changes);

    Ok(())
}

#[quickcheck]
fn test_large_additions(BitWidth2to18(bit_width): BitWidth2to18) -> Result<()> {
    let prev_store = MemoryBlockstore::new();
    let curr_store = MemoryBlockstore::new();
    let mut a: Amt<String, _> = Amt::new_with_bit_width(prev_store, bit_width);
    let mut b: Amt<String, _> = Amt::new_with_bit_width(curr_store, bit_width);
    for i in 0..100 {
        a.set(i, format!("foo{i}"))?;
        b.set(i, format!("foo{i}"))?;
    }

    let mut expected_changes = vec![];
    for i in 2000..2500 {
        b.set(i, format!("bar{i}"))?;
        expected_changes.push(Change {
            key: i,
            before: None,
            after: Some(format!("bar{i}")),
        });
    }

    a.flush()?;
    b.flush()?;

    let mut changes = diff(&a, &b)?;
    ensure!(changes.len() == 500);
    changes.sort_by(|a, b| a.key.cmp(&b.key));

    ensure!(changes == expected_changes);

    Ok(())
}

#[quickcheck]
fn test_big_diff(
    BitWidth2to18(bit_width): BitWidth2to18,
    flush_a: bool,
    flush_b: bool,
) -> Result<()> {
    let prev_store = MemoryBlockstore::new();
    let curr_store = MemoryBlockstore::new();
    let mut a: Amt<String, _> = Amt::new_with_bit_width(prev_store, bit_width);
    let mut b: Amt<String, _> = Amt::new_with_bit_width(curr_store, bit_width);
    for i in 0..100 {
        a.set(i, format!("foo{i}"))?;
    }

    let mut expected_changes = vec![];
    for i in (0..100).step_by(2) {
        b.set(i, format!("bar{i}"))?;
        expected_changes.push(Change {
            key: i,
            before: Some(format!("foo{i}")),
            after: Some(format!("bar{i}")),
        });
        expected_changes.push(Change {
            key: i + 1,
            before: Some(format!("foo{}", i + 1)),
            after: None,
        });
    }

    for i in 1000..1500 {
        a.set(i, format!("foo{i}"))?;
        b.set(i, format!("bar{i}"))?;
        expected_changes.push(Change {
            key: i,
            before: Some(format!("foo{i}")),
            after: Some(format!("bar{i}")),
        });
    }

    for i in 2000..2500 {
        b.set(i, format!("bar{i}"))?;
        expected_changes.push(Change {
            key: i,
            before: None,
            after: Some(format!("bar{i}")),
        });
    }

    for i in 10000..10250 {
        a.set(i, format!("foo{i}"))?;
        expected_changes.push(Change {
            key: i,
            before: Some(format!("foo{i}")),
            after: None,
        });
    }

    for i in 10250..10500 {
        a.set(i, format!("foo{i}"))?;
        b.set(i, format!("bar{i}"))?;
        expected_changes.push(Change {
            key: i,
            before: Some(format!("foo{i}")),
            after: Some(format!("bar{i}")),
        });
    }

    if flush_a {
        a.flush()?;
    }
    if flush_b {
        b.flush()?;
    }

    let mut changes = diff(&a, &b)?;
    ensure!(changes.len() == 1600);
    changes.sort_by(|a, b| a.key.cmp(&b.key));

    ensure!(changes == expected_changes);

    Ok(())
}

#[quickcheck]
fn test_diff_empty_state_with_non_empty_state(
    BitWidth2to18(bit_width): BitWidth2to18,
) -> Result<()> {
    let prev_store = MemoryBlockstore::new();
    let curr_store = MemoryBlockstore::new();
    let mut a: Amt<String, _> = Amt::new_with_bit_width(prev_store, bit_width);
    let mut b: Amt<String, _> = Amt::new_with_bit_width(curr_store, bit_width);

    a.set(2, "foo".into())?;
    a.flush()?;
    b.flush()?;

    let changes = diff(&a, &b)?;
    ensure!(changes.len() == 1);
    ensure!(
        changes[0]
            == Change {
                key: 2,
                before: Some("foo".into()),
                after: None,
            }
    );

    Ok(())
}
