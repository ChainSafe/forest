// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::Result;
use cid::{multihash::MultihashDigest, Cid};
use criterion::{criterion_group, criterion_main, Criterion};
use forest_db::{rocks::RocksDb, rocks_config::RocksDbConfig, Store};
use fvm_ipld_blockstore::Blockstore;
use rand::{rngs::StdRng, seq::SliceRandom, RngCore, SeedableRng};
use rocksdb::DB;
use std::rc::Rc;

fn rockdb_bench(c: &mut Criterion) {
    rockdb_bench_inner(c).unwrap();
}

fn rockdb_bench_inner(c: &mut Criterion) -> Result<()> {
    const ENABLE_STATS: bool = false;
    const PARTITION: &str = "extra";
    const RECORD_BYTES: usize = 2048;
    const N_RECORD: usize = 2000;
    const N_LOOKUP: usize = 200;
    const CACHE_KEY_PREFIX: &[u8] = b"block_val/";

    let tmp_dir1 = tempfile::tempdir()?;
    let tmp_dir2 = tempfile::tempdir()?;
    {
        let db_opt = {
            let mut config = RocksDbConfig::default();
            config.enable_statistics = ENABLE_STATS;
            config.max_open_files = -1;
            config.compaction_style = "none".into();
            config.compression_type = "none".into();
            config.block_size = 2048;
            config.optimize_for_point_lookup = 64;
            config.to_options()
        };

        let db1_dir = tmp_dir1.path();
        let db1 = {
            let db = DB::open(&db_opt, &db1_dir).unwrap();
            Rc::new(RocksDb::from_raw_db(db))
        };
        let db2_dir = tmp_dir2.path();
        let db_cf = {
            let mut db = DB::open(&db_opt, &db2_dir).unwrap();
            if db.cf_handle(PARTITION).is_none() {
                db.create_cf(PARTITION, &db_opt)?;
            }
            Rc::new(RocksDb::from_raw_db(db))
        };
        let mut keys = Vec::with_capacity(N_RECORD);
        let mut rng = StdRng::seed_from_u64(0);
        for _i in 0..N_RECORD {
            let mut rec = [0_u8; RECORD_BYTES];
            rng.fill_bytes(&mut rec);
            let mh = cid::multihash::Code::Blake2b256.digest(&rec);
            let cid = Cid::new_v1(0x71, mh);
            // Write record
            db1.put_keyed(&cid, &rec)?;
            db_cf.put_keyed(&cid, &rec)?;

            // Write cache
            let key = cid.to_bytes();
            let mut cache_key = CACHE_KEY_PREFIX.to_vec();
            cache_key.extend_from_slice(&key);
            db1.write(&cache_key, &[])?;
            db_cf.write_column(&key, &[], PARTITION)?;

            keys.push(key);
        }

        let db1_size = fs_extra::dir::get_size(&db1_dir)?;
        println!("db size (before flush): {db1_size}B");
        let db2_size = fs_extra::dir::get_size(&db2_dir)?;
        println!("db_cf size (before flush): {db2_size}B");

        db1.db.flush()?;
        db_cf.db.flush_cf(db_cf.db.cf_handle(PARTITION).unwrap())?;
        db_cf.db.flush()?;

        let db1_size = fs_extra::dir::get_size(&db1_dir)?;
        println!("db size (after flush): {db1_size}B");
        let db2_size = fs_extra::dir::get_size(&db2_dir)?;
        println!("db_cf size (after flushed): {db2_size}B");

        // return Ok(());

        keys.shuffle(&mut rng);
        let keys = Rc::new(keys);

        c.bench_function("[block lookup hit] db", |b| {
            let db = db1.clone();
            let keys = keys.clone();
            b.iter(move || {
                for k in keys.iter().take(N_LOOKUP) {
                    db.read(k).unwrap().unwrap();
                }
            })
        });

        c.bench_function("[block lookup hit] db_cf", |b| {
            let db = db_cf.clone();
            let keys = keys.clone();
            b.iter(move || {
                for k in keys.iter().take(N_LOOKUP) {
                    db.read(k).unwrap().unwrap();
                }
            })
        });

        c.bench_function("[cache lookup hit] db", |b| {
            let db = db1.clone();
            let keys = keys.clone();
            b.iter(move || {
                for k in keys.iter().take(N_LOOKUP) {
                    let mut cache_key = CACHE_KEY_PREFIX.to_vec();
                    cache_key.extend_from_slice(k);
                    db.exists(&cache_key).unwrap();
                }
            })
        });

        c.bench_function("[cache lookup hit] db_cf", |b| {
            let db = db_cf.clone();
            let keys = keys.clone();
            b.iter(move || {
                for k in keys.iter().take(N_LOOKUP) {
                    db.exists_column(k, PARTITION).unwrap();
                }
            })
        });

        let mut keys_to_miss = Vec::with_capacity(N_LOOKUP);
        for _i in 0..N_LOOKUP {
            let mut rec = [0_u8; RECORD_BYTES];
            rng.fill_bytes(&mut rec);
            let mh = cid::multihash::Code::Blake2b256.digest(&rec);
            let cid = Cid::new_v1(0x71, mh);
            keys_to_miss.push(cid.to_bytes());
        }
        let keys_to_miss = Rc::new(keys_to_miss);

        c.bench_function("[block lookup miss] db", |b| {
            let db = db1.clone();
            let keys = keys_to_miss.clone();
            b.iter(move || {
                for k in keys.iter().take(N_LOOKUP) {
                    db.read(k).unwrap();
                }
            })
        });

        c.bench_function("[block lookup miss] db_cf", |b| {
            let db = db_cf.clone();
            let keys = keys_to_miss.clone();
            b.iter(move || {
                for k in keys.iter().take(N_LOOKUP) {
                    db.read(k).unwrap();
                }
            })
        });

        c.bench_function("[cache lookup miss] db", |b| {
            let db = db1.clone();
            let keys = keys_to_miss.clone();
            b.iter(move || {
                for k in keys.iter().take(N_LOOKUP) {
                    let mut cache_key = CACHE_KEY_PREFIX.to_vec();
                    cache_key.extend_from_slice(k);
                    db.exists(&cache_key).unwrap();
                }
            })
        });

        c.bench_function("[cache lookup miss] db_cf", |b| {
            let db = db_cf.clone();
            let keys = keys_to_miss.clone();
            b.iter(move || {
                for k in keys.iter().take(N_LOOKUP) {
                    db.exists_column(k, PARTITION).unwrap();
                }
            })
        });

        if ENABLE_STATS {
            // println!(
            //     "DB1 stats:\n{}",
            //     db1_opt.get_statistics().unwrap_or_default()
            // );
            // println!(
            //     "DB2 stats:\n{}",
            //     db2_opt.get_statistics().unwrap_or_default()
            // );
        }
    }

    // tmp_dir1.close();
    // tmp_dir2.close();
    Ok(())
}

criterion_group!(benches, rockdb_bench);
criterion_main!(benches);
