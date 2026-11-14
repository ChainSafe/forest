// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! The miner the trials drive: a `lotus-miner` whose full node is Forest, reached through the
//! Lotus Miner JSON-RPC API, which Forest's RPC does not define.

use crate::dev::subcommands::tests_cmd::helpers::{harness_env, poll};
use crate::lotus_json::LotusJson;
use crate::rpc::{ApiPaths, Client, Request};
use crate::shim::address::Address;
use crate::utils::UrlFromMultiAddr;
use ahash::HashMap;
use anyhow::{Context as _, ensure};
use cid::Cid;
use itertools::Itertools as _;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;
use std::str::FromStr as _;
use std::sync::atomic::{AtomicU8, Ordering};

/// The miner's on-chain actor and its API.
pub(super) struct TestMiner {
    pub actor: Address,
    pub api: MinerApi,
}

impl TestMiner {
    /// Returns the genesis miner the compose devnet runs.
    pub fn genesis() -> anyhow::Result<Self> {
        let raw = harness_env("MINER_ACTOR_ADDRESS")?;
        let actor = Address::from_str(&raw)
            .with_context(|| format!("invalid MINER_ACTOR_ADDRESS `{raw}`"))?;
        Ok(Self {
            actor,
            api: MinerApi::connect()?,
        })
    }
}

/// The Lotus Miner API; its token must be admin, since the batch flushes are admin methods.
pub(super) struct MinerApi(Client);

impl MinerApi {
    /// Connects to `MINER_API_INFO`.
    pub fn connect() -> anyhow::Result<Self> {
        let raw = harness_env("MINER_API_INFO")?;
        let UrlFromMultiAddr(url) = raw.parse().context("parsing MINER_API_INFO")?;
        Ok(Self(Client::from_url(url)))
    }

