// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::utils::db::{car_stream::CarHeader, car_util::load_car};
use anyhow::Context as _;
use fvm_ipld_blockstore::Blockstore;

pub async fn load_actor_bundles(db: &impl Blockstore) -> anyhow::Result<CarHeader> {
    const ERROR_MESSAGE: &str = "Actor bundles assets are not properly downloaded, make sure git-lfs is installed and run `git lfs pull` again. See <https://github.com/git-lfs/git-lfs/blob/main/INSTALLING.md>";

    const ACTOR_BUNDLES_CAR_ZST: &[u8] = include_bytes!("../../assets/actor_bundles.car.zst");

    load_car(db, ACTOR_BUNDLES_CAR_ZST)
        .await
        .context(ERROR_MESSAGE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::networks::ACTOR_BUNDLES;
    use ahash::HashSet;
    use cid::Cid;
    use pretty_assertions::assert_eq;

    #[tokio::test]
    async fn test_load_actor_bundles() {
        let db = fvm_ipld_blockstore::MemoryBlockstore::new();
        let roots = HashSet::from_iter(load_actor_bundles(&db).await.unwrap().roots);
        let roots_expected: HashSet<Cid> = ACTOR_BUNDLES.iter().map(|b| b.manifest).collect();
        assert_eq!(roots, roots_expected);
    }
}
