// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    car_backed_blockstore::CompressedCarV1BackedBlockstore,
    daemon::bundle::get_actors_bundle,
    networks::{ChainConfig, Height, NetworkChain},
    state_migration::run_state_migrations,
};
use anyhow::*;
use cid::Cid;
use pretty_assertions::assert_eq;
use std::{str::FromStr, sync::Arc};

#[tokio::test]
async fn test_nv17_state_migration_calibnet() -> Result<()> {
    // forest_filecoin::state_migration: State migration at height Shark(epoch 16800) was successful,
    // Previous state: bafy2bzacedxtdhqjsrw2twioyaeomdk4z7umhgfv36vzrrotjb4woutphqgyg,
    // new state: bafy2bzacecrejypa2rqdh3geg2u3qdqdrejrfqvh2ykqcrnyhleehpiynh4k4. Took: 1.4302154s.
    //
    // See <https://github.com/ChainSafe/forest/actions/runs/5579505385/jobs/10195488001#step:6:232>
    test_state_migration(
        Height::Shark,
        NetworkChain::Calibnet,
        Cid::from_str("bafy2bzacedxtdhqjsrw2twioyaeomdk4z7umhgfv36vzrrotjb4woutphqgyg")?,
        Cid::from_str("bafy2bzacecrejypa2rqdh3geg2u3qdqdrejrfqvh2ykqcrnyhleehpiynh4k4")?,
    )
    .await
}

async fn test_state_migration(
    height: Height,
    network: NetworkChain,
    old_state: Cid,
    expected_new_state: Cid,
) -> Result<()> {
    let store = Arc::new(CompressedCarV1BackedBlockstore::new(
        std::io::BufReader::new(std::fs::File::open(format!(
            "/home/me/fr/snapshots/calibnet/{old_state}.car.zst"
        ))?),
    )?);
    let chain_config = Arc::new(match network {
        NetworkChain::Calibnet => ChainConfig::calibnet(),
        NetworkChain::Mainnet => ChainConfig::mainnet(),
        _ => unimplemented!(),
    });
    let height_info = &chain_config.height_infos[height as usize];

    fvm_ipld_car::load_car(
        &store,
        get_actors_bundle(
            &{
                let mut config = crate::Config::default();
                config.chain = chain_config.clone();
                config
            },
            height,
        )
        .await?,
    )
    .await?;

    let new_state = run_state_migrations(height_info.epoch, &chain_config, &store, &old_state)?;

    assert_eq!(new_state, Some(expected_new_state));

    Ok(())
}
