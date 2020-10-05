// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ipld_hamt::{BytesKey, Hamt};

use cid::multihash::Blake2b256;
use ipld_blockstore::{BSStats, BlockStore, TrackingBlockStore};
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
fn delete() {
    let mem = db::MemoryDB::default();
    let store = TrackingBlockStore::new(&mem);

    let mut hamt: Hamt<_, _> = Hamt::new(&store);
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
        c.to_string().as_str(),
        "bafy2bzacebhjoag2qmyibmvvzq372pg2evlkchovqdksmna4hm7py5itnrlhg"
    );

    let mut h2 = Hamt::<_, ByteBuf>::load(&c, &store).unwrap();
    assert_eq!(h2.delete(&b"foo".to_vec()).unwrap(), true);
    assert_eq!(h2.get(&b"foo".to_vec()).unwrap(), None);

    let c2 = h2.flush().unwrap();
    assert_eq!(
        c2.to_string().as_str(),
        "bafy2bzaceczehhtzfhg4ijrkv2omajt5ygwbd6srqhhtkxgd2hjttpihxs5ky"
    );
    #[rustfmt::skip]
    assert_eq!(*store.stats.borrow(), BSStats {r: 1, w: 2, br: 88, bw: 154});
}

#[test]
fn reload_empty() {
    let mem = db::MemoryDB::default();
    let store = TrackingBlockStore::new(&mem);

    let hamt: Hamt<_, ()> = Hamt::new(&store);
    let c = store.put(&hamt, Blake2b256).unwrap();

    let h2 = Hamt::<_, ()>::load(&c, &store).unwrap();
    let c2 = store.put(&h2, Blake2b256).unwrap();
    assert_eq!(c, c2);
    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzaceamp42wmmgr2g2ymg46euououzfyck7szknvfacqscohrvaikwfay"
    );
    #[rustfmt::skip]
    assert_eq!(*store.stats.borrow(), BSStats {r: 1, w: 2, br: 3, bw: 6});
}

#[test]
fn set_delete_many() {
    let mem = db::MemoryDB::default();
    let store = TrackingBlockStore::new(&mem);

    // Test vectors setup specifically for bit width of 5
    let mut hamt: Hamt<_, _> = Hamt::new_with_bit_width(&store, 5);

    for i in 0..200 {
        hamt.set(format!("{}", i).into_bytes().into(), i).unwrap();
    }

    let c1 = hamt.flush().unwrap();
    assert_eq!(
        c1.to_string().as_str(),
        "bafy2bzaceaneyzybb37pn4rtg2mvn2qxb43rhgmqoojgtz7avdfjw2lhz4dge"
    );

    for i in 200..400 {
        hamt.set(format!("{}", i).into_bytes().into(), i).unwrap();
    }

    let cid_all = hamt.flush().unwrap();
    assert_eq!(
        cid_all.to_string().as_str(),
        "bafy2bzaceaqmub32nf33s3joo6x2l3schxreuow7jkla7a27l7qcrsb2elzay"
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
        cid_d.to_string().as_str(),
        "bafy2bzaceaneyzybb37pn4rtg2mvn2qxb43rhgmqoojgtz7avdfjw2lhz4dge"
    );
    #[rustfmt::skip]
    assert_eq!(*store.stats.borrow(), BSStats {r: 87, w: 119, br: 7671, bw: 14042});
}

#[cfg(feature = "identity")]
fn add_and_remove_keys(
    bit_width: u32,
    keys: &[&[u8]],
    extra_keys: &[&[u8]],
    expected: &'static str,
    stats: BSStats,
) {
    let all: Vec<(BytesKey, u8)> = keys
        .iter()
        .enumerate()
        // Value doesn't matter for this test, only checking cids against previous
        .map(|(i, k)| (k.to_vec().into(), i as u8))
        .collect();

    let mem = db::MemoryDB::default();
    let store = TrackingBlockStore::new(&mem);

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
    assert_eq!(cid1.to_string().as_str(), expected);
    assert_eq!(*store.stats.borrow(), stats);
}

#[test]
#[cfg(feature = "identity")]
fn canonical_structure() {
    // Champ mutation semantics test
    #[rustfmt::skip]
    add_and_remove_keys(
        DEFAULT_BIT_WIDTH,
        &[b"K"],
        &[b"B"],
        "bafy2bzacecdihronbg6gyhpzhuiax3thpv5gxpunwczuanqym3r7wih3bkmb4",
        BSStats {r: 2, w: 5, br: 42, bw: 105},
    );
    #[rustfmt::skip]
    add_and_remove_keys(
        DEFAULT_BIT_WIDTH,
        &[b"K0", b"K1", b"KAA1", b"KAA2", b"KAA3"],
        &[b"KAA4"],
        "bafy2bzaceaxfdngr56h3kj5hplwslhyxauizcpwx3agwjqns6gjhroepgnfkm",
        BSStats {r: 2, w: 5, br: 168, bw: 420},
    );
}

#[test]
#[cfg(feature = "identity")]
fn canonical_structure_alt_bit_width() {
    #[rustfmt::skip]
    let kb_cases = [
        (
            "bafy2bzacedckymwjxmg35sllfegwrmnr7rxb3x7dh6muec2li2qhqjk5tf63q",
            BSStats {r: 2, w: 5, br: 32, bw: 80},
        ),
        (
            "bafy2bzacec2f6scvfmnyal5pzn43if6e2kls5jbm2jdab2xzuditctd5i3bbi",
            BSStats {r: 2, w: 5, br: 28, bw: 70},
        ),
        (
            "bafy2bzacecnabvcxw7k5hgfcex5ig4jf3nabuxvl35edgnjk5ven2kg4n3ffm",
            BSStats {r: 2, w: 5, br: 26, bw: 65},
        ),
    ];
    #[rustfmt::skip]
    let other_cases = [
        (
            "bafy2bzaceckigpba3kck23qyuyb2i6vbiprtsmlr2rlyn3o4lmmcvzsh3l6wi",
            BSStats {r: 8, w: 13, br: 419, bw: 687},
        ),
        (
            "bafy2bzacedeeqff3p7n3ogqxvqslb2yrbi4ojz44sp6mvjwyp6u6lktxdo2fg",
            BSStats {r: 8, w: 13, br: 385, bw: 639},
        ),
        (
            "bafy2bzacedc7hh2tyz66m7n7ricyw2m7whsgoplyux3kbxczla7zuf25wi2og",
            BSStats {r: 9, w: 14, br: 420, bw: 661},
        ),
    ];
    for i in 5..8 {
        #[rustfmt::skip]
        add_and_remove_keys(
            i,
            &[b"K"],
            &[b"B"],
            kb_cases[(i - 5) as usize].0,
            kb_cases[(i - 5) as usize].1,
        );
        #[rustfmt::skip]
        add_and_remove_keys(
            i,
            &[b"K0", b"K1", b"KAA1", b"KAA2", b"KAA3"],
            &[b"KAA4"],
            other_cases[(i - 5) as usize].0,
            other_cases[(i - 5) as usize].1,
        );
    }
}
