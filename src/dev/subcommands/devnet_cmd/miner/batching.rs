// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Lotus
//! [`TestPledgeBatching`](https://github.com/filecoin-project/lotus/blob/v1.39.1/itests/sector_pledge_test.go)
//! seal loop (real 2 KiB proofs), reduced to 4 sectors (`MinAggregatedSectors`).
//! Flushes only when all sectors are ready (full batch). Then #7791 Forest checks.

use super::api::{SectorState, TestMiner, enrich_wait_error, ensure_still_sealing};
use super::devnet::{At, Devnet};
use crate::dev::subcommands::tests_cmd::helpers::{block_on, poll, trial};
use crate::rpc::prelude::*;
use crate::rpc::types::ApiTipsetKey;
use crate::shim::actors::miner::qa_power_max;
use crate::shim::address::Address;
use crate::shim::fvm_shared_latest::error::ExitCode;
use crate::shim::sector::SectorSize;
use ahash::HashMap;
use anyhow::{Context as _, ensure};
use cid::Cid;
use fil_actor_miner_state::v19::{Method, PreCommitSectorBatchParams2, ProveCommitSectors3Params};
use libtest_mimic::Trial;
use num::BigInt;
use parking_lot::Mutex;

/// Lotus `MinAggregatedSectors`.
const BATCH_SECTORS: usize = 4;

pub(super) fn tests() -> Vec<Trial> {
    vec![trial("batching::pledge_batch", || block_on(pledge_batch()))]
}

async fn pledge_batch() -> anyhow::Result<()> {
    let net = Devnet::connect()?;
    let miner = TestMiner::genesis()?;

    let at = net.synced().await?;
    let before = at
        .same(|tsk| StateMinerPower::request((miner.actor, tsk)))
        .await?;

    let mut sectors = Vec::with_capacity(BATCH_SECTORS);
    for _ in 0..BATCH_SECTORS {
        sectors.push(miner.api.pledge_sector().await?);
    }
    eprintln!("pledged sectors {sectors:?}");

    // Flush full batches first (before Proving) so the gas mutation can estimate a pending commit.
    let (pre_cid, commit_cid) = wait_seal_batches(&miner, &sectors).await?;
    eprintln!("pre-commit batch: {pre_cid}");
    eprintln!("commit aggregate: {commit_cid}");

    let lookup = net.executed(pre_cid).await?;
    ensure!(
        lookup.receipt.exit_code().is_success(),
        "PreCommitSectorBatch2 {pre_cid} exited {}",
        lookup.receipt.exit_code()
    );
    // Immediately after pre is on-chain (StateCall has no mpool), before decode work,
    // while commit is still pending (post-inclusion StateCall can exit non-OOG).
    assert_halved_gas_estimate_fails(&net, commit_cid).await?;
    assert_pre_commit_batch(&net, pre_cid, &sectors).await?;

    let lookup = net.executed(commit_cid).await?;
    ensure!(
        lookup.receipt.exit_code().is_success(),
        "ProveCommitSectors3 {commit_cid} exited {}",
        lookup.receipt.exit_code()
    );
    assert_prove_commit_aggregate(&net, commit_cid, &sectors).await?;

    wait_sectors_proving(&miner, &sectors).await?;
    let at = net.synced().await?;
    for &sector in &sectors {
        at.same(|tsk| StateSectorGetInfo::request((miner.actor, sector, tsk)))
            .await?
            .with_context(|| format!("sector {sector} is Proving, but chain has no info"))?;
    }

    let raw_delta = BigInt::from(SectorSize::_2KiB as u64) * BigInt::from(BATCH_SECTORS);
    let qap_delta = qa_power_max(SectorSize::_2KiB.into()) * BigInt::from(BATCH_SECTORS);
    let expected_raw = &before.miner_power.raw_byte_power + &raw_delta;
    let expected_qap = &before.miner_power.quality_adj_power + &qap_delta;
    let sample = *sectors.first().context("no sectors")?;
    let at = wait_for_power(&net, miner.actor, sample, &expected_raw).await?;
    let after = at
        .same(|tsk| StateMinerPower::request((miner.actor, tsk)))
        .await?;
    ensure!(
        after.miner_power.raw_byte_power == expected_raw,
        "raw power {}, expected {expected_raw}",
        after.miner_power.raw_byte_power
    );
    ensure!(
        after.miner_power.quality_adj_power == expected_qap,
        "QA power {}, expected {expected_qap} (10× post-NV29)",
        after.miner_power.quality_adj_power
    );
    Ok(())
}

