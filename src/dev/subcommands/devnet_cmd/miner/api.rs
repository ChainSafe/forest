// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! The miner the trials drive: a `lotus-miner` whose full node is Forest, reached through the
//! Lotus Miner JSON-RPC API, which Forest's RPC does not define.

use crate::dev::subcommands::tests_cmd::helpers::{harness_env, poll};
use crate::rpc::{ApiPaths, Client, Request, humanize_rpc_error};
use crate::shim::address::Address;
use crate::utils::UrlFromMultiAddr;
use anyhow::{Context as _, bail};
use itertools::Itertools as _;
use serde::Deserialize;
use serde::de::{DeserializeOwned, IgnoredAny};
use serde_json::json;
use std::str::FromStr as _;

/// A `lotus-miner` whose full node is Forest: its actor on chain and its API.
pub(super) struct TestMiner {
    pub actor: Address,
    pub api: MinerApi,
}

impl TestMiner {
    /// Returns the genesis miner the compose devnet runs, from `MINER_ACTOR_ADDRESS` and `MINER_API_INFO`.
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

/// The Lotus Miner API (`/rpc/v0`) with the admin token that flushing the sealing batches needs.
pub(super) struct MinerApi(Client);

impl MinerApi {
    /// Connects to `MINER_API_INFO` (`<token>:<multiaddr>`, the shape of Lotus's own variable).
    pub fn connect() -> anyhow::Result<Self> {
        let raw = harness_env("MINER_API_INFO")?;
        let UrlFromMultiAddr(url) = raw.parse().context("parsing MINER_API_INFO")?;
        Ok(Self(Client::from_url(url)))
    }

    async fn call<T: DeserializeOwned>(
        &self,
        method: &'static str,
        params: serde_json::Value,
    ) -> anyhow::Result<T> {
        let result: serde_json::Value = self
            .0
            .call(Request::new(method, params, ApiPaths::V0))
            .await
            .with_context(|| format!("Lotus Miner {method}"))
            .map_err(humanize_rpc_error)?;
        serde_json::from_value(result)
            .with_context(|| format!("decoding the Lotus Miner {method} result"))
    }

    /// Starts sealing a new CC sector and returns its number.
    pub async fn pledge_sector(&self) -> anyhow::Result<u64> {
        let id: SectorId = self.call("Filecoin.PledgeSector", json!([])).await?;
        Ok(id.number)
    }

    async fn sectors_status(&self, sector: u64) -> anyhow::Result<SectorStatus> {
        self.call("Filecoin.SectorsStatus", json!([sector, false]))
            .await
    }

    /// Flushes the pre-commit and commit batches, which Lotus may otherwise hold for up to a day.
    async fn flush_sealing_batches(&self) -> anyhow::Result<()> {
        for method in [
            "Filecoin.SectorPreCommitFlush",
            "Filecoin.SectorCommitFlush",
        ] {
            self.call::<IgnoredAny>(method, json!([])).await?;
        }
        Ok(())
    }

    /// Waits until `sector` is `Proving`, flushing the sealing batches on every poll.
    ///
    /// # Errors
    /// The sector enters a state that never leads to `Proving`, or the poll budget passes; both
    /// errors carry the sector log, whose trace lines hold Lotus's error text.
    pub async fn wait_until_proving(&self, sector: u64) -> anyhow::Result<()> {
        let waited = poll(&format!("sector {sector} to reach Proving"), || async {
            self.flush_sealing_batches().await?;
            let status = self.sectors_status(sector).await?;
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
        let Ok(status) = self.sectors_status(sector).await else {
            return Err(e);
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
        Err(e.context(format!(
            "sector {sector} is in {:?}\nsector log:\n{log}",
            status.state
        )))
    }
}

/// Lotus `abi.SectorID`
/// (<https://github.com/filecoin-project/go-state-types/blob/v0.19.0/abi/sector.go>).
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SectorId {
    number: u64,
}

/// The fields of Lotus `api.SectorInfo` these tests read
/// (<https://github.com/filecoin-project/lotus/blob/v1.38.3/api/api_storage.go#L272-L305>).
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SectorStatus {
    state: SectorState,
    log: Vec<SectorLog>,
}

/// Lotus `api.SectorLog` (<https://github.com/filecoin-project/lotus/blob/v1.38.3/api/api_storage.go#L243-L250>):
/// `message` is the marshalled event, `trace` the error text when the event carried one.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SectorLog {
    kind: String,
    message: String,
    trace: String,
}

/// Lotus `api.SectorState`
/// (<https://github.com/filecoin-project/lotus/blob/v1.38.3/storage/pipeline/sector_state.go#L74-L157>).
/// Unknown values fail to decode.
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
        assert_eq!(status.log[0].trace, "boom");
    }
}
