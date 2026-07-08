// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::{
    blocks::{CachingBlockHeader, Chain4U, Tipset, TipsetKey, chain4u},
    cid_collections::CidHashSet,
    db::{MemoryDB, car::ForestCar},
    utils::db::CborStoreExt,
};
use rstest::rstest;
use sha2::{Digest as _, Sha256};
use std::fs::File;
use std::sync::Arc;

#[test]
fn test_snapshot_version_cbor_serde() {
    assert_eq!(
        fvm_ipld_encoding::to_vec(&FilecoinSnapshotVersion::V2),
        fvm_ipld_encoding::to_vec(&2_u64)
    );
    assert_eq!(
        fvm_ipld_encoding::from_slice::<FilecoinSnapshotVersion>(
            &fvm_ipld_encoding::to_vec(&2_u64).unwrap()
        )
        .unwrap(),
        FilecoinSnapshotVersion::V2
    );
}

#[rstest]
#[case(FilecoinSnapshotVersion::V1, true)]
#[case(FilecoinSnapshotVersion::V1, false)]
#[case(FilecoinSnapshotVersion::V2, true)]
#[case(FilecoinSnapshotVersion::V2, false)]
fn test_export(#[case] version: FilecoinSnapshotVersion, #[case] include_tipset_lookup: bool) {
    tokio_test::block_on(test_export_inner(version, include_tipset_lookup)).unwrap()
}

async fn test_export_inner(
    version: FilecoinSnapshotVersion,
    include_tipset_lookup: bool,
) -> anyhow::Result<()> {
    let db = Arc::new(MemoryDB::default());
    let c4u = Chain4U::with_blockstore(db.clone());
    chain4u! {
        in c4u; // select the context
        [genesis]
        -> [b_1]
        -> [b_2_0, b_2_1]
        -> [b_3]
        -> [b_4]
        -> [b_5_0, b_5_1]
        -> [b_6_0]
        -> [b_7_0]
        -> [b_8_0, b_8_1]
        -> [b_9_0]
        -> [b_10_0]
        -> [b_11_0]
        -> [b_12_0]
        -> [b_13_0, b_13_1, b_13_2]
        -> [b_14_0]
        -> [b_15_0]
        -> [b_16_0]
        -> [b_17_0]
        -> [b_18_0]
        -> [b_19_0]
        -> [b_20_0]
        -> [b_21_0]
        -> [b_22_0, b_22_1]
    };

    let head_key_cids = nunny::vec![b_22_0.cid(), b_22_1.cid()];
    let head_key = TipsetKey::from(head_key_cids.clone());
    let head = Tipset::load_required(&db, &head_key)?;
    // Tipset sorts blocks by ticket, so re-derive the canonical CID order from `head`
    // rather than relying on the user-supplied order.
    let head_key_cids = head.key().to_cids();

    let mut car_bytes = vec![];

    let option = ExportOptions::<CidHashSet> {
        include_tipset_lookup,
        ..Default::default()
    };
    let (checksum, tipset_lookup_hamt) = match version {
        FilecoinSnapshotVersion::V1 => {
            export::<Sha256, _>(&db, &head, 0, &mut car_bytes, option).await?
        }
        FilecoinSnapshotVersion::V2 => {
            export_v2::<Sha256, File, _>(&db, None, &head, 0, &mut car_bytes, option).await?
        }
    };

    assert_eq!(Sha256::digest(&car_bytes), checksum.unwrap());

    let car = ForestCar::new(car_bytes)?;

    assert_eq!(car.heaviest_tipset()?, head);

    match version {
        FilecoinSnapshotVersion::V1 => {
            assert_eq!(car.metadata(), None);
        }
        FilecoinSnapshotVersion::V2 => {
            assert_eq!(
                car.metadata(),
                Some(&FilecoinSnapshotMetadata {
                    version,
                    head_tipset_key: head_key_cids,
                    f3_data: None,
                })
            );
        }
    }

    for b in [
        &genesis, &b_1, &b_2_0, &b_2_1, &b_3, &b_4, &b_5_0, &b_5_1, &b_6_0, &b_7_0, &b_8_0, &b_8_1,
        &b_9_0, &b_10_0, &b_11_0, &b_12_0, &b_13_0, &b_13_1, &b_13_2, &b_14_0, &b_15_0, &b_16_0,
        &b_17_0, &b_18_0, &b_19_0, &b_20_0, &b_21_0, &b_22_0, &b_22_1,
    ] {
        let b_from_car: CachingBlockHeader = car.get_cbor_required(&b.cid())?;
        let b_from_db: CachingBlockHeader = db.get_cbor_required(&b.cid())?;
        assert_eq!(b_from_car, b_from_db);
    }

    if include_tipset_lookup {
        let tipset_lookup_hamt = tipset_lookup_hamt
            .context("tipset lookup should be included")?
            .context("tipset lookup should be exported successfully")?;
        assert_eq!(
            tipset_lookup_hamt.iter().count(),
            1,
            "there should be exactly 1 checkpoint exported"
        );
        assert!(
            !tipset_lookup_hamt.contains_key(&0)?,
            "genesis should not be exported"
        );
        assert!(
            !tipset_lookup_hamt.contains_key(&10)?,
            "epoch 10 should not be exported"
        );
        assert!(
            tipset_lookup_hamt.contains_key(&20)?,
            "epoch 20 should be exported"
        );
        assert!(
            !tipset_lookup_hamt.contains_key(&21)?,
            "epoch 21 should not be exported"
        );
    }

    Ok(())
}
