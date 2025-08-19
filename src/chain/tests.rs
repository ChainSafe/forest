// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::{
    blocks::{CachingBlockHeader, Chain4U, Tipset, TipsetKey, chain4u},
    db::{MemoryDB, car::ForestCar},
    utils::db::CborStoreExt,
};
use sha2::{Digest as _, Sha256};

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

#[tokio::test]
async fn test_export_v1() {
    test_export_inner(FilecoinSnapshotVersion::V1)
        .await
        .unwrap()
}

#[tokio::test]
async fn test_export_v2() {
    test_export_inner(FilecoinSnapshotVersion::V2)
        .await
        .unwrap()
}

async fn test_export_inner(version: FilecoinSnapshotVersion) -> anyhow::Result<()> {
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
    };

    let head_key_cids = nunny::vec![b_5_0.cid(), b_5_1.cid()];
    let head_key = TipsetKey::from(head_key_cids.clone());
    let head = Tipset::load_required(&db, &head_key)?;

    let mut car_bytes = vec![];

    let checksum = match version {
        FilecoinSnapshotVersion::V1 => {
            export::<Sha256>(&db, &head, 0, &mut car_bytes, None).await?
        }
        FilecoinSnapshotVersion::V2 => {
            export_v2::<Sha256>(&db, None, &head, 0, &mut car_bytes, None).await?
        }
    };

    assert_eq!(Sha256::digest(&car_bytes), checksum.unwrap());

    let car = ForestCar::new(car_bytes)?;

    assert_eq!(car.heaviest_tipset()?, head);

    match version {
        FilecoinSnapshotVersion::V1 => {
            assert_eq!(car.metadata(), &None);
        }
        FilecoinSnapshotVersion::V2 => {
            assert_eq!(
                car.metadata(),
                &Some(FilecoinSnapshotMetadata {
                    version,
                    head_tipset_key: head_key_cids,
                    f3_data: None,
                })
            );
        }
    }

    for b in [&genesis, &b_1, &b_2_0, &b_2_1, &b_3, &b_4, &b_5_0, &b_5_1] {
        let b_from_car: CachingBlockHeader = car.get_cbor_required(&b.cid())?;
        let b_from_db: CachingBlockHeader = db.get_cbor_required(&b.cid())?;
        assert_eq!(b_from_car, b_from_db);
    }

    Ok(())
}
