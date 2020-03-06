// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::multihash::Blake2b256;
use ipld_blockstore::BlockStore;
use ipld_hamt::Hamt;
use serde_bytes::ByteBuf;

#[test]
fn test_basics() {
    let store = db::MemoryDB::default();
    let mut hamt = Hamt::new(&store);
    assert!(hamt.insert(1, "world".to_string()).is_none());

    assert_eq!(hamt.get(&1), Some(&"world".to_string()));
    assert_eq!(
        hamt.insert(1, "world2".to_string()),
        Some("world".to_string())
    );
    assert_eq!(hamt.get(&1), Some(&"world2".to_string()));
}

#[test]
fn test_from_link() {
    let store = db::MemoryDB::default();

    let mut hamt: Hamt<usize, String, _> = Hamt::new(&store);
    assert!(hamt.insert(1, "world".to_string()).is_none());

    assert_eq!(hamt.get(&1), Some(&"world".to_string()));
    assert_eq!(
        hamt.insert(1, "world2".to_string()),
        Some("world".to_string())
    );
    assert_eq!(hamt.get(&1), Some(&"world2".to_string()));
    let c = store.put(&hamt, Blake2b256).unwrap();

    let new_hamt = Hamt::from_link(&c, &store).unwrap();
    assert_eq!(hamt, new_hamt);

    // insert value in the first one
    hamt.insert(2, "stuff".to_string());

    // loading original hash should returnnot be equal now
    let new_hamt = Hamt::from_link(&c, &store).unwrap();
    assert_ne!(hamt, new_hamt);

    // loading new hash
    let c2 = store.put(&hamt, Blake2b256).unwrap();
    let new_hamt = Hamt::from_link(&c2, &store).unwrap();
    assert_eq!(hamt, new_hamt);

    // loading from an empty store does not work
    let empty_store = db::MemoryDB::default();
    assert!(Hamt::<usize, String, _>::from_link(&c2, &empty_store).is_err());

    // storing the hamt should produce the same cid as storing the root
    let c3 = store.put(&hamt, Blake2b256).unwrap();
    assert_eq!(c3, c2);
}

#[test]
fn delete() {
    let store = db::MemoryDB::default();

    // ! Note that bytes must be specifically indicated serde_bytes type
    let mut hamt: Hamt<String, ByteBuf, _> = Hamt::new(&store);
    let (v1, v2, v3): (&[u8], &[u8], &[u8]) = (
        b"cat dog bear".as_ref(),
        b"cat dog".as_ref(),
        b"cat".as_ref(),
    );
    hamt.insert("foo".to_owned(), ByteBuf::from(v1));
    hamt.insert("bar".to_owned(), ByteBuf::from(v2));
    hamt.insert("baz".to_owned(), ByteBuf::from(v3));

    let c = store.put(&hamt, Blake2b256).unwrap();
    assert_eq!(
        hex::encode(c.to_bytes()),
        "0171a0e402209531e0f913dff0c17f8dddb35e2cbf5bbc940c6abef5604c06fc4de3e8101c53"
    );

    let mut h2 = Hamt::<String, ByteBuf, _>::from_link(&c, &store).unwrap();
    assert_eq!(h2.remove(&"foo".to_owned()), Some(ByteBuf::from(v1)));
    assert_eq!(h2.get(&"foo".to_owned()), None);

    // Assert previous hamt still has access
    assert_eq!(hamt.get(&"foo".to_owned()), Some(&ByteBuf::from(v1)));

    let c2 = store.put(&hamt, Blake2b256).unwrap();
    assert_ne!(
        hex::encode(c2.to_bytes()),
        "0171a0e4022017a2dc44939d3b74b086cd78dd927edbf20c81d39c576bdc4fc48931b2f2b117"
    );
}

#[test]
fn reload_empty() {
    let store = db::MemoryDB::default();

    let hamt: Hamt<String, Vec<u8>, _> = Hamt::new(&store);
    let c = store.put(&hamt, Blake2b256).unwrap();
    assert_eq!(
        hex::encode(c.to_bytes()),
        "0171a0e4022018fe6acc61a3a36b0c373c4a3a8ea64b812bf2ca9b528050909c78d408558a0c"
    );
    let h2 = Hamt::<String, Vec<u8>, _>::from_link(&c, &store).unwrap();
    let c2 = store.put(&h2, Blake2b256).unwrap();
    assert_eq!(c, c2);
}
