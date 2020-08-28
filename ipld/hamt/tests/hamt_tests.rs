// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ipld_hamt::{BytesKey, Hamt};

#[cfg(feature = "murmur")]
use cid::multihash::Blake2b256;
#[cfg(feature = "murmur")]
use ipld_blockstore::BlockStore;
#[cfg(feature = "murmur")]
use ipld_hamt::Murmur3;
#[cfg(feature = "murmur")]
use serde_bytes::ByteBuf;

#[cfg(feature = "identity")]
use ipld_hamt::Identity;

// Duplicate kept here to not have to expose the default.
const DEFAULT_BIT_WIDTH: u32 = 8;

#[test]
fn test_basics() {
    let store = db::MemoryDB::default();
    let mut hamt = Hamt::<_, String, _>::new(&store);
    hamt.set(1, "world".to_string()).unwrap();

    assert_eq!(hamt.get(&1).unwrap(), Some("world".to_string()));
    hamt.set(1, "world2".to_string()).unwrap();
    assert_eq!(hamt.get(&1).unwrap(), Some("world2".to_string()));
}

#[test]
fn test_load() {
    let store = db::MemoryDB::default();

    let mut hamt: Hamt<_, _, usize> = Hamt::new(&store);
    hamt.set(1, "world".to_string()).unwrap();

    assert_eq!(hamt.get(&1).unwrap(), Some("world".to_string()));
    hamt.set(1, "world2".to_string()).unwrap();
    assert_eq!(hamt.get(&1).unwrap(), Some("world2".to_string()));
    let c = hamt.flush().unwrap();

    let new_hamt = Hamt::load(&c, &store).unwrap();
    assert_eq!(hamt, new_hamt);

    // set value in the first one
    hamt.set(2, "stuff".to_string()).unwrap();

    // loading original hash should returnnot be equal now
    let new_hamt = Hamt::load(&c, &store).unwrap();
    assert_ne!(hamt, new_hamt);

    // loading new hash
    let c2 = hamt.flush().unwrap();
    let new_hamt = Hamt::load(&c2, &store).unwrap();
    assert_eq!(hamt, new_hamt);

    // loading from an empty store does not work
    let empty_store = db::MemoryDB::default();
    assert!(Hamt::<_, usize>::load(&c2, &empty_store).is_err());

    // storing the hamt should produce the same cid as storing the root
    let c3 = hamt.flush().unwrap();
    assert_eq!(c3, c2);
}

#[test]
#[cfg(feature = "murmur")]
fn delete() {
    let store = db::MemoryDB::default();

    // ! Note that bytes must be specifically indicated serde_bytes type
    let mut hamt: Hamt<_, _, BytesKey, Murmur3> = Hamt::new(&store);
    let (v1, v2, v3): (&[u8], &[u8], &[u8]) = (
        b"cat dog bear".as_ref(),
        b"cat dog".as_ref(),
        b"cat".as_ref(),
    );
    hamt.set(b"foo".to_vec().into(), ByteBuf::from(v1)).unwrap();
    hamt.set(b"bar".to_vec().into(), ByteBuf::from(v2)).unwrap();
    hamt.set(b"baz".to_vec().into(), ByteBuf::from(v3)).unwrap();

    let c = hamt.flush().unwrap();
    assert_eq!(
        hex::encode(c.to_bytes()),
        "0171a0e402204c4cec750f4e5fc0df61e5a6b6f430d45e6d42108824492658ccd480a4f86aef"
    );

    let mut h2 = Hamt::<_, ByteBuf, BytesKey, Murmur3>::load(&c, &store).unwrap();
    assert_eq!(h2.delete(&b"foo".to_vec()).unwrap(), true);
    assert_eq!(h2.get(&b"foo".to_vec()).unwrap(), None);

    // Assert previous hamt still has access
    assert_eq!(hamt.get(&b"foo".to_vec()).unwrap(), Some(ByteBuf::from(v1)));

    let c2 = h2.flush().unwrap();
    assert_eq!(
        hex::encode(c2.to_bytes()),
        "0171a0e40220f8889d65614928ee8fd0a1fc27fb94357751ce95e99260b16b8789455eb7d212"
    );
}

#[test]
#[cfg(feature = "murmur")]
fn reload_empty() {
    let store = db::MemoryDB::default();

    let hamt: Hamt<_, (), BytesKey, Murmur3> = Hamt::new(&store);
    let c = store.put(&hamt, Blake2b256).unwrap();
    assert_eq!(
        hex::encode(c.to_bytes()),
        "0171a0e4022018fe6acc61a3a36b0c373c4a3a8ea64b812bf2ca9b528050909c78d408558a0c"
    );
    let h2 = Hamt::<_, (), BytesKey, Murmur3>::load(&c, &store).unwrap();
    let c2 = store.put(&h2, Blake2b256).unwrap();
    assert_eq!(c, c2);
}

