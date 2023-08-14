// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_compression::futures::bufread::ZstdDecoder;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;

pub async fn load_actor_bundles(db: &impl Blockstore) -> anyhow::Result<Vec<Cid>> {
    pub const ACTOR_BUNDLES_CAR_ZST: &[u8] = include_bytes!("../../assets/actor_bundles.car.zst");

    Ok(fvm_ipld_car::load_car(
        db,
        ZstdDecoder::new(futures::io::BufReader::new(ACTOR_BUNDLES_CAR_ZST)),
    )
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::ACTOR_BUNDLES;
    use ahash::HashSet;
    use pretty_assertions::assert_eq;

    #[tokio::test]
    async fn test_load_actor_bundles() {
        let db = fvm_ipld_blockstore::MemoryBlockstore::new();
        let roots = HashSet::from_iter(load_actor_bundles(&db).await.unwrap());
        let roots_expected: HashSet<Cid> = ACTOR_BUNDLES.iter().map(|b| b.manifest).collect();
        assert_eq!(roots, roots_expected);
    }
}
