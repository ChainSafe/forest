// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::Result;
use cid::{multihash::MultihashDigest, Cid};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use forest_db::{rocks::RocksDb, rocks_config::RocksDbConfig};
use fvm_ipld_blockstore::Blockstore;
use rand::{rngs::OsRng, RngCore};
use rocksdb::DB;

fn rockdb_bench(c: &mut Criterion) {
    rockdb_bench_inner(c).unwrap();
}

fn rockdb_bench_inner(c: &mut Criterion) -> Result<()> {
    const PARTITION: &str = "extra";
    const RECORD_BYTES: usize = 2048;
    const N_RECORD: usize = 100000;
    const N_LOOPUP: usize = 100;

    let db1_dir = tempfile::tempdir()?.into_path();
    let db1_opt = {
        let mut config = RocksDbConfig::default();
        config.enable_statistics = true;
        config.to_options()
    };
    let db1 = {
        let db = DB::open(&db1_opt, &db1_dir).unwrap();
        RocksDb::from_raw_db(db)
    };
    let db2_dir = tempfile::tempdir()?.into_path();
    let db2_opt = {
        let mut config = RocksDbConfig::default();
        config.enable_statistics = true;
        config.to_options()
    };
    let db2 = {
        let db = DB::open_cf(&db2_opt, &db2_dir, vec![PARTITION]).unwrap();
        RocksDb::from_raw_db(db)
    };
    for _i in 0..N_RECORD {
        let mut rec = [0_u8; RECORD_BYTES];
        OsRng.fill_bytes(&mut rec);
        let mh = cid::multihash::Code::Sha2_256.digest(&rec);
        let cid = Cid::new_v0(mh)?;
        db1.put_keyed(&cid, &rec)?;
        db2.put_keyed(&cid, &rec)?;
    }
    println!("DB1 size: {}B", fs_extra::dir::get_size(&db1_dir)?);
    println!("DB2 size: {}B", fs_extra::dir::get_size(&db2_dir)?);
    // println!(
    //     "DB1 stats:\n{}",
    //     db1_opt.get_statistics().unwrap_or_default()
    // );
    // println!(
    //     "DB2 stats:\n{}",
    //     db2_opt.get_statistics().unwrap_or_default()
    // );
    // c.bench_function(
    //     "block serialization: 3NKaBJsN1SehD6iJwRwJSFmVzJg5DXSUQVgnMxtH4eer4aF5BrDK",
    //     |b| {
    //         let block = TEST_BLOCKS
    //             .get("3NKaBJsN1SehD6iJwRwJSFmVzJg5DXSUQVgnMxtH4eer4aF5BrDK.hex")
    //             .unwrap();
    //         let et = block.external_transitionv1().unwrap();
    //         b.iter(|| {
    //             let mut output: Vec<u8> = Vec::new();
    //             bin_prot::to_writer(&mut output, black_box(&et)).unwrap();
    //             output
    //         })
    //     },
    // );
    Ok(())
}

criterion_group!(benches, rockdb_bench);
criterion_main!(benches);
