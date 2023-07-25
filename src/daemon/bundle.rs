// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_compression::futures::bufread::ZstdDecoder;
use fvm_ipld_blockstore::Blockstore;

pub async fn load_actor_bundles(db: &impl Blockstore) -> anyhow::Result<()> {
    pub const ACTOR_BUNDLES_CAR_ZST: &[u8] =
        include_bytes!(concat!(env!("OUT_DIR"), "/actor_bundles.car.zst"));

    fvm_ipld_car::load_car(
        db,
        ZstdDecoder::new(futures::io::BufReader::new(ACTOR_BUNDLES_CAR_ZST)),
    )
    .await?;
    Ok(())
}
