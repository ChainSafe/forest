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

// #[test]
// fn set_delete_many() {
// let store = db::MemoryDB::default();

// let mut hamt: Hamt<String, u64, _> = Hamt::new(&store);

// let c = hamt.flush().unwrap();
// }
