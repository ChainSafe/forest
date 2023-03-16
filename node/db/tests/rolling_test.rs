// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(test)]
#[cfg(any(feature = "paritydb", feature = "rocksdb"))]
mod tests {
    use std::{thread::sleep, time::Duration};

    use anyhow::*;
    use cid::{multihash::MultihashDigest, Cid};
    use forest_db::{rolling::RollingDB, Store};
    use forest_libp2p_bitswap::BitswapStoreRead;
    use fvm_ipld_blockstore::Blockstore;
    use rand::Rng;
    use tempfile::TempDir;

    #[test]
    fn rolling_db_behaviour_tests() -> Result<()> {
        let db_root = TempDir::new()?;
        println!("Creating rolling db under {}", db_root.path().display());
        let rolling_db = RollingDB::load_or_create(db_root.path().into(), Default::default())?;
        println!("Generating random blocks");
        let pairs: Vec<_> = (0..1000)
            .map(|_| {
                let mut bytes = [0; 1024];
                rand::rngs::OsRng.fill(&mut bytes);
                let cid =
                    Cid::new_v0(cid::multihash::Code::Sha2_256.digest(bytes.as_slice())).unwrap();
                (cid, bytes.to_vec())
            })
            .collect();

        let split_index = 500;

        for (i, (k, block)) in pairs.iter().enumerate() {
            if i == split_index {
                sleep(Duration::from_millis(1));
                println!("Creating a new current db");
                rolling_db.next_current()?;
                println!("Created a new current db");
            }
            rolling_db.put_keyed(k, block)?;
        }

        for (i, (k, block)) in pairs.iter().enumerate() {
            ensure!(rolling_db.contains(k)?, "{i}");
            ensure!(
                Blockstore::get(&rolling_db, k)?.unwrap().as_slice() == block,
                "{i}"
            );
        }

        rolling_db.next_current()?;

        for (i, (k, _)) in pairs.iter().enumerate() {
            if i < split_index {
                ensure!(!rolling_db.contains(k)?, "{i}");
            } else {
                ensure!(rolling_db.contains(k)?, "{i}");
            }
        }

        drop(rolling_db);

        let rolling_db = RollingDB::load_or_create(db_root.path().into(), Default::default())?;
        for (i, (k, _)) in pairs.iter().enumerate() {
            if i < split_index {
                ensure!(!rolling_db.contains(k)?);
            } else {
                ensure!(rolling_db.contains(k)?);
            }
        }

        Ok(())
    }
}
