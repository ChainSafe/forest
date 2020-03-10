// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ipld_hamt::Hamt;

#[cfg(not(feature = "identity-hash"))]
use cid::multihash::Blake2b256;
#[cfg(not(feature = "identity-hash"))]
use ipld_blockstore::BlockStore;
#[cfg(not(feature = "identity-hash"))]
use serde_bytes::ByteBuf;

#[cfg(feature = "identity-hash")]
use ipld_hamt::DEFAULT_BIT_WIDTH;

#[test]
fn test_basics() {
    let store = db::MemoryDB::default();
    let mut hamt = Hamt::new(&store);
    assert!(hamt.set(1, "world".to_string()).unwrap().is_none());

    assert_eq!(hamt.get(&1).unwrap(), Some("world".to_string()));
    assert_eq!(
        hamt.set(1, "world2".to_string()).unwrap(),
        Some("world".to_string())
    );
    assert_eq!(hamt.get(&1).unwrap(), Some("world2".to_string()));
}

#[test]
fn test_load() {
    let store = db::MemoryDB::default();

    let mut hamt: Hamt<usize, String, _> = Hamt::new(&store);
    assert!(hamt.set(1, "world".to_string()).unwrap().is_none());

    assert_eq!(hamt.get(&1).unwrap(), Some("world".to_string()));
    assert_eq!(
        hamt.set(1, "world2".to_string()).unwrap(),
        Some("world".to_string())
    );
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
    assert!(Hamt::<usize, String, _>::load(&c2, &empty_store).is_err());

    // storing the hamt should produce the same cid as storing the root
    let c3 = hamt.flush().unwrap();
    assert_eq!(c3, c2);
}

#[test]
#[cfg(not(feature = "identity-hash"))]
fn delete() {
    let store = db::MemoryDB::default();

    // ! Note that bytes must be specifically indicated serde_bytes type
    let mut hamt: Hamt<String, ByteBuf, _> = Hamt::new(&store);
    let (v1, v2, v3): (&[u8], &[u8], &[u8]) = (
        b"cat dog bear".as_ref(),
        b"cat dog".as_ref(),
        b"cat".as_ref(),
    );
    hamt.set("foo".to_owned(), ByteBuf::from(v1)).unwrap();
    hamt.set("bar".to_owned(), ByteBuf::from(v2)).unwrap();
    hamt.set("baz".to_owned(), ByteBuf::from(v3)).unwrap();

    let c = hamt.flush().unwrap();
    assert_eq!(
        hex::encode(c.to_bytes()),
        "0171a0e402209531e0f913dff0c17f8dddb35e2cbf5bbc940c6abef5604c06fc4de3e8101c53"
    );

    let mut h2 = Hamt::<String, ByteBuf, _>::load(&c, &store).unwrap();
    assert_eq!(
        h2.delete(&"foo".to_owned()).unwrap(),
        Some(ByteBuf::from(v1))
    );
    assert_eq!(h2.get(&"foo".to_owned()).unwrap(), None);

    // Assert previous hamt still has access
    assert_eq!(
        hamt.get(&"foo".to_owned()).unwrap(),
        Some(ByteBuf::from(v1))
    );

    let c2 = h2.flush().unwrap();
    assert_eq!(
        hex::encode(c2.to_bytes()),
        "0171a0e4022017a2dc44939d3b74b086cd78dd927edbf20c81d39c576bdc4fc48931b2f2b117"
    );
}

#[test]
#[cfg(not(feature = "identity-hash"))]
fn reload_empty() {
    let store = db::MemoryDB::default();

    let hamt: Hamt<String, Vec<u8>, _> = Hamt::new(&store);
    let c = store.put(&hamt, Blake2b256).unwrap();
    assert_eq!(
        hex::encode(c.to_bytes()),
        "0171a0e4022018fe6acc61a3a36b0c373c4a3a8ea64b812bf2ca9b528050909c78d408558a0c"
    );
    let h2 = Hamt::<String, Vec<u8>, _>::load(&c, &store).unwrap();
    let c2 = store.put(&h2, Blake2b256).unwrap();
    assert_eq!(c, c2);
}