#[test]
#[cfg(feature = "murmur")]
fn set_delete_many() {
    let store = db::MemoryDB::default();

    // Test vectors setup specifically for bit width of 5
    let mut hamt: Hamt<_, _, BytesKey, Murmur3> = Hamt::new_with_bit_width(&store, 5);

    for i in 0..200 {
        hamt.set(format!("{}", i).into_bytes().into(), i).unwrap();
    }

    let c1 = hamt.flush().unwrap();
    assert_eq!(
        hex::encode(c1.to_bytes()),
        "0171a0e402207c660382de99c174ce39517bdbd28f3967801aebbd9795f0591e226d93e2f010"
    );

    for i in 200..400 {
        hamt.set(format!("{}", i).into_bytes().into(), i).unwrap();
    }

    let cid_all = hamt.flush().unwrap();
    assert_eq!(
        hex::encode(cid_all.to_bytes()),
        "0171a0e40220dba161623db24093bd90e00c3d185bae8468f8d3e81f01f112b3afe47e603fd1"
    );

    for i in 200..400 {
        assert_eq!(hamt.delete(&format!("{}", i).into_bytes()).unwrap(), true);
    }
    // Ensure first 200 keys still exist
    for i in 0..200 {
        assert_eq!(hamt.get(&format!("{}", i).into_bytes()).unwrap(), Some(i));
    }

    let cid_d = hamt.flush().unwrap();
    assert_eq!(
        hex::encode(cid_d.to_bytes()),
        "0171a0e402207c660382de99c174ce39517bdbd28f3967801aebbd9795f0591e226d93e2f010"
    );
}

#[cfg(feature = "identity")]
fn add_and_remove_keys(
    bit_width: u32,
    keys: &[&[u8]],
    extra_keys: &[&[u8]],
    expected: &'static str,
) {
    let all: Vec<(BytesKey, u8)> = keys
        .iter()
        .enumerate()
        // Value doesn't matter for this test, only checking cids against previous
        .map(|(i, k)| (k.to_vec().into(), i as u8))
        .collect();

    let store = db::MemoryDB::default();

    let mut hamt: Hamt<_, _, _, Identity> = Hamt::new_with_bit_width(&store, bit_width);

    for (k, v) in all.iter() {
        hamt.set(k.clone(), *v).unwrap();
    }
    let cid = hamt.flush().unwrap();

    let mut h1: Hamt<_, _, BytesKey, Identity> =
        Hamt::load_with_bit_width(&cid, &store, bit_width).unwrap();

    for (k, v) in all {
        assert_eq!(Some(v), h1.get(&k).unwrap());
    }

    // Set and delete extra keys
    for k in extra_keys.iter() {
        hamt.set(k.to_vec().into(), 0).unwrap();
    }
    for k in extra_keys.iter() {
        hamt.delete(*k).unwrap();
    }
    let cid2 = hamt.flush().unwrap();
    let mut h2: Hamt<_, u8, BytesKey, Identity> =
        Hamt::load_with_bit_width(&cid2, &store, bit_width).unwrap();

    let cid1 = h1.flush().unwrap();
    let cid2 = h2.flush().unwrap();
    assert_eq!(cid1, cid2);
    assert_eq!(hex::encode(cid1.to_bytes()), expected);
}

#[test]
#[cfg(feature = "identity")]
fn canonical_structure() {
    // Champ mutation semantics test
    add_and_remove_keys(
        DEFAULT_BIT_WIDTH,
        &[b"K"],
        &[b"B"],
        "0171a0e402208683c5cd09bc6c1df93d100bee677d7a6bbe8db0b340361866e3fb20fb0a981e",
    );
    add_and_remove_keys(
        DEFAULT_BIT_WIDTH,
        &[b"K0", b"K1", b"KAA1", b"KAA2", b"KAA3"],
        &[b"KAA4"],
        "0171a0e40220e2a9e53c77d146010b60f2be9b3ba423c0db4efea06e66bd87e072671c8ef411",
    );
}

#[test]
#[cfg(feature = "identity")]
fn canonical_structure_alt_bit_width() {
    let kb_cases = [
        "0171a0e402209a00d457b7d5d398a225fa837125db401a5eabdf4833352aed48dd28dc6eca56",
        "0171a0e40220b45f48552b1b802fafcb79b417c4d2972ea42cd24600eaf9a0d1314c7d46c214",
        "0171a0e40220c4ac32c9bb0dbec96b290d68b1b1fc6e1ddfe33f99420b4b46a078255d997db8",
    ];
    let other_cases = [
        "0171a0e40220c5f39f53c67de67dbf8a058b699fb1e4673d78a5f6a0dc59583f9a175db234e3",
        "0171a0e40220c84814bb7fdbb71a17ac24b0eb110a38e4e79c93fccaa6d87fa9e5aa771bb453",
        "0171a0e4022094833c20da84ad6e18a603a47aa143e3393171d45786eddc5b182ae647dafd64",
    ];
    for i in 5..8 {
        add_and_remove_keys(i, &[b"K"], &[b"B"], kb_cases[(i - 5) as usize]);
        add_and_remove_keys(
            i,
            &[b"K0", b"K1", b"KAA1", b"KAA2", b"KAA3"],
            &[b"KAA4"],
            other_cases[(i - 5) as usize],
        );
    }
}