/// Flush when all sectors are in the submit state; return CIDs before Proving.
async fn wait_seal_batches(miner: &TestMiner, sectors: &[u64]) -> anyhow::Result<(Cid, Cid)> {
    let n_sectors = sectors.len();
    let last = Mutex::new(HashMap::<u64, SectorState>::default());
    let pre = Mutex::new(None::<Cid>);
    let commit = Mutex::new(None::<Cid>);

    let waited = poll(
        &format!("sectors {sectors:?} full PreCommit and Commit flushes"),
        || async {
            let mut counts = HashMap::<SectorState, usize>::default();
            for &sector in sectors {
                let status = miner.api.sectors_status(sector).await?;
                note_sector_state(&last, sector, status.state);
                ensure_still_sealing(sector, status.state, "a batch flush")?;
                *counts.entry(status.state).or_insert(0) += 1;
            }

            let n = |s| counts.get(&s).copied().unwrap_or(0);
            if pre.lock().is_none() && n(SectorState::SubmitPreCommitBatch) == n_sectors {
                for batch in miner.api.pre_commit_flush().await? {
                    eprintln!("PRECOMMIT BATCH: {:?}", batch.sectors);
                    if batch.error.is_empty()
                        && same_sectors(&batch.sectors, sectors)
                        && let Some(cid) = batch.msg()
                    {
                        *pre.lock() = Some(cid);
                    }
                }
            }

            if commit.lock().is_none() && n(SectorState::SubmitCommitAggregate) == n_sectors {
                for batch in miner.api.commit_flush().await? {
                    eprintln!("COMMIT BATCH: {:?}", batch.sectors);
                    if batch.error.is_empty()
                        && batch.failed_sectors.is_empty()
                        && same_sectors(&batch.sectors, sectors)
                        && let Some(cid) = batch.msg()
                    {
                        *commit.lock() = Some(cid);
                    }
                }
            }

            Ok((pre.lock().is_some() && commit.lock().is_some()).then_some(()))
        },
    )
    .await;
    enrich_wait_error(&miner.api, sectors, waited).await?;

    let pre = pre
        .into_inner()
        .context("no PreCommit batch covering all pledged sectors")?;
    let commit = commit
        .into_inner()
        .context("no Commit aggregate covering all pledged sectors")?;
    Ok((pre, commit))
}

/// Wait until every sector is `Proving` or `Available` (do not flush again).
async fn wait_sectors_proving(miner: &TestMiner, sectors: &[u64]) -> anyhow::Result<()> {
    let last = Mutex::new(HashMap::<u64, SectorState>::default());
    let waited = poll(&format!("sectors {sectors:?} to reach Proving"), || async {
        let mut n_ready = 0usize;
        for &sector in sectors {
            let status = miner.api.sectors_status(sector).await?;
            note_sector_state(&last, sector, status.state);
            ensure_still_sealing(sector, status.state, "Proving")?;
            if matches!(status.state, SectorState::Proving | SectorState::Available) {
                n_ready += 1;
            }
        }
        Ok((n_ready == sectors.len()).then_some(()))
    })
    .await;
    enrich_wait_error(&miner.api, sectors, waited).await
}

fn note_sector_state(last: &Mutex<HashMap<u64, SectorState>>, sector: u64, state: SectorState) {
    let mut last = last.lock();
    if last.insert(sector, state) != Some(state) {
        eprintln!("sector {sector} state: {state:?}");
    }
}

fn same_sectors(got: &[u64], want: &[u64]) -> bool {
    let mut got = got.to_vec();
    let mut want = want.to_vec();
    got.sort_unstable();
    want.sort_unstable();
    got == want
}

async fn assert_pre_commit_batch(net: &Devnet, cid: Cid, sectors: &[u64]) -> anyhow::Result<()> {
    let at = net.synced().await?;
    let message = at.same(|_| ChainGetMessage::request((cid,))).await?;
    ensure!(
        message.method_num() == Method::PreCommitSectorBatch2 as u64,
        "{cid} method {}, expected PreCommitSectorBatch2",
        message.method_num()
    );
    let params: PreCommitSectorBatchParams2 = message
        .params()
        .deserialize()
        .with_context(|| format!("decoding PreCommitSectorBatch2 params of {cid}"))?;
    let got: Vec<u64> = params.sectors.iter().map(|s| s.sector_number).collect();
    ensure!(
        same_sectors(&got, sectors),
        "PreCommitSectorBatch2 {cid} sectors {got:?}, expected {sectors:?}"
    );
    Ok(())
}