#[test]
#[cfg(not(feature = "identity-hash"))]
fn set_delete_many() {
    let store = db::MemoryDB::default();

    // Test vectors setup specifically for bit width of 5
    let mut hamt: Hamt<String, u64, _> = Hamt::new_with_bit_width(&store, 5);

    for i in 0..200 {
        assert!(hamt.set(format!("{}", i), i).unwrap().is_none());
    }

    let c1 = hamt.flush().unwrap();
    assert_eq!(
        hex::encode(c1.to_bytes()),
        "0171a0e402206379d4c48c8a0457683d45c0cd2bd601e3758c202c5a02b2cab043c9a777b105"
    );

    for i in 200..400 {
        assert!(hamt.set(format!("{}", i), i).unwrap().is_none());
    }

    let cid_all = hamt.flush().unwrap();
    assert_eq!(
        hex::encode(cid_all.to_bytes()),
        "0171a0e402201dc496895750c0b731021269bae57f36acc0becfdf98ef219a9f567786804cc8"
    );

    for i in 200..400 {
        assert_eq!(hamt.delete(&format!("{}", i)).unwrap(), Some(i));
    }
    // Ensure first 200 keys still exist
    for i in 0..200 {
        assert_eq!(hamt.get(&format!("{}", i)).unwrap(), Some(i));
    }

    let cid_d = hamt.flush().unwrap();
    assert_eq!(
        hex::encode(cid_d.to_bytes()),
        "0171a0e402206379d4c48c8a0457683d45c0cd2bd601e3758c202c5a02b2cab043c9a777b105"
    );
}

#[cfg(feature = "identity-hash")]
fn add_and_remove_keys(bit_width: u8, keys: &[&str], extra_keys: &[&str]) {
    let all: Vec<(String, u8)> = keys
        .iter()
        .enumerate()
        // Value doesn't matter for this test, only checking cids against previous
        .map(|(i, k)| (k.to_string(), i as u8))
        .collect();

    let store = db::MemoryDB::default();

    let mut hamt: Hamt<String, u8, _> = Hamt::new_with_bit_width(&store, bit_width);

    for (k, v) in all.iter() {
        hamt.set(k.to_string(), *v).unwrap();
    }
    let cid = hamt.flush().unwrap();

    let mut h1: Hamt<String, u8, _> = Hamt::load_with_bit_width(&cid, &store, bit_width).unwrap();

    for (k, v) in all {
        assert_eq!(Some(v), h1.get(&k).unwrap());
    }

    // Set and delete extra keys
    for k in extra_keys.iter() {
        hamt.set(k.to_string(), 0).unwrap();
    }
    for k in extra_keys.iter() {
        hamt.delete(&k.to_string()).unwrap();
    }
    let cid2 = hamt.flush().unwrap();
    let mut h2: Hamt<String, u8, _> = Hamt::load(&cid2, &store).unwrap();

    let cid1 = h1.flush().unwrap();
    let cid2 = h2.flush().unwrap();
    assert_eq!(cid1, cid2);
}

#[test]
#[cfg(feature = "identity-hash")]
fn canonical_structure() {
    // Champ mutation semantics test
    add_and_remove_keys(DEFAULT_BIT_WIDTH, &["K"], &["B"]);
    add_and_remove_keys(
        DEFAULT_BIT_WIDTH,
        &["K0", "K1", "KAA1", "KAA2", "KAA3"],
        &["KAA4"],
    );
}

#[test]
#[cfg(feature = "identity-hash")]
fn canonical_structure_alt_bit_width() {
    for i in 5..DEFAULT_BIT_WIDTH {
        add_and_remove_keys(i, &["K"], &["B"]);
        add_and_remove_keys(i, &["K0", "K1", "KAA1", "KAA2", "KAA3"], &["KAA4"]);
    }
}
