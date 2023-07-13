use crate::{
    car_backed_blockstore::CarBackedBlockstore,
    networks::{ChainConfig, Height},
    state_migration::run_state_migrations,
};
use anyhow::*;
use cid::Cid;
use pretty_assertions::assert_eq;
use std::{str::FromStr, sync::Arc};

#[test]
fn test_nv17_state_migration_calibnet() -> Result<()> {
    let store = Arc::new(CarBackedBlockstore::new(std::fs::File::open(
        // "/home/me/fr/snapshots/calibnet/bafy2bzacedxtdhqjsrw2twioyaeomdk4z7umhgfv36vzrrotjb4woutphqgyg.car",
        "/home/me/fr/snapshots/calibnet/shark-test.car",
    )?)?);
    let chain_config = Arc::new(ChainConfig::calibnet());
    let height_info = &chain_config.height_infos[Height::Shark as usize];
    let new_state = run_state_migrations(
        height_info.epoch,
        &chain_config,
        &store,
        &Cid::from_str("bafy2bzacedxtdhqjsrw2twioyaeomdk4z7umhgfv36vzrrotjb4woutphqgyg")?,
    )?;
    assert_eq!(
        new_state,
        Some(Cid::from_str(
            "bafy2bzacecrejypa2rqdh3geg2u3qdqdrejrfqvh2ykqcrnyhleehpiynh4k4"
        )?)
    );

    Ok(())
}
