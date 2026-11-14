// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Create-miner + pledge-sector (Lotus `testNonGenesisMiner` / `TestPledgeSectors`).

use super::MinerBackend;
use super::api::TestMiner;
use super::curio;
use super::devnet::Devnet;
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
    let backend = MinerBackend::from_env()?;
    match backend {
        MinerBackend::Curio => curio::create_miner(owner_key, worker_key)?,
        MinerBackend::Lotus => {
            let repo = format!(
                "/tmp/forest-onboarding-{}",
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis()
            );
            let init = init_lotus_miner(&repo, owner_key, worker_key);
            let _ = docker(&["exec", "lotus-miner", "rm", "-rf", &repo]);
            init?;
        }
    }

    // Wait until the new actor shows up (sptool --confidence 1 can still race a tipset).
    let miner = poll("new miner to appear on Forest", || async {
        let created: Vec<Address> = list_miners(&net.forest)
            .await?
            .difference(&before)
            .copied()
            .collect();
        match created.as_slice() {
            [] => Ok(None),
            [miner] => Ok(Some(*miner)),
            other => bail!("expected exactly one new miner, found {other:?}"),
        }
    })
    .await?;

    let worker_id = net
        .forest
        .call(StateLookupID::request((worker_key, ApiTipsetKey(None)))?)
        .await?;
    let at = net.synced().await?;
    let info = at
        .same(|tsk| StateMinerInfo::request((miner, tsk)))
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
    match backend {
        MinerBackend::Curio => ensure!(
            info.peer_id.is_empty(),
            "miner {miner}: Curio should leave peer_id empty, got {} bytes",
            info.peer_id.len()
        ),
        MinerBackend::Lotus => ensure!(
            !info.peer_id.is_empty(),
            "miner {miner}: no peer ID recorded"
        ),
    }
    let power = at
        .same(|tsk| StateMinerPower::request((miner, tsk)))
        .await?;
    ensure!(
        !power.has_min_power,
        "miner {miner}: has the minimum power before pledging a sector"
    );
    Ok(())
}

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
    let api = super::forest_fullnode_api_info_shell()?;
    let script = format!(
        "export LOTUS_MINER_PATH='{repo}' \
         FULLNODE_API_INFO=\"{api}\" && \
         exec lotus-miner init --owner='{owner}' --worker='{worker}' --sector-size=2KiB \
         --no-local-storage --nosync"
    );
    docker(&["exec", "lotus-miner", "sh", "-c", &script])
        .context("`lotus-miner init` against Forest failed")?;
    Ok(())
}
