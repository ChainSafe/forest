// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::state_tree::StateRoot;
use crate::{
    daemon::bundle::get_actors_bundle,
    networks::{ChainConfig, Height, NetworkChain},
    state_migration::run_state_migrations,
};
use anyhow::*;
use cid::Cid;
use fvm_ipld_encoding::CborStore;
use pretty_assertions::assert_eq;
use std::path::Path;
use std::{str::FromStr, sync::Arc};
use tokio::io::AsyncWriteExt;

#[ignore = "https://github.com/ChainSafe/forest/issues/2765"]
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

#[ignore = "https://github.com/ChainSafe/forest/issues/2765"]
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

#[ignore = "https://github.com/ChainSafe/forest/issues/2765"]
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
    // Car files are cached under data folder for Go test to pick up without network access
    let car_path = format!("./src/state_migration/tests/data/{old_state}.car");
    if !Path::new(&car_path).is_file() {
        let tmp: tempfile::TempPath = tempfile::NamedTempFile::new()?.into_temp_path();
        {
            let mut reader = crate::utils::net::reader(&format!(
                "https://forest-continuous-integration.fra1.cdn.digitaloceanspaces.com/state_migration/state/{old_state}.car"
            ))
            .await?;
            let mut writer = tokio::io::BufWriter::new(tokio::fs::File::create(&tmp).await?);
            tokio::io::copy(&mut reader, &mut writer).await?;
            writer.shutdown().await?;
        }
        tmp.persist(&car_path)?;
    }

    let store = Arc::new(
        crate::car_backed_blockstore::UncompressedCarV1BackedBlockstore::new(
            std::io::BufReader::new(std::fs::File::open(&car_path)?),
        )?,
    );
    let chain_config = Arc::new(ChainConfig::from_chain(&network));
    let height_info = &chain_config.height_infos[height as usize];

    fvm_ipld_car::load_car(
        &store,
        get_actors_bundle(
            &crate::Config {
                chain: chain_config.clone(),
                ..Default::default()
            },
            height,
        )
        .await?,
    )
    .await?;

    let state_root: StateRoot = store.get_cbor(&old_state)?.unwrap();
    println!("Actor root (for Go test): {}", state_root.actors);

    let new_state = run_state_migrations(height_info.epoch, &chain_config, &store, &old_state)?;

    assert_eq!(new_state, Some(expected_new_state));

    Ok(())
}
