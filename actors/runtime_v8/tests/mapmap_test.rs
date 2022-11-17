use cid::Cid;
use fil_actors_runtime_v8::{make_map_with_root_and_bitwidth, Keyer, Map, MapMap};
use fvm_ipld_blockstore::MemoryBlockstore;
use fvm_shared::HAMT_BIT_WIDTH;

#[test]
fn mapmap_test() {
    let store = MemoryBlockstore::default();
    let mut mm = MapMap::new(&store, HAMT_BIT_WIDTH, HAMT_BIT_WIDTH);

    let prev = mm.put("tree", "evergreen", "pine".to_string()).unwrap();
    assert!(prev.is_none());
    let prev = mm.put("tree", "evergreen", "cypress".to_string()).unwrap();
    assert_eq!(Some("pine".to_string()), prev);
    assert_eq!(
        Some(&"cypress".to_string()),
        mm.get("tree", "evergreen").unwrap()
    );
    // put_if_absent can write to an unassigned key
    assert!(mm
        .put_if_absent("rock", "igneous", "basalt".to_string())
        .unwrap());

    assert!(mm.get("tree", "deciduous").unwrap().is_none());
    mm.put_many(
        "tree",
        vec![
            ("deciduous", "mango".to_string()),
            ("evergreen", "larch".to_string()),
        ]
        .into_iter(),
    )
    .unwrap();

    // put_many overwrites and adds new inner keys
    assert_eq!(
        Some(&"mango".to_string()),
        mm.get("tree", "deciduous").unwrap()
    );
    assert_eq!(
        Some(&"larch".to_string()),
        mm.get("tree", "evergreen").unwrap()
    );

    // put_if_absent won't overwrite
    assert!(!mm
        .put_if_absent("tree", "deciduous", "guava".to_string())
        .unwrap());

    // for each accounts for all inner keys and values
    let mut count = 0;
    mm.for_each("tree", |bk, v| -> anyhow::Result<()> {
        count += 1;
        assert!(
            (bk == &"deciduous".key() && v == &"mango".to_string())
                || (bk == &"evergreen".key() && v == &"larch".to_string())
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(2, count);

    let mut count = 0;
    mm.for_each("rock", |bk, v| -> anyhow::Result<()> {
        count += 1;
        assert_eq!(&"igneous".key(), bk);
        assert_eq!(&"basalt".to_string(), v);
        Ok(())
    })
    .unwrap();
    assert_eq!(1, count);

    // remove a non existent outer key
    assert!(mm.remove("glacier", "alpine").unwrap().is_none());
    // remove a non existent inner key
    assert!(mm.remove("rock", "sedimentary").unwrap().is_none());

    // remove last remaining member of inner map
    assert_eq!(
        Some("basalt".to_string()),
        mm.remove("rock", "igneous").unwrap()
    );

    let root = mm.flush().unwrap();

    // load the outermap as a map
    let outer_map_raw: Map<MemoryBlockstore, Cid> =
        make_map_with_root_and_bitwidth(&root, &store, HAMT_BIT_WIDTH).unwrap();

    // expect to find a trees key
    assert!(outer_map_raw.get(&"tree".key()).unwrap().is_some());
    // expect NOT to find a rock key as inner keys all removed and flush won't write inner maps without any keys
    assert!(outer_map_raw.get(&"rock".key()).unwrap().is_none());

    // load mapmap again and check keys
    let mut mm_reloaded: MapMap<MemoryBlockstore, String, &str, &str> =
        MapMap::from_root(&store, &root, HAMT_BIT_WIDTH, HAMT_BIT_WIDTH).unwrap();
    let mut count = 0;
    mm_reloaded
        .for_each("tree", |bk, v| -> anyhow::Result<()> {
            count += 1;
            assert!(
                (bk == &"deciduous".key() && v == &"mango".to_string())
                    || (bk == &"evergreen".key() && v == &"larch".to_string())
            );
            Ok(())
        })
        .unwrap();
    assert_eq!(2, count);
}
