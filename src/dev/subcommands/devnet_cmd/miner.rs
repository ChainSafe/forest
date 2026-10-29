// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Lotus Miner integration tests, ported from Lotus `itests`. They need
//! `scripts/devnet-forest-miner`, where Lotus Miner seals and mines through Forest's RPC.

mod onboarding;

use crate::dev::subcommands::tests_cmd::helpers::*;
use crate::rpc::prelude::*;
use crate::rpc::types::{ApiTipsetKey, SectorOnChainInfo};
use crate::rpc::{ApiPaths, Client, DEFAULT_REQUEST_TIMEOUT, Request, humanize_rpc_error};
use crate::shim::actors::miner::MinerInfo;
use crate::shim::address::Address;
use anyhow::{Context as _, bail, ensure};
use clap::ValueEnum as _;
use itertools::Itertools as _;
use libtest_mimic::{Arguments, Trial};
use serde::Deserialize;
use serde::de::{DeserializeOwned, IgnoredAny};
use serde_json::json;
use std::marker::PhantomData;
use std::str::FromStr as _;

/// Lotus Miner integration tests on Forest (`scripts/devnet-forest-miner` only)
#[derive(Debug, clap::Args)]
pub struct MinerTestCommand {
    /// Run only this group; all groups run by default
    #[arg(long)]
    group: Option<Group>,
    /// Run only the tests whose name contains this string
    filter: Option<String>,
}

/// Tests that share one devnet. CI runs each group as a separate job on its own devnet.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum Group {
    Onboarding,
}

impl MinerTestCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let groups = match self.group {
            Some(group) => vec![group],
            None => Group::value_variants().to_vec(),
        };
        let args = Arguments {
            filter: self.filter,
            test_threads: Some(1),
            ..Default::default()
        };
        libtest_mimic::run(&args, groups.into_iter().flat_map(tests).collect()).exit();
    }
}

fn tests(group: Group) -> Vec<Trial> {
    match group {
        Group::Onboarding => onboarding::tests(),
    }
}

fn genesis_miner() -> anyhow::Result<Address> {
    let raw = harness_env("MINER_ACTOR_ADDRESS")?;
    Address::from_str(&raw).with_context(|| format!("invalid MINER_ACTOR_ADDRESS `{raw}`"))
}

async fn miner_info(client: &Client, miner: Address) -> anyhow::Result<MinerInfo> {
    Ok(client
        .call(StateMinerInfo::request((miner, ApiTipsetKey(None)))?)
        .await?)
}

/// `Ok(true)` if Lotus shares Forest's head tipset and has not marked it bad; `Ok(false)` if Lotus still trails.
async fn lotus_accepts_forest_head(forest: &Client, lotus: &Client) -> anyhow::Result<bool> {
    let head = forest.call(ChainHead::request(())?).await?;
    let lotus_head = lotus.call(ChainHead::request(())?).await?;
    if lotus_head.epoch() < head.epoch() {
        return Ok(false);
    }
    let lotus_at_height = lotus
        .call(ChainGetTipSetByHeight::request((
            head.epoch(),
            ApiTipsetKey(None),
        ))?)
        .await?;
    ensure!(
        lotus_at_height.key() == head.key(),
        "Lotus tipset at Forest head epoch {} differs: forest {}, lotus {}",
        head.epoch(),
        head.key(),
        lotus_at_height.key()
    );
    for cid in head.cids() {
        let reason = lotus.call(SyncCheckBad::request((cid,))?).await?;
        ensure!(
            reason.is_empty(),
            "Lotus rejected Forest block {cid} at epoch {}: {reason}",
            head.epoch()
        );
    }
    Ok(true)
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

/// The Lotus Miner API, which Forest's RPC does not define.
struct MinerApi(Client);

impl MinerApi {
    /// Connects with an admin token, which flushing sealing batches needs.
    fn new() -> anyhow::Result<Self> {
        let port = harness_env("MINER_RPC_PORT")?;
        let token = lotus_miner_exec(&["auth", "create-token", "--perm", "admin"])?;
        let mut url: url::Url = format!("http://127.0.0.1:{port}/").parse()?;
        url.set_password(Some(token.as_str()))
            .map_err(|()| anyhow::anyhow!("cannot set the Lotus Miner token on {url}"))?;
        Ok(Self(Client::from_url(url)))
    }

    async fn call<T: DeserializeOwned>(
        &self,
        method: &'static str,
        params: serde_json::Value,
    ) -> anyhow::Result<T> {
        let request = Request::<serde_json::Value> {
            method_name: method.into(),
            params,
            result_type: PhantomData,
            api_path: ApiPaths::V0,
            timeout: *DEFAULT_REQUEST_TIMEOUT,
        };
        let result = self
            .0
            .call(request)
            .await
            .with_context(|| format!("Lotus Miner {method}"))
            .map_err(humanize_rpc_error)?;
        serde_json::from_value(result)
            .with_context(|| format!("decoding the Lotus Miner {method} result"))
    }

    /// Starts sealing a new CC sector and returns its number.
    async fn pledge_sector(&self) -> anyhow::Result<u64> {
        let id: SectorId = self.call("Filecoin.PledgeSector", json!([])).await?;
        Ok(id.number)
    }

    async fn sectors_status(&self, sector: u64) -> anyhow::Result<SectorStatus> {
        self.call("Filecoin.SectorsStatus", json!([sector, false]))
            .await
    }

    /// Flush pre-commit/commit batches (Lotus may hold them for up to a day).
    async fn flush_sealing_batches(&self) -> anyhow::Result<()> {
        for method in [
            "Filecoin.SectorPreCommitFlush",
            "Filecoin.SectorCommitFlush",
        ] {
            self.call::<IgnoredAny>(method, json!([])).await?;
        }
        Ok(())
    }
}

/// Lotus `abi.SectorID`.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SectorId {
    number: u64,
}

