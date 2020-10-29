// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ipld_hamt::Hamt;

use cid::multihash::Blake2b256;
use ipld_blockstore::{BSStats, BlockStore, TrackingBlockStore};
use serde_bytes::ByteBuf;

#[cfg(any(feature = "identity", feature = "v2"))]
use ipld_hamt::BytesKey;

#[cfg(feature = "identity")]
use ipld_hamt::Identity;

#[test]
fn test_basics() {
    let store = db::MemoryDB::default();
    let mut hamt = Hamt::<_, String, _>::new(&store);
    hamt.set(1, "world".to_string()).unwrap();

    assert_eq!(hamt.get(&1).unwrap(), Some(&"world".to_string()));
    hamt.set(1, "world2".to_string()).unwrap();
    assert_eq!(hamt.get(&1).unwrap(), Some(&"world2".to_string()));
}

#[test]
fn test_load() {
    let store = db::MemoryDB::default();

    let mut hamt: Hamt<_, _, usize> = Hamt::new(&store);
    hamt.set(1, "world".to_string()).unwrap();

    assert_eq!(hamt.get(&1).unwrap(), Some(&"world".to_string()));
    hamt.set(1, "world2".to_string()).unwrap();
    assert_eq!(hamt.get(&1).unwrap(), Some(&"world2".to_string()));
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
    assert!(h2.delete(&b"foo".to_vec()).unwrap().is_some());
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
fn delete_case() {
    let mem = db::MemoryDB::default();
    let store = TrackingBlockStore::new(&mem);

    let mut hamt: Hamt<_, _> = Hamt::new(&store);

    hamt.set([0].to_vec().into(), ByteBuf::from(b"Test data".as_ref()))
        .unwrap();

    let c = hamt.flush().unwrap();
    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzacecngbbdw3ut45b3tnsan3fgxwlsnit25unejfmh4ihlhkxr2hutuo"
    );

    let mut h2 = Hamt::<_, ByteBuf>::load(&c, &store).unwrap();
    assert!(h2.delete(&[0].to_vec()).unwrap().is_some());
    assert_eq!(h2.get(&[0].to_vec()).unwrap(), None);

    let c2 = h2.flush().unwrap();
    assert_eq!(
        c2.to_string().as_str(),
        "bafy2bzaceamp42wmmgr2g2ymg46euououzfyck7szknvfacqscohrvaikwfay"
    );
    #[rustfmt::skip]
    assert_eq!(*store.stats.borrow(), BSStats {r:1, w:2, br:34, bw:37});
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
#[cfg(feature = "v2")]
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
        assert!(hamt
            .delete(&format!("{}", i).into_bytes())
            .unwrap()
            .is_some());
    }
    // Ensure first 200 keys still exist
    for i in 0..200 {
        assert_eq!(hamt.get(&format!("{}", i).into_bytes()).unwrap(), Some(&i));
    }

    let cid_d = hamt.flush().unwrap();
    assert_eq!(
        cid_d.to_string().as_str(),
        "bafy2bzaceaneyzybb37pn4rtg2mvn2qxb43rhgmqoojgtz7avdfjw2lhz4dge"
    );
    #[rustfmt::skip]
    #[cfg(not(feature = "go-interop"))]
    assert_eq!(*store.stats.borrow(), BSStats { r: 0, w: 93, br: 0, bw: 12849 });

    #[rustfmt::skip]
    #[cfg(feature = "go-interop")]
    assert_eq!(*store.stats.borrow(), BSStats {r: 87, w: 119, br: 7671, bw: 14042});
}
#[test]
fn for_each() {
    let mem = db::MemoryDB::default();
    let store = TrackingBlockStore::new(&mem);

    let mut hamt: Hamt<_, _> = Hamt::new_with_bit_width(&store, 5);

    for i in 0..200 {
        hamt.set(format!("{}", i).into_bytes().into(), i).unwrap();
    }

    // Iterating through hamt with dirty caches.
    let mut count = 0;
    hamt.for_each(|k, v| {
        assert_eq!(k.0, format!("{}", v).into_bytes());
        count += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(count, 200);

    let c = hamt.flush().unwrap();
    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzaceaneyzybb37pn4rtg2mvn2qxb43rhgmqoojgtz7avdfjw2lhz4dge"
    );

    let mut hamt: Hamt<_, i32> = Hamt::load_with_bit_width(&c, &store, 5).unwrap();

    // Iterating through hamt with no cache.
    let mut count = 0;
    hamt.for_each(|k, v| {
        assert_eq!(k.0, format!("{}", v).into_bytes());
        count += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(count, 200);

    // Iterating through hamt with cached nodes.
    let mut count = 0;
    hamt.for_each(|k, v| {
        assert_eq!(k.0, format!("{}", v).into_bytes());
        count += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(count, 200);

    let c = hamt.flush().unwrap();
    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzaceaneyzybb37pn4rtg2mvn2qxb43rhgmqoojgtz7avdfjw2lhz4dge"
    );

    #[rustfmt::skip]
    #[cfg(not(feature = "go-interop"))]
    assert_eq!(*store.stats.borrow(), BSStats { r: 30, w: 31, br: 3510, bw: 4914 });

    #[rustfmt::skip]
    #[cfg(feature = "go-interop")]
    assert_eq!(*store.stats.borrow(), BSStats {r: 59, w: 89, br: 4841, bw: 8351});
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
        assert_eq!(Some(&v), h1.get(&k).unwrap());
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
        8,
        &[b"K"],
        &[b"B"],
        "bafy2bzacecdihronbg6gyhpzhuiax3thpv5gxpunwczuanqym3r7wih3bkmb4",
        BSStats {r: 2, w: 4, br: 42, bw: 84},
    );

    #[rustfmt::skip]
    #[cfg(not(feature = "go-interop"))]
    let stats = BSStats { r: 4, w: 6, br: 228, bw: 346 };

    #[rustfmt::skip]
    #[cfg(feature = "go-interop")]
    let stats = BSStats {r: 7, w: 10, br: 388, bw: 561};

    add_and_remove_keys(
        8,
        &[b"K0", b"K1", b"KAA1", b"KAA2", b"KAA3"],
        &[b"KAA4"],
        "bafy2bzacedrktzj4o7iumailmdzl5gz3uqr4bw2o72qg4zv5q7qhezy4r32bc",
        stats,
    );
}

#[test]
#[cfg(feature = "identity")]
fn canonical_structure_alt_bit_width() {
    let kb_cases = [
        "bafy2bzacecnabvcxw7k5hgfcex5ig4jf3nabuxvl35edgnjk5ven2kg4n3ffm",
        "bafy2bzacec2f6scvfmnyal5pzn43if6e2kls5jbm2jdab2xzuditctd5i3bbi",
        "bafy2bzacedckymwjxmg35sllfegwrmnr7rxb3x7dh6muec2li2qhqjk5tf63q",
    ];

    let other_cases = [
        "bafy2bzacedc7hh2tyz66m7n7ricyw2m7whsgoplyux3kbxczla7zuf25wi2og",
        "bafy2bzacedeeqff3p7n3ogqxvqslb2yrbi4ojz44sp6mvjwyp6u6lktxdo2fg",
        "bafy2bzaceckigpba3kck23qyuyb2i6vbiprtsmlr2rlyn3o4lmmcvzsh3l6wi",
    ];

    #[rustfmt::skip]
    let kb_stats = [
        BSStats { r: 2, w: 4, br: 26, bw: 52 },
        BSStats { r: 2, w: 4, br: 28, bw: 56 },
        BSStats { r: 2, w: 4, br: 32, bw: 64 },
    ];

    #[rustfmt::skip]
    #[cfg(not(feature = "go-interop"))]
    let other_stats = [
        BSStats { r: 4, w: 6, br: 190, bw: 292 },
        BSStats { r: 4, w: 6, br: 202, bw: 306 },
        BSStats { r: 4, w: 6, br: 214, bw: 322 },
    ];

    #[rustfmt::skip]
    #[cfg(feature = "go-interop")]
    let other_stats = [
        BSStats {r: 9, w: 12, br: 420, bw: 566},
        BSStats {r: 8, w: 11, br: 385, bw: 538},
        BSStats {r: 8, w: 11, br: 419, bw: 580},
    ];

    for i in 5..8 {
        #[rustfmt::skip]
        add_and_remove_keys(
            i,
            &[b"K"],
            &[b"B"],
            kb_cases[(i - 5) as usize],
            kb_stats[(i - 5) as usize],
        );
        #[rustfmt::skip]
        add_and_remove_keys(
            i,
            &[b"K0", b"K1", b"KAA1", b"KAA2", b"KAA3"],
            &[b"KAA4"],
            other_cases[(i - 5) as usize],
            other_stats[(i - 5) as usize],
        );
    }
}

#[test]
#[cfg(feature = "v2")]
fn clean_child_ordering() {
    let make_key = |i: u64| -> BytesKey {
        let mut key = unsigned_varint::encode::u64_buffer();
        let n = unsigned_varint::encode::u64(i, &mut key);
        n.to_vec().into()
    };

    let dummy_value = BytesKey(vec![0xaa, 0xbb, 0xcc, 0xdd]);

    let mem = db::MemoryDB::default();
    let store = TrackingBlockStore::new(&mem);

    let mut h: Hamt<_, _> = Hamt::new_with_bit_width(&store, 5);

    for i in 100..195 {
        h.set(make_key(i), dummy_value.clone()).unwrap();
    }

    let root = h.flush().unwrap();
    assert_eq!(
        root.to_string().as_str(),
        "bafy2bzaced2mfx4zquihmrbqei2ghtbsf7bvupjzaiwkkgfmvpfrbud25gfli"
    );
    let mut h = Hamt::<_, BytesKey>::load_with_bit_width(&root, &store, 5).unwrap();

    h.delete(&make_key(104)).unwrap();
    h.delete(&make_key(108)).unwrap();
    let root = h.flush().unwrap();
    Hamt::<_, BytesKey>::load_with_bit_width(&root, &store, 5).unwrap();

    assert_eq!(
        root.to_string().as_str(),
        "bafy2bzacec6ro3q36okye22evifu6h7kwdkjlb4keq6ogpfqivka6myk6wkjo"
    );

    #[rustfmt::skip]
    #[cfg(not(feature = "go-interop"))]
    assert_eq!(*store.stats.borrow(), BSStats { r: 3, w: 11, br: 1992, bw: 2510 });

    #[rustfmt::skip]
    #[cfg(feature = "go-interop")]
    assert_eq!(*store.stats.borrow(), BSStats {r: 9, w: 17, br: 2327, bw: 2845});
}
