// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Lotus Miner creating a miner and pledging a sector through Forest's RPC. Mirrors Lotus
//! `testNonGenesisMiner` and `TestPledgeSectors` (single sector, real 2 KiB proofs).

use crate::dev::subcommands::tests_cmd::helpers::*;
use crate::key_management::generate_key;
use crate::rpc::Client;
use crate::rpc::prelude::*;
use crate::rpc::types::{ApiTipsetKey, SectorOnChainInfo};
use crate::shim::actors::miner::MinerInfo;
use crate::shim::address::Address;
use crate::shim::crypto::SignatureType;
use crate::shim::sector::SectorSize;
use ahash::HashSet;
use anyhow::{Context as _, bail, ensure};
use libtest_mimic::{Arguments, Failed, Trial};
use std::str::FromStr as _;
use std::time::{SystemTime, UNIX_EPOCH};

const FOREST_MINER_PROJECT: &str = "devnet-forest-miner";

/// Miner creation and sector pledge tests with Lotus Miner on Forest
#[derive(Debug, clap::Args)]
pub struct MinerOnboardingTestCommand {}

impl MinerOnboardingTestCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        if !forest_miner_devnet()? {
            println!("skipped: miner-onboarding runs on scripts/devnet-forest-miner");
            return Ok(());
        }
        let args = Arguments {
            test_threads: Some(1),
            ..Default::default()
        };
        libtest_mimic::run(&args, tests()).exit();
    }
}

fn tests() -> Vec<Trial> {
    fn trial(name: &'static str, body: fn() -> anyhow::Result<()>) -> Trial {
        Trial::test(name, move || {
            body().map_err(|e| Failed::from(format!("{e:?}")))
        })
    }

    vec![
        trial("create_miner", || block_on(create_miner())),
        trial("pledge_sector", || block_on(pledge_sector())),
    ]
}

/// Skip `scripts/devnet`: its `lotus-miner` seals through Lotus, not Forest.
fn forest_miner_devnet() -> anyhow::Result<bool> {
    let project = docker(&[
        "inspect",
        "-f",
        r#"{{index .Config.Labels "com.docker.compose.project"}}"#,
        "lotus-miner",
    ])
    .context("could not inspect the `lotus-miner` container; is the devnet up?")?;
    Ok(project == FOREST_MINER_PROJECT)
}

fn genesis_miner() -> anyhow::Result<Address> {
    let raw = std::env::var("MINER_ACTOR_ADDRESS")
        .context("MINER_ACTOR_ADDRESS not set; source the devnet test harness")?;
    Address::from_str(&raw).with_context(|| format!("invalid MINER_ACTOR_ADDRESS `{raw}`"))
}

async fn miner_info(client: &Client, miner: Address) -> anyhow::Result<MinerInfo> {
    Ok(client
        .call(StateMinerInfo::request((miner, ApiTipsetKey(None)))?)
        .await?)
}

async fn list_miners(client: &Client) -> anyhow::Result<HashSet<Address>> {
    Ok(client
        .call(StateListMiners::request((ApiTipsetKey(None),))?)
        .await?
        .into_iter()
        .collect())
}

/// Lotus also marks every descendant of a rejected block bad, so checking Forest's head catches
/// an earlier split.
async fn ensure_lotus_accepts_forest_head(forest: &Client, lotus: &Client) -> anyhow::Result<()> {
    let head = forest.call(ChainHead::request(())?).await?;
    for cid in head.cids() {
        let reason = lotus.call(SyncCheckBad::request((cid,))?).await?;
        ensure!(
            reason.is_empty(),
            "Lotus rejected Forest block {cid} at epoch {}: {reason}",
            head.epoch()
        );
    }
    Ok(())
}

async fn sector_info(
    client: &Client,
    miner: Address,
    sector: u64,
) -> anyhow::Result<Option<SectorOnChainInfo>> {
    Ok(client
        .call(StateSectorGetInfo::request((
            miner,
            sector,
            ApiTipsetKey(None),
        ))?)
        .await?)
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
    let forest_deposit = forest
        .call(StateMinerCreationDeposit::request(
            (at_lotus_head.clone(),),
        )?)
        .await?;
    let lotus_deposit = lotus
        .call(StateMinerCreationDeposit::request(
            (at_lotus_head.clone(),),
        )?)
        .await?;
    ensure!(
        forest_deposit == lotus_deposit,
        "StateMinerCreationDeposit at epoch {}: forest {forest_deposit}, lotus {lotus_deposit}",
        lotus_head.epoch()
    );
    // The deposit is the per-byte cap here, so a supply disagreement does not change it.
    let forest_supply = forest
        .call(StateVMCirculatingSupplyInternal::request((
            at_lotus_head.clone(),
        ))?)
        .await?;
    let lotus_supply = lotus
        .call(StateVMCirculatingSupplyInternal::request((at_lotus_head,))?)
        .await?;
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
    let port = std::env::var("FOREST_RPC_PORT")
        .context("FOREST_RPC_PORT not set; source the devnet test harness")?;
    let data_dir = std::env::var("FOREST_DATA_DIR")
        .context("FOREST_DATA_DIR not set; source the devnet test harness")?;
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
    let miner = genesis_miner()?;
    let out = lotus_miner_exec(&["sectors", "pledge"])?;
    let sector: u64 = out
        .lines()
        .find_map(|l| l.trim().strip_prefix("Created CC sector:"))
        .with_context(|| format!("no `Created CC sector:` in pledge output:\n{out}"))?
        .trim()
        .parse()
        .context("parsing the pledged sector number")?;

    wait_until_proving(sector).await?;

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

/// Flush only once this one sector is queued.
async fn wait_until_proving(sector: u64) -> anyhow::Result<()> {
    let sector_arg = sector.to_string();
    let label = format!("sector {sector} to reach Proving");
    poll(&label, || async {
        let status = lotus_miner_exec(&["sectors", "status", &sector_arg])?;
        let state = status
            .lines()
            .find_map(|l| l.trim().strip_prefix("Status:"))
            .map(str::trim)
            .with_context(|| format!("no `Status:` in sector status output:\n{status}"))?;
        eprintln!("sector {sector} state: {state}");
        if state.contains("Fail") {
            let log = lotus_miner_exec(&["sectors", "status", "--log", &sector_arg])
                .unwrap_or_else(|e| format!("{e:#}"));
            bail!("sector {sector} entered failed state {state}:\n{log}");
        }
        match state {
            "SubmitPreCommitBatch" => {
                publish_batch(&["sectors", "batching", "precommit", "--publish-now"])?;
            }
            "SubmitCommitAggregate" => {
                publish_batch(&["sectors", "batching", "commit", "--publish-now"])?;
            }
            _ => {}
        }
        Ok(matches!(state, "Proving" | "Available").then_some(()))
    })
    .await
}

/// `--publish-now` exits with "no sectors to publish" when the flush API returns no batch.
/// Lotus treats that nil result as nothing sent.
fn publish_batch(args: &[&str]) -> anyhow::Result<()> {
    match lotus_miner_exec(args) {
        Err(e) if format!("{e:#}").contains("no sectors to publish") => Ok(()),
        other => other.map(|_| ()),
    }
}
