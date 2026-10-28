// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Lotus Miner creating a miner and pledging a sector through Forest's RPC. Mirrors Lotus
//! `testNonGenesisMiner` and `TestPledgeSectors` (single sector, real 2 KiB proofs).

use super::{
    MinerApi, ensure_lotus_accepts_forest_head, genesis_miner, miner_info, sector_info,
    wait_until_proving,
};
use crate::dev::subcommands::tests_cmd::helpers::*;
use crate::key_management::generate_key;
use crate::rpc::Client;
use crate::rpc::prelude::*;
use crate::rpc::types::ApiTipsetKey;
use crate::shim::address::Address;
use crate::shim::crypto::SignatureType;
use crate::shim::sector::SectorSize;
use ahash::HashSet;
use anyhow::{Context as _, bail, ensure};
use libtest_mimic::Trial;
use std::str::FromStr as _;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn tests() -> Vec<Trial> {
    vec![
        trial("onboarding::create_miner", || block_on(create_miner())),
        trial("onboarding::pledge_sector", || block_on(pledge_sector())),
    ]
}

async fn list_miners(client: &Client) -> anyhow::Result<HashSet<Address>> {
    Ok(client
        .call(StateListMiners::request((ApiTipsetKey(None),))?)
        .await?
        .into_iter()
        .collect())
}

async fn create_miner() -> anyhow::Result<()> {
    let forest = forest_client()?;
    let lotus = lotus_client()?;
    let owner_key = Address::from_str(&FOREST_TEST_PRELOADED_ADDRESS)
        .context("parsing FOREST_TEST_PRELOADED_ADDRESS")?;
    let owner_id = forest
        .call(StateLookupID::request((owner_key, ApiTipsetKey(None)))?)
        .await?;
    // The miner actor requires a BLS worker; the harness wallet is secp256k1.
    let worker_key = generate_key(SignatureType::Bls)?.address;

    // Lotus trails Forest; query both at Lotus's head so the tipset exists on Forest.
    let lotus_head = lotus.call(ChainHead::request(())?).await?;
    let at_lotus_head = ApiTipsetKey(Some(lotus_head.key().clone()));
    let deposit = StateMinerCreationDeposit::request((at_lotus_head.clone(),))?;
    let forest_deposit = forest.call(deposit.clone()).await?;
    let lotus_deposit = lotus.call(deposit).await?;
    ensure!(
        forest_deposit == lotus_deposit,
        "StateMinerCreationDeposit at epoch {}: forest {forest_deposit}, lotus {lotus_deposit}",
        lotus_head.epoch()
    );
    // The deposit is the per-byte cap here, so a supply disagreement does not change it.
    let supply = StateVMCirculatingSupplyInternal::request((at_lotus_head,))?;
    let forest_supply = forest.call(supply.clone()).await?;
    let lotus_supply = lotus.call(supply).await?;
    ensure!(
        forest_supply == lotus_supply,
        "StateVMCirculatingSupplyInternal at epoch {}:\nforest: {forest_supply:?}\nlotus: {lotus_supply:?}",
        lotus_head.epoch()
    );

    let before = list_miners(&forest).await?;
    let repo = format!(
        "/tmp/forest-onboarding-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis()
    );
    let init = init_miner(&repo, owner_key, worker_key);
    let _ = docker(&["exec", "lotus-miner", "rm", "-rf", &repo]);
    init?;

    let created: Vec<Address> = list_miners(&forest)
        .await?
        .difference(&before)
        .copied()
        .collect();
    let [miner] = created.as_slice() else {
        bail!("expected exactly one new miner after `lotus-miner init`, found {created:?}");
    };

    let worker_id = forest
        .call(StateLookupID::request((worker_key, ApiTipsetKey(None)))?)
        .await?;
    let info = miner_info(&forest, *miner).await?;
    ensure!(
        info.owner == owner_id && info.worker == worker_id,
        "miner {miner}: owner {} and worker {}, expected {owner_id} and {worker_id}",
        info.owner,
        info.worker
    );
    ensure!(
        info.sector_size == SectorSize::_2KiB,
        "miner {miner}: sector size {:?}, expected 2 KiB",
        info.sector_size
    );
    ensure!(
        !info.peer_id.is_empty(),
        "miner {miner}: no peer ID recorded"
    );

    poll(&format!("lotus StateGetActor {miner}"), || async {
        ensure_lotus_accepts_forest_head(&forest, &lotus).await?;
        get_actor(&lotus, *miner).await
    })
    .await?;
    let lotus_info = miner_info(&lotus, *miner).await?;
    ensure!(
        lotus_info == info,
        "StateMinerInfo for {miner} differs:\nforest: {info:?}\nlotus: {lotus_info:?}"
    );
    Ok(())
}

fn init_miner(repo: &str, owner: Address, worker: Address) -> anyhow::Result<()> {
    let port = harness_env("FOREST_RPC_PORT")?;
    let data_dir = harness_env("FOREST_DATA_DIR")?;
    let script = format!(
        "export LOTUS_MINER_PATH='{repo}' \
         FULLNODE_API_INFO=\"$(cat '{data_dir}/token.jwt'):/dns/forest/tcp/{port}/http\" && \
         exec lotus-miner init --owner='{owner}' --worker='{worker}' --sector-size=2KiB \
         --no-local-storage --nosync"
    );
    docker(&["exec", "lotus-miner", "sh", "-c", &script])
        .context("`lotus-miner init` against Forest failed")?;
    Ok(())
}

/// Pledges one CC sector; the chain verifies the seal proof on prove-commit.
async fn pledge_sector() -> anyhow::Result<()> {
    let forest = forest_client()?;
    let lotus = lotus_client()?;
    let miner_api = MinerApi::new()?;
    let miner = genesis_miner()?;
    let sector = miner_api.pledge_sector().await?;

    wait_until_proving(&miner_api, sector).await?;

    let forest_info = sector_info(&forest, miner, sector)
        .await?
        .with_context(|| format!("sector {sector} is Proving, but Forest has no info for it"))?;
    let lotus_info = poll(
        &format!("lotus StateSectorGetInfo {miner} {sector}"),
        || async {
            ensure_lotus_accepts_forest_head(&forest, &lotus).await?;
            sector_info(&lotus, miner, sector).await
        },
    )
    .await?;
    ensure!(
        forest_info == lotus_info,
        "StateSectorGetInfo for sector {sector} differs:\nforest: {forest_info:?}\nlotus: {lotus_info:?}"
    );
    Ok(())
}
