// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Code::Blake2b256;
use ipld_blockstore::{BSStats, BlockStore, TrackingBlockStore};
use ipld_hamt::BytesKey;
use ipld_hamt::Hamt;
use serde_bytes::ByteBuf;
use std::fmt::Display;

#[cfg(feature = "identity")]
use ipld_hamt::Identity;

// Redeclaring max array size of Hamt to avoid exposing value
const BUCKET_SIZE: usize = 3;

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
fn test_set_if_absent() {
    let mem = db::MemoryDB::default();
    let store = TrackingBlockStore::new(&mem);

    let mut hamt: Hamt<_, _> = Hamt::new(&store);
    assert!(hamt
        .set_if_absent(tstring("favorite-animal"), tstring("owl bear"))
        .unwrap());

    // Next two are negatively asserted, shouldn't change
    assert!(!hamt
        .set_if_absent(tstring("favorite-animal"), tstring("bright green bear"))
        .unwrap());
    assert!(!hamt
        .set_if_absent(tstring("favorite-animal"), tstring("owl bear"))
        .unwrap());

    let c = hamt.flush().unwrap();

    let mut h2 = Hamt::<_, BytesKey>::load(&c, &store).unwrap();
    // Reloading should still have same effect
    assert!(!h2
        .set_if_absent(tstring("favorite-animal"), tstring("bright green bear"))
        .unwrap());

    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzaced2tgnlsq4n2ioe6ldy75fw3vlrrkyfv4bq6didbwoob2552zvpuk"
    );
    #[rustfmt::skip]
    assert_eq!(*store.stats.borrow(), BSStats {r: 1, w: 1, br: 63, bw: 63});
}

#[test]
fn set_with_no_effect_does_not_put() {
    let mem = db::MemoryDB::default();
    let store = TrackingBlockStore::new(&mem);

    let mut begn: Hamt<_, _> = Hamt::new_with_bit_width(&store, 1);
    let entries = 2 * BUCKET_SIZE * 5;
    for i in 0..entries {
        begn.set(tstring(i), tstring("filler")).unwrap();
    }

    let c = begn.flush().unwrap();
    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzacebjilcrsqa4uyxuh36gllup4rlgnvwgeywdm5yqq2ks4jrsj756qq"
    );

    begn.set(tstring("favorite-animal"), tstring("bright green bear"))
        .unwrap();
    let c2 = begn.flush().unwrap();
    assert_eq!(
        c2.to_string().as_str(),
        "bafy2bzacea7biyabzk7v7le2rrlec5tesjbdnymh5sk4lfprxibg4rtudwtku"
    );
    #[rustfmt::skip]
    assert_eq!(*store.stats.borrow(), BSStats {r: 0, w: 18, br: 0, bw: 1282});

    // This insert should not change value or affect reads or writes
    begn.set(tstring("favorite-animal"), tstring("bright green bear"))
        .unwrap();
    let c3 = begn.flush().unwrap();
    assert_eq!(
        c3.to_string().as_str(),
        "bafy2bzacea7biyabzk7v7le2rrlec5tesjbdnymh5sk4lfprxibg4rtudwtku"
    );

    #[rustfmt::skip]
    assert_eq!(*store.stats.borrow(), BSStats {r:0, w:19, br:0, bw:1372});
}