async fn assert_prove_commit_aggregate(
    net: &Devnet,
    cid: Cid,
    sectors: &[u64],
) -> anyhow::Result<()> {
    let at = net.synced().await?;
    let message = at.same(|_| ChainGetMessage::request((cid,))).await?;
    ensure!(
        message.method_num() == Method::ProveCommitSectors3 as u64,
        "{cid} method {}, expected ProveCommitSectors3",
        message.method_num()
    );
    let params: ProveCommitSectors3Params = message
        .params()
        .deserialize()
        .with_context(|| format!("decoding ProveCommitSectors3 params of {cid}"))?;
    let got: Vec<u64> = params
        .sector_activations
        .iter()
        .map(|s| s.sector_number)
        .collect();
    ensure!(
        same_sectors(&got, sectors),
        "ProveCommitSectors3 {cid} sectors {got:?}, expected {sectors:?}"
    );
    ensure!(
        !params.aggregate_proof.is_empty(),
        "ProveCommitSectors3 {cid} has empty aggregate proof"
    );
    Ok(())
}

/// #7791: half of Forest's gas estimate must OOG under `StateCall` (not mpool).
///
/// Call right after pre-commit executes (StateCall ignores mpool pending) and while the
/// aggregate is still pending. `GasEstimateGasLimit` always probes the mpool tipset.
async fn assert_halved_gas_estimate_fails(net: &Devnet, cid: Cid) -> anyhow::Result<()> {
    let message = net
        .forest
        .call(ChainGetMessage::request((cid,))?)
        .await
        .context("ChainGetMessage of aggregate commit")?;
    let tipset = ApiTipsetKey(None);
    let estimate = net
        .forest
        .call(GasEstimateGasLimit::request((
            message.clone(),
            tipset.clone(),
        ))?)
        .await
        .context("GasEstimateGasLimit of aggregate commit")?;
    ensure!(estimate >= 2, "GasEstimateGasLimit returned {estimate}");
    let half = estimate / 2;
    let starved = message
        .into_builder()
        .gas_limit(u64::try_from(half)?)
        .build();
    let call = net
        .forest
        .call(StateCall::request((starved, tipset))?)
        .await
        .context("StateCall of aggregate commit with half gas")?;
    let receipt = call
        .msg_rct
        .context("no receipt for halved-gas StateCall")?;
    ensure!(
        receipt.exit_code() == ExitCode::SYS_OUT_OF_GAS,
        "StateCall gas {half} (half of {estimate}) exited {} `{}`; expected SYS_OUT_OF_GAS",
        receipt.exit_code(),
        call.error
    );
    eprintln!(
        "halved-gas mutation: estimate {estimate}, limit {half}, exit {}",
        receipt.exit_code()
    );
    Ok(())
}

/// Waits until raw power reaches `target`, waiting through the sector's deadline if needed.
async fn wait_for_power<'a>(
    net: &'a Devnet,
    miner: Address,
    sector: u64,
    target: &BigInt,
) -> anyhow::Result<At<'a>> {
    let at = net.synced().await?;
    let power = at
        .same(|tsk| StateMinerPower::request((miner, tsk)))
        .await?;
    if power.miner_power.raw_byte_power >= *target {
        return Ok(at);
    }

    let location = at
        .same(|tsk| StateSectorPartition::request((miner, sector, tsk)))
        .await?;
    let current = at
        .same(|tsk| StateMinerProvingDeadline::request((miner, tsk)))
        .await?
        .0;
    let deadline = current.for_index(location.deadline).next_not_elapsed();
    eprintln!(
        "raw power {}; waiting for sector {sector} deadline {} close at {}",
        power.miner_power.raw_byte_power, location.deadline, deadline.close
    );
    net.wait_for_epoch(deadline.close).await?;
    poll(
        &format!("raw power {target} after deadline {}", location.deadline),
        || async {
            let at = net.synced().await?;
            let power = at
                .same(|tsk| StateMinerPower::request((miner, tsk)))
                .await?;
            Ok((power.miner_power.raw_byte_power >= *target).then_some(at))
        },
    )
    .await
}