    /// Retries brief connection drops under `PoRep` load.
    async fn call<T: DeserializeOwned>(
        &self,
        method: &'static str,
        params: serde_json::Value,
    ) -> anyhow::Result<T> {
        const ATTEMPTS: u32 = 5;
        let mut last_err = None;
        for attempt in 1..=ATTEMPTS {
            match super::call(
                &self.0,
                "Lotus Miner",
                Request::new(method, params.clone(), ApiPaths::V0),
            )
            .await
            {
                Ok(result) => {
                    return serde_json::from_value(result)
                        .with_context(|| format!("decoding the Lotus Miner {method} result"));
                }
                Err(e) if attempt < ATTEMPTS && is_transient_rpc(&e) => {
                    eprintln!(
                        "Lotus Miner {method} transient failure on attempt {attempt}/{ATTEMPTS}: {e:#}"
                    );
                    last_err = Some(e);
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
                Err(e) => return Err(e),
            }
        }
        Err(last_err.expect("retry loop left no error"))
    }

    /// Starts sealing a new CC sector and returns its number.
    pub async fn pledge_sector(&self) -> anyhow::Result<u64> {
        let id: SectorId = self.call("Filecoin.PledgeSector", json!([])).await?;
        Ok(id.number)
    }

    pub(super) async fn sectors_status(&self, sector: u64) -> anyhow::Result<SectorStatus> {
        self.call("Filecoin.SectorsStatus", json!([sector, false]))
            .await
    }

    /// Flushes the pre-commit and commit batches, which Lotus may otherwise hold for up to a day
    /// (<https://github.com/filecoin-project/lotus/blob/v1.38.3/node/config/def.go#L129-L137>).
    async fn flush_sealing_batches(&self) -> anyhow::Result<()> {
        let _ = self.pre_commit_flush().await?;
        let _ = self.commit_flush().await?;
        Ok(())
    }

    /// Lotus returns JSON `null` when nothing was sent.
    pub async fn pre_commit_flush(&self) -> anyhow::Result<Vec<PreCommitBatchRes>> {
        let batches: Option<Vec<_>> = self
            .call("Filecoin.SectorPreCommitFlush", json!([]))
            .await?;
        Ok(batches.unwrap_or_default())
    }

    /// Lotus returns JSON `null` when nothing was sent.
    pub async fn commit_flush(&self) -> anyhow::Result<Vec<CommitBatchRes>> {
        let batches: Option<Vec<_>> = self.call("Filecoin.SectorCommitFlush", json!([])).await?;
        Ok(batches.unwrap_or_default())
    }

    /// Waits until `sector` is `Proving` or `Available`, flushing the sealing batches on every
    /// poll; a state that never leads to `Proving` fails at once.
    pub async fn wait_until_proving(&self, sector: u64) -> anyhow::Result<()> {
        // `poll` logs every attempt, so the state is printed only when it changes.
        let last_state = AtomicU8::new(u8::MAX);
        let waited = poll(&format!("sector {sector} to reach Proving"), || async {
            self.flush_sealing_batches().await?;
            let status = self.sectors_status(sector).await?;
            if last_state.swap(status.state as u8, Ordering::Relaxed) != status.state as u8 {
                eprintln!("sector {sector} state: {:?}", status.state);
            }
            ensure_still_sealing(sector, status.state, "Proving")?;
            Ok(matches!(status.state, SectorState::Proving | SectorState::Available).then_some(()))
        })
        .await;
        enrich_wait_error(self, &[sector], waited).await
    }
}

fn is_transient_rpc(err: &anyhow::Error) -> bool {
    let msg = format!("{err:#}").to_lowercase();
    msg.contains("connection")
        || msg.contains("reset by peer")
        || msg.contains("broken pipe")
        || msg.contains("temporarily unavailable")
}

/// Fail-fast on Lotus `*Fail*` states and other terminals that never reach `goal`.
pub(super) fn ensure_still_sealing(
    sector: u64,
    state: SectorState,
    goal: &str,
) -> anyhow::Result<()> {
    ensure!(
        !state.is_failed(),
        "sector {sector} in a failed state: {state:?}"
    );
    ensure!(
        !state.is_terminal_unhappy(),
        "sector {sector} reached {state:?}, which never leads to {goal}"
    );
    Ok(())
}

pub(super) async fn enrich_wait_error(
    api: &MinerApi,
    sectors: &[u64],
    waited: anyhow::Result<()>,
) -> anyhow::Result<()> {
    let Err(e) = waited else {
        return Ok(());
    };
    let mut details = Vec::new();
    for &sector in sectors {
        let Ok(status) = api.sectors_status(sector).await else {
            continue;
        };
        let log = status
            .log
            .iter()
            .map(|l| {
                if l.trace.is_empty() {
                    format!("{} {}", l.kind, l.message)
                } else {
                    format!("{} {}\n{}", l.kind, l.message, l.trace)
                }
            })
            .join("\n");
        details.push(format!(
            "sector {sector} is in {:?}\nsector log:\n{log}",
            status.state
        ));
    }
    if details.is_empty() {
        return Err(e);
    }
    Err(e.context(details.join("\n")))
}

/// Lotus `abi.SectorID`
/// (<https://github.com/filecoin-project/go-state-types/blob/v0.19.0/abi/sector.go>).
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SectorId {
    number: u64,
}

/// Lotus `sealiface.PreCommitBatchRes`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct PreCommitBatchRes {
    pub sectors: Vec<u64>,
    pub msg: Option<LotusJson<Cid>>,
    #[serde(default)]
    pub error: String,
}

impl PreCommitBatchRes {
    pub fn msg(&self) -> Option<Cid> {
        self.msg.as_ref().map(|c| c.clone().into_inner())
    }
}

/// Lotus `sealiface.CommitBatchRes`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct CommitBatchRes {
    pub sectors: Vec<u64>,
    #[serde(default)]
    pub failed_sectors: HashMap<String, String>,
    pub msg: Option<LotusJson<Cid>>,
    #[serde(default)]
    pub error: String,
}

impl CommitBatchRes {
    pub fn msg(&self) -> Option<Cid> {
        self.msg.as_ref().map(|c| c.clone().into_inner())
    }
}

/// The fields of Lotus `api.SectorInfo` these tests read
/// (<https://github.com/filecoin-project/lotus/blob/v1.38.3/api/api_storage.go#L272-L305>).
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct SectorStatus {
    pub state: SectorState,
    log: Vec<SectorLog>,
}

/// Lotus `api.SectorLog` (<https://github.com/filecoin-project/lotus/blob/v1.38.3/api/api_storage.go#L243-L250>);
/// `trace` is the error text when the event carried one.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SectorLog {
    kind: String,
    message: String,
    trace: String,
}

