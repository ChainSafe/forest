// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Creating a miner and pledging a sector through Forest's RPC (Lotus Miner or Curio).

use super::api::TestMiner;
use super::curio;
use super::devnet::Devnet;
use super::on_curio;
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

async fn create_miner() -> anyhow::Result<()> {
    let net = Devnet::connect()?;
    let owner_key = Address::from_str(&FOREST_TEST_PRELOADED_ADDRESS)
        .context("parsing FOREST_TEST_PRELOADED_ADDRESS")?;
    let owner_id = net
        .forest
        .call(StateLookupID::request((owner_key, ApiTipsetKey(None)))?)
        .await?;
    // The miner actor requires a BLS worker; the harness wallet is secp256k1.
    let worker_key = generate_key(SignatureType::Bls)?.address;

    let at = net.synced().await?;
    at.same(|tsk| StateMinerCreationDeposit::request((tsk,)))
        .await?;
    at.same(|tsk| StateVMCirculatingSupplyInternal::request((tsk,)))
        .await?;

    let before = list_miners(&net.forest).await?;
    let curio = on_curio()?;
    if curio {
        curio::create_miner(owner_key, worker_key)?;
    } else {
        let repo = format!(
            "/tmp/forest-onboarding-{}",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis()
        );
        let init = init_lotus_miner(&repo, owner_key, worker_key);
        let _ = docker(&["exec", "lotus-miner", "rm", "-rf", &repo]);
        init?;
    }

    let created: Vec<Address> = list_miners(&net.forest)
        .await?
        .difference(&before)
        .copied()
        .collect();
    let [miner] = created.as_slice() else {
        bail!("expected exactly one new miner, found {created:?}");
    };

    let worker_id = net
        .forest
        .call(StateLookupID::request((worker_key, ApiTipsetKey(None)))?)
        .await?;
    let info = net
        .synced()
        .await?
        .same(|tsk| StateMinerInfo::request((*miner, tsk)))
        .await?;
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
    if curio {
        ensure!(
            info.peer_id.is_empty(),
            "miner {miner}: Curio should leave peer_id empty, got {}",
            info.peer_id
        );
    } else {
        ensure!(
            !info.peer_id.is_empty(),
            "miner {miner}: no peer ID recorded"
        );
    }
    Ok(())
}

/// Pledges one CC sector; the chain verifies the seal proof on prove-commit.
async fn pledge_sector() -> anyhow::Result<()> {
    let net = Devnet::connect()?;
    let miner = TestMiner::genesis()?;
    let sector = miner.pledge_sector().await?;
    miner.wait_until_proving(sector).await?;
    net.synced()
        .await?
        .same(|tsk| StateSectorGetInfo::request((miner.actor, sector, tsk)))
        .await?
        .with_context(|| format!("sector {sector} is Proving, but the chain has no info for it"))?;
    Ok(())
}

async fn list_miners(client: &Client) -> anyhow::Result<HashSet<Address>> {
    Ok(client
        .call(StateListMiners::request((ApiTipsetKey(None),))?)
        .await?
        .into_iter()
        .collect())
}

fn init_lotus_miner(repo: &str, owner: Address, worker: Address) -> anyhow::Result<()> {
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
