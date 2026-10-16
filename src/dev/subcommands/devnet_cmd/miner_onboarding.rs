// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Lotus Miner creating a miner and pledging a sector through Forest's RPC. Mirrors Lotus
//! `testNonGenesisMiner` and `TestPledgeSectors` (single sector, real 2 KiB proofs).

use crate::dev::subcommands::tests_cmd::helpers::*;
use crate::rpc::Client;
use crate::rpc::prelude::*;
use crate::rpc::types::{ApiTipsetKey, SectorOnChainInfo};
use crate::shim::actors::miner::MinerInfo;
use crate::shim::address::Address;
use crate::shim::sector::SectorSize;
use ahash::HashSet;
use anyhow::{Context as _, bail, ensure};
use libtest_mimic::{Arguments, Failed, Trial};
use std::str::FromStr as _;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const FOREST_MINER_PROJECT: &str = "devnet-forest-miner";
/// Poll once per block so batch flushes land before the next status check.
const BLOCK_TIME: Duration = Duration::from_secs(4);
/// 2 KiB seal needs several minutes (seed wait + message confidence).
const SEALING_TIMEOUT: Duration = Duration::from_secs(600);

/// Miner creation and sector pledge tests with Lotus Miner on Forest
#[derive(Debug, clap::Args)]
pub struct MinerOnboardingTestCommand {}

impl MinerOnboardingTestCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        ensure_forest_miner_devnet()?;
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

/// On `scripts/devnet` Lotus Miner talks to Lotus, so this suite would pass without hitting Forest.
fn ensure_forest_miner_devnet() -> anyhow::Result<()> {
    let project = docker(&[
        "inspect",
        "-f",
        r#"{{index .Config.Labels "com.docker.compose.project"}}"#,
        "lotus-miner",
    ])
    .context("could not inspect the `lotus-miner` container; is the Forest-miner devnet up?")?;
    ensure!(
        project == FOREST_MINER_PROJECT,
        "`lotus-miner` belongs to compose project `{project}`, not `{FOREST_MINER_PROJECT}`; \
         this suite needs the Forest-miner devnet (scripts/devnet-forest-miner)"
    );
    Ok(())
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
    let genesis_owner = miner_info(&forest, genesis_miner()?).await?.owner;
    let owner_key = forest
        .call(StateAccountKey::request((
            genesis_owner,
            ApiTipsetKey(None),
        ))?)
        .await?;

    // Lotus trails Forest; query both at Lotus's head so the tipset exists on Forest.
    let lotus_head = lotus.call(ChainHead::request(())?).await?;
    let at_lotus_head = ApiTipsetKey(Some(lotus_head.key().clone()));
    let forest_deposit = forest
        .call(StateMinerCreationDeposit::request(
            (at_lotus_head.clone(),),
        )?)
        .await?;
    let lotus_deposit = lotus
        .call(StateMinerCreationDeposit::request((at_lotus_head,))?)
        .await?;
    ensure!(
        forest_deposit == lotus_deposit,
        "StateMinerCreationDeposit at epoch {}: forest {forest_deposit}, lotus {lotus_deposit}",
        lotus_head.epoch()
    );

    let before = list_miners(&forest).await?;
    let repo = format!(
        "/tmp/forest-onboarding-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis()
    );
    let init = init_miner(&repo, owner_key);
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

    let info = miner_info(&forest, *miner).await?;
    ensure!(
        info.owner == genesis_owner && info.worker == genesis_owner,
        "miner {miner}: owner {} and worker {} should both be {genesis_owner}",
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

    poll_until_actor_on("lotus", *miner, lotus_client).await?;
    let lotus_info = miner_info(&lotus, *miner).await?;
    ensure!(
        lotus_info == info,
        "StateMinerInfo for {miner} differs:\nforest: {info:?}\nlotus: {lotus_info:?}"
    );
    Ok(())
}

fn init_miner(repo: &str, owner: Address) -> anyhow::Result<()> {
    let port = std::env::var("FOREST_RPC_PORT")
        .context("FOREST_RPC_PORT not set; source the devnet test harness")?;
    let repo_env = format!("LOTUS_MINER_PATH={repo}");
    let api_env = format!(
        "FULLNODE_API_INFO={}:/dns/forest/tcp/{port}/http",
        forest_api_token()?
    );
    let owner_flag = format!("--owner={owner}");
    docker(&[
        "exec",
        "-e",
        &repo_env,
        "-e",
        &api_env,
        "lotus-miner",
        "lotus-miner",
        "init",
        &owner_flag,
        "--sector-size=2KiB",
        "--no-local-storage",
        "--nosync",
    ])
    .context("`lotus-miner init` against Forest failed")?;
    Ok(())
}

/// Pledges one CC sector; the chain verifies the seal proof on prove-commit.
async fn pledge_sector() -> anyhow::Result<()> {
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

    let forest_info = sector_info(&forest_client()?, miner, sector)
        .await?
        .with_context(|| format!("sector {sector} is Proving, but Forest has no info for it"))?;
    let lotus_info = poll(
        &format!("lotus StateSectorGetInfo {miner} {sector}"),
        || async { sector_info(&lotus_client()?, miner, sector).await },
    )
    .await?;
    ensure!(
        forest_info == lotus_info,
        "StateSectorGetInfo for sector {sector} differs:\nforest: {forest_info:?}\nlotus: {lotus_info:?}"
    );
    Ok(())
}

/// Flush precommit/commit batches each attempt: Lotus Miner waits up to `24h` before publishing.
async fn wait_until_proving(sector: u64) -> anyhow::Result<()> {
    let sector_arg = sector.to_string();
    let label = format!("sector {sector} to reach Proving");
    poll_every(&label, BLOCK_TIME, SEALING_TIMEOUT, || async {
        // Non-zero exit with "no sectors to publish" is expected while nothing is batched.
        let _ = lotus_miner_exec(&["sectors", "batching", "precommit", "--publish-now"]);
        let _ = lotus_miner_exec(&["sectors", "batching", "commit", "--publish-now"]);
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
        Ok(matches!(state, "Proving" | "Available").then_some(()))
    })
    .await
}
