// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::{
    blocks::{Chain4U, chain4u},
    db::MemoryDB,
    networks::ChainConfig,
};

#[tokio::test(flavor = "multi_thread")]
async fn test_indexer_new() {
    let c4u = Chain4U::new();
    chain4u! {
        in c4u;
        t0 @ [_b0]
    };

    let bs = Arc::new(MemoryDB::default());
    let cs = Arc::new(
        ChainStore::new(
            bs.clone(),
            bs.clone(),
            bs,
            Arc::new(ChainConfig::devnet()),
            t0.min_ticket_block().clone(),
        )
        .unwrap(),
    );
    let temp_db_path = tempfile::Builder::new()
        .suffix(".sqlite3")
        .tempfile_in(std::env::temp_dir())
        .unwrap();
    let db = crate::utils::sqlite::open_file(temp_db_path.path())
        .await
        .unwrap();
    SqliteIndexer::new(db, cs, Default::default())
        .await
        .unwrap();
}