/// The fields of Lotus `api.SectorInfo` these tests read.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SectorStatus {
    state: SectorState,
    last_err: String,
    log: Vec<SectorLog>,
}

/// Lotus `api.SectorState`. Unknown Lotus values fail to decode.
#[derive(Debug, Deserialize)]
enum SectorState {
    /// A new sector, before Lotus processes its first event.
    #[serde(rename = "")]
    Undefined,
    Empty,
    WaitDeals,
    AddPiece,
    Packing,
    GetTicket,
    PreCommit1,
    PreCommit2,
    PreCommitting,
    PreCommitWait,
    SubmitPreCommitBatch,
    PreCommitBatchWait,
    WaitSeed,
    Committing,
    CommitFinalize,
    CommitFinalizeFailed,
    SubmitCommit,
    CommitWait,
    SubmitCommitAggregate,
    CommitAggregateWait,
    FinalizeSector,
    Proving,
    Available,

    SnapDealsWaitDeals,
    SnapDealsAddPiece,
    SnapDealsPacking,
    UpdateReplica,
    ProveReplicaUpdate,
    SubmitReplicaUpdate,
    WaitMutable,
    ReplicaUpdateWait,
    FinalizeReplicaUpdate,
    UpdateActivating,
    ReleaseSectorKey,

    ReceiveSector,

    FailedUnrecoverable,
    AddPieceFailed,
    SealPreCommit1Failed,
    SealPreCommit2Failed,
    PreCommitFailed,
    ComputeProofFailed,
    RemoteCommitFailed,
    CommitFailed,
    PackingFailed,
    FinalizeFailed,
    DealsExpired,
    RecoverDealIDs,

    SnapDealsAddPieceFailed,
    SnapDealsDealsExpired,
    SnapDealsRecoverDealIDs,
    AbortUpgrade,
    ReplicaUpdateFailed,
    ReleaseSectorKeyFailed,
    FinalizeReplicaUpdateFailed,

    Faulty,
    FaultReported,
    FaultedFinal,

    Terminating,
    TerminateWait,
    TerminateFinality,
    TerminateFailed,

    Removing,
    RemoveFailed,
    Removed,
}

/// Lotus `api.SectorLog`.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SectorLog {
    kind: String,
    message: String,
}

async fn wait_until_proving(miner_api: &MinerApi, sector: u64) -> anyhow::Result<()> {
    let waited = poll(&format!("sector {sector} to reach Proving"), || async {
        miner_api.flush_sealing_batches().await?;
        let status = miner_api.sectors_status(sector).await?;
        eprintln!("sector {sector} state: {:?}", status.state);
        match status.state {
            SectorState::Proving | SectorState::Available => Ok(Some(())),
            // Never reaches Proving from these.
            SectorState::FailedUnrecoverable
            | SectorState::PackingFailed
            | SectorState::DealsExpired
            | SectorState::Faulty
            | SectorState::FaultReported
            | SectorState::FaultedFinal
            | SectorState::Terminating
            | SectorState::TerminateWait
            | SectorState::TerminateFinality
            | SectorState::TerminateFailed
            | SectorState::Removing
            | SectorState::RemoveFailed
            | SectorState::Removed => bail!(
                "sector {sector} reached {:?}, which never leads to Proving",
                status.state
            ),
            _ => Ok(None),
        }
    })
    .await;
    let Err(e) = waited else {
        return Ok(());
    };
    let Ok(status) = miner_api.sectors_status(sector).await else {
        return Err(e);
    };
    let log = status
        .log
        .iter()
        .map(|l| format!("{} {}", l.kind, l.message))
        .join("\n");
    Err(e.context(format!(
        "sector {sector} is in {:?}, last error: {:?}\nsector log:\n{log}",
        status.state, status.last_err
    )))
}