/// Lotus `api.SectorState`
/// (<https://github.com/filecoin-project/lotus/blob/v1.38.3/storage/pipeline/sector_state.go#L74-L157>).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub(super) enum SectorState {
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

impl SectorState {
    /// States whose Lotus name contains `Fail`. The batching wait uses these to fail fast.
    pub(super) fn is_failed(self) -> bool {
        matches!(
            self,
            Self::FailedUnrecoverable
                | Self::AddPieceFailed
                | Self::SealPreCommit1Failed
                | Self::SealPreCommit2Failed
                | Self::PreCommitFailed
                | Self::ComputeProofFailed
                | Self::RemoteCommitFailed
                | Self::CommitFailed
                | Self::PackingFailed
                | Self::FinalizeFailed
                | Self::CommitFinalizeFailed
                | Self::SnapDealsAddPieceFailed
                | Self::ReplicaUpdateFailed
                | Self::ReleaseSectorKeyFailed
                | Self::FinalizeReplicaUpdateFailed
                | Self::TerminateFailed
                | Self::RemoveFailed
        )
    }

    /// Terminal states that never reach `Proving`, aside from names containing `Fail`.
    pub(super) fn is_terminal_unhappy(self) -> bool {
        matches!(
            self,
            Self::DealsExpired
                | Self::Faulty
                | Self::FaultReported
                | Self::FaultedFinal
                | Self::Terminating
                | Self::TerminateWait
                | Self::TerminateFinality
                | Self::Removing
                | Self::Removed
                | Self::SnapDealsDealsExpired
                | Self::AbortUpgrade
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sector_status_decodes_the_lotus_shape() {
        let status: SectorStatus = serde_json::from_value(json!({
            "State": "",
            "LastErr": "",
            "Log": [{"Kind": "event;sealing.SectorCommitFailed", "Timestamp": 1, "Trace": "boom", "Message": "{\"User\":{}}"}],
        }))
        .unwrap();
        assert!(matches!(status.state, SectorState::Undefined));
        assert_eq!(status.log.first().unwrap().trace, "boom");
    }

    #[test]
    fn batch_flush_results_decode_the_lotus_shape() {
        let cid_str = "bafy2bzacedhl5cowfmp7hq2ke3irf7indry55px44cbbjthazwvpm7ktf676w";
        let want = Cid::from_str(cid_str).unwrap();
        let pre: Vec<PreCommitBatchRes> = serde_json::from_value(json!([{
            "Sectors": [2, 3],
            "Msg": {"/": cid_str},
            "Error": "",
        }]))
        .unwrap();
        let pre = pre.first().unwrap();
        assert_eq!(pre.sectors, vec![2, 3]);
        assert_eq!(pre.msg(), Some(want));
        assert!(pre.error.is_empty());

        let commit: Vec<CommitBatchRes> = serde_json::from_value(json!([{
            "Sectors": [2, 3, 4, 5],
            "FailedSectors": {},
            "Msg": {"/": cid_str},
            "Error": "",
        }]))
        .unwrap();
        let commit = commit.first().unwrap();
        assert_eq!(commit.sectors, vec![2, 3, 4, 5]);
        assert!(commit.failed_sectors.is_empty());
        assert_eq!(commit.msg(), Some(want));
        assert!(commit.error.is_empty());
    }

    #[test]
    fn batch_flush_results_decode_null_msg_error_and_failed_sectors() {
        let pre: Vec<PreCommitBatchRes> = serde_json::from_value(json!([{
            "Sectors": [1],
            "Msg": null,
            "Error": "held",
        }]))
        .unwrap();
        let pre = pre.first().unwrap();
        assert!(pre.msg().is_none());
        assert_eq!(pre.error, "held");

        let commit: Vec<CommitBatchRes> = serde_json::from_value(json!([{
            "Sectors": [1],
            "FailedSectors": {"1": "boom"},
            "Msg": null,
            "Error": "",
        }]))
        .unwrap();
        let commit = commit.first().unwrap();
        assert!(commit.msg().is_none());
        assert_eq!(
            commit.failed_sectors.get("1").map(String::as_str),
            Some("boom")
        );
    }
}