#[test]
fn delete() {
    let mem = db::MemoryDB::default();
    let store = TrackingBlockStore::new(&mem);

    let mut hamt: Hamt<_, _> = Hamt::new(&store);
    hamt.set(tstring("foo"), tstring("cat dog bear")).unwrap();
    hamt.set(tstring("bar"), tstring("cat dog")).unwrap();
    hamt.set(tstring("baz"), tstring("cat")).unwrap();

    let c = hamt.flush().unwrap();
    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzacebql36crv4odvxzstx2ubaczmawy2tlljxezvorcsoqeyyojxkrom"
    );

    let mut h2 = Hamt::<_, BytesKey>::load(&c, &store).unwrap();
    assert!(h2.delete(&b"foo".to_vec()).unwrap().is_some());
    assert_eq!(h2.get(&b"foo".to_vec()).unwrap(), None);

    let c2 = h2.flush().unwrap();
    assert_eq!(
        c2.to_string().as_str(),
        "bafy2bzaced7up7wkm7cirieh5bs4iyula5inrprihmjzozmku3ywvekzzmlyi"
    );
    #[rustfmt::skip]
    assert_eq!(*store.stats.borrow(), BSStats {r:1, w:2, br:79, bw:139});
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
        "bafy2bzaceb2hikcc6tfuuuuehjstbiq356oruwx6ejyse77zupq445unranv6"
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
    assert_eq!(*store.stats.borrow(), BSStats {r: 1, w: 2, br: 31, bw: 34});
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
    let mut hamt: Hamt<_, BytesKey> = Hamt::new_with_bit_width(&store, 5);

    for i in 0..200 {
        hamt.set(tstring(i), tstring(i)).unwrap();
    }

    let c1 = hamt.flush().unwrap();
    assert_eq!(
        c1.to_string().as_str(),
        "bafy2bzaceczhz54xmmz3xqnbmvxfbaty3qprr6dq7xh5vzwqbirlsnbd36z7a"
    );

    for i in 200..400 {
        hamt.set(tstring(i), tstring(i)).unwrap();
    }

    let cid_all = hamt.flush().unwrap();
    assert_eq!(
        cid_all.to_string().as_str(),
        "bafy2bzacecxcp736xkl2mcyjlors3tug6vdlbispbzxvb75xlrhthiw2xwxvw"
    );

    for i in 200..400 {
        assert!(hamt.delete(&tstring(i)).unwrap().is_some());
    }
    // Ensure first 200 keys still exist
    for i in 0..200 {
        assert_eq!(hamt.get(&tstring(i)).unwrap(), Some(&tstring(i)));
    }

    let cid_d = hamt.flush().unwrap();
    assert_eq!(
        cid_d.to_string().as_str(),
        "bafy2bzaceczhz54xmmz3xqnbmvxfbaty3qprr6dq7xh5vzwqbirlsnbd36z7a"
    );
    #[rustfmt::skip]
    assert_eq!(*store.stats.borrow(), BSStats {r: 0, w: 93, br: 0, bw: 11734});
}
#[test]
fn for_each() {
    let mem = db::MemoryDB::default();
    let store = TrackingBlockStore::new(&mem);

    let mut hamt: Hamt<_, BytesKey> = Hamt::new_with_bit_width(&store, 5);

    for i in 0..200 {
        hamt.set(tstring(i), tstring(i)).unwrap();
    }

    // Iterating through hamt with dirty caches.
    let mut count = 0;
    hamt.for_each(|k, v| {
        assert_eq!(k, v);
        count += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(count, 200);

    let c = hamt.flush().unwrap();
    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzaceczhz54xmmz3xqnbmvxfbaty3qprr6dq7xh5vzwqbirlsnbd36z7a"
    );

    let mut hamt: Hamt<_, BytesKey> = Hamt::load_with_bit_width(&c, &store, 5).unwrap();

    // Iterating through hamt with no cache.
    let mut count = 0;
    hamt.for_each(|k, v| {
        assert_eq!(k, v);
        count += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(count, 200);

    // Iterating through hamt with cached nodes.
    let mut count = 0;
    hamt.for_each(|k, v| {
        assert_eq!(k, v);
        count += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(count, 200);

    let c = hamt.flush().unwrap();
    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzaceczhz54xmmz3xqnbmvxfbaty3qprr6dq7xh5vzwqbirlsnbd36z7a"
    );

    #[rustfmt::skip]
    assert_eq!(*store.stats.borrow(), BSStats {r: 30, w: 31, br: 3209, bw: 4529});
}

#[cfg(feature = "identity")]
fn add_and_remove_keys(
    bit_width: u32,
    keys: &[&[u8]],
    extra_keys: &[&[u8]],
    expected: &'static str,
    stats: BSStats,
) {
    let all: Vec<(BytesKey, BytesKey)> = keys
        .iter()
        .enumerate()
        // Value doesn't matter for this test, only checking cids against previous
        .map(|(i, k)| (k.to_vec().into(), tstring(i)))
        .collect();

    let mem = db::MemoryDB::default();
    let store = TrackingBlockStore::new(&mem);

    let mut hamt: Hamt<_, _, _, Identity> = Hamt::new_with_bit_width(&store, bit_width);

    for (k, v) in all.iter() {
        hamt.set(k.clone(), v.clone()).unwrap();
    }
    let cid = hamt.flush().unwrap();

    let mut h1: Hamt<_, _, BytesKey, Identity> =
        Hamt::load_with_bit_width(&cid, &store, bit_width).unwrap();

    for (k, v) in all {
        assert_eq!(Some(&v), h1.get(&k).unwrap());
    }

    // Set and delete extra keys
    for k in extra_keys.iter() {
        hamt.set(k.to_vec().into(), tstring(0)).unwrap();
    }
    for k in extra_keys.iter() {
        hamt.delete(*k).unwrap();
    }
    let cid2 = hamt.flush().unwrap();
    let mut h2: Hamt<_, BytesKey, BytesKey, Identity> =
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
        "bafy2bzacecosy45hp4sz2t4o4flxvntnwjy7yaq43bykci22xycpeuj542lse",
        BSStats {r: 2, w: 4, br: 38, bw: 76},
    );

    #[rustfmt::skip]
    add_and_remove_keys(
        8,
        &[b"K0", b"K1", b"KAA1", b"KAA2", b"KAA3"],
        &[b"KAA4"],
        "bafy2bzaceaqdaj5aqkwugr7wx4to3fahynoqlxuo5j6xznly3khazgyxihkbo",
        BSStats {r:3, w:6, br:163, bw:326},
    );
}

#[test]
#[cfg(feature = "identity")]
fn canonical_structure_alt_bit_width() {
    let kb_cases = [
        "bafy2bzacec3cquclaqkb32cntwtizgij55b7isb4s5hv5hv5ujbbeu6clxkug",
        "bafy2bzacebj7b2jahw7nxmu6mlhkwzucjmfq7aqlj52jusqtufqtaxcma4pdm",
        "bafy2bzacedrwwndijql6lmmtyicjwyehxtgey5fhzocc43hrzhetrz25v2k2y",
    ];

    let other_cases = [
        "bafy2bzacedbiipe7l7gbtjandyyl6rqlkuqr2im2nl7d4bljidv5mta22rjqk",
        "bafy2bzaceb3c76qlbsiv3baogpao3zah56eqonsowpkof33o5hmncfow4seso",
        "bafy2bzacebhkyrwfexokaoygsx2crydq3fosiyfoa5bthphntmicsco2xf442",
    ];

    #[rustfmt::skip]
    let kb_stats = [
        BSStats {r: 2, w: 4, br: 22, bw: 44},
        BSStats {r: 2, w: 4, br: 24, bw: 48},
        BSStats {r: 2, w: 4, br: 28, bw: 56},
    ];

    #[rustfmt::skip]
    let other_stats = [
        BSStats {r: 3, w: 6, br: 139, bw: 278},
        BSStats {r: 3, w: 6, br: 146, bw: 292},
        BSStats {r: 3, w: 6, br: 154, bw: 308},
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
fn clean_child_ordering() {
    let make_key = |i: u64| -> BytesKey {
        let mut key = unsigned_varint::encode::u64_buffer();
        let n = unsigned_varint::encode::u64(i, &mut key);
        n.to_vec().into()
    };

    let dummy_value: u8 = 42;

    let mem = db::MemoryDB::default();
    let store = TrackingBlockStore::new(&mem);

    let mut h: Hamt<_, _> = Hamt::new_with_bit_width(&store, 5);

    for i in 100..195 {
        h.set(make_key(i), dummy_value).unwrap();
    }

    let root = h.flush().unwrap();
    assert_eq!(
        root.to_string().as_str(),
        "bafy2bzacebqox3gtng4ytexyacr6zmaliyins3llnhbnfbcrqmhzuhmuuawqk"
    );
    let mut h = Hamt::<_, u8>::load_with_bit_width(&root, &store, 5).unwrap();

    h.delete(&make_key(104)).unwrap();
    h.delete(&make_key(108)).unwrap();
    let root = h.flush().unwrap();
    Hamt::<_, u8>::load_with_bit_width(&root, &store, 5).unwrap();

    assert_eq!(
        root.to_string().as_str(),
        "bafy2bzacedlyeuub3mo4aweqs7zyxrbldsq2u4a2taswubudgupglu2j4eru6"
    );

    #[rustfmt::skip]
    assert_eq!(*store.stats.borrow(), BSStats {r: 3, w: 11, br: 1449, bw: 1751});
}

fn tstring(v: impl Display) -> BytesKey {
    BytesKey(v.to_string().into_bytes())
}
