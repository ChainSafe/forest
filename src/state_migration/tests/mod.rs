// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
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
    // new state: bafy2bzacecrejypa2rqdh3geg2u3qdqdrejrfqvh2ykqcrnyhleehpiynh4k4.
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

#[tokio::test]
async fn test_nv18_state_migration_calibnet() -> Result<()> {
    // State migration at height Hygge(epoch 322354) was successful,
    // Previous state: bafy2bzacedjqwdqxlkyyuohmtcfciekl5qh2s4yf67neiuuhkibbteqoucvsm,
    // new state: bafy2bzacedhhgkmr26rbr3yujounnz2ufiwrlvamogyabgfv6uvwq3rlv4t2i.
    //
    // See <https://github.com/ChainSafe/forest/actions/runs/5579505385/jobs/10195488001#step:6:515>
    test_state_migration(
        Height::Hygge,
        NetworkChain::Calibnet,
        Cid::from_str("bafy2bzacedjqwdqxlkyyuohmtcfciekl5qh2s4yf67neiuuhkibbteqoucvsm")?,
        Cid::from_str("bafy2bzacedhhgkmr26rbr3yujounnz2ufiwrlvamogyabgfv6uvwq3rlv4t2i")?,
    )
    .await
}

#[tokio::test]
async fn test_nv19_state_migration_calibnet() -> Result<()> {
    // State migration at height Lightning(epoch 489094) was successful,
    // Previous state: bafy2bzacedgamjgha75e7w2cgklfdgtmumsj7nadqppnpz3wexl2wl6dexsle,
    // new state: bafy2bzacebhjx4uqtg6c65km46wiiq45dbbeckqhs2oontwdzba335nxk6bia.
    //
    // See <https://github.com/ChainSafe/forest/actions/runs/5579505385/jobs/10195488001#step:6:232>
    test_state_migration(
        Height::Lightning,
        NetworkChain::Calibnet,
        Cid::from_str("bafy2bzacedgamjgha75e7w2cgklfdgtmumsj7nadqppnpz3wexl2wl6dexsle")?,
        Cid::from_str("bafy2bzacebhjx4uqtg6c65km46wiiq45dbbeckqhs2oontwdzba335nxk6bia")?,
    )
    .await
}

async fn test_state_migration(
    height: Height,
    network: NetworkChain,
    old_state: Cid,
    expected_new_state: Cid,
) -> Result<()> {
    let store = Arc::new(
        crate::car_backed_blockstore::UncompressedCarV1BackedBlockstore::new(
            std::io::BufReader::new(std::fs::File::open(format!(
                "/home/me/fr/snapshots/calibnet/{old_state}.car"
            ))?),
        )?,
    );
    // TODO: Not working for nv18 and nv19
    // let store = Arc::new(
    //     crate::car_backed_blockstore::CompressedCarV1BackedBlockstore::new(
    //         std::io::BufReader::new(std::fs::File::open(format!(
    //             "/home/me/fr/snapshots/calibnet/{old_state}.car.zst"
    //         ))?),
    //     )?,
    // );
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
