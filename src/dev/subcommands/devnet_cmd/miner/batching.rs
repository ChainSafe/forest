// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Mirrors Lotus `TestPledgeBatching` with real 2 KiB proofs, reduced to 4 sectors
//! (`MinAggregatedSectors`).
//! <https://github.com/filecoin-project/lotus/blob/v1.39.1/itests/sector_pledge_test.go>
//!
//! Flushes a batch only once its queue holds every pledged sector.

use super::api::{SectorState, TestMiner, enrich_wait_error, ensure_still_sealing};
use super::devnet::{At, Devnet};
use crate::dev::subcommands::tests_cmd::helpers::{block_on, poll, trial};
use crate::rpc::prelude::*;
use crate::shim::actors::miner::qa_power_max;
use crate::shim::address::Address;
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

    let mut sectors = Vec::with_capacity(BATCH_SECTORS);
    for _ in 0..BATCH_SECTORS {
        sectors.push(miner.api.pledge_sector().await?);
    }
    eprintln!("pledged sectors {sectors:?}");

    let (pre_cid, commit_cid) = wait_seal_batches(&miner, &sectors).await?;
    eprintln!("pre-commit batch: {pre_cid}");
    eprintln!("commit aggregate: {commit_cid}");

    // New sector power is counted at the deadline, not when the prove-commit is sent. A fault
    // during the seal is already in this snapshot.
    let at = net.synced().await?;
    let powered = at
        .same(|tsk| StateMinerPower::request((miner.actor, tsk)))
        .await?;
    let raw_delta = BigInt::from(SectorSize::_2KiB as u64) * BigInt::from(BATCH_SECTORS);
    let qap_delta = qa_power_max(SectorSize::_2KiB.into()) * BigInt::from(BATCH_SECTORS);
    let expected_raw = &powered.miner_power.raw_byte_power + &raw_delta;
    let expected_qap = &powered.miner_power.quality_adj_power + &qap_delta;

    let lookup = net.executed(pre_cid).await?;
    ensure!(
        lookup.receipt.exit_code().is_success(),
        "PreCommitSectorBatch2 {pre_cid} exited {}",
        lookup.receipt.exit_code()
    );
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
    let epoch = at.epoch();
    for &sector in &sectors {
        let info = at
            .same(|tsk| StateSectorGetInfo::request((miner.actor, sector, tsk)))
            .await?
            .with_context(|| format!("sector {sector} is Proving, but chain has no info"))?;
        ensure!(
            info.activation <= epoch && info.expiration > epoch,
            "sector {sector} is not active at epoch {epoch}: activation {}, expiration {}",
            info.activation,
            info.expiration
        );
    }
    let sample = *sectors.first().context("no sectors")?;
    let at = wait_for_power(&net, miner.actor, sample, &expected_raw).await?;
    let after = at
        .same(|tsk| StateMinerPower::request((miner.actor, tsk)))
        .await?;
    ensure!(
        after.miner_power.raw_byte_power >= expected_raw,
        "raw power {}, expected at least {expected_raw}",
        after.miner_power.raw_byte_power
    );
    ensure!(
        after.miner_power.quality_adj_power >= expected_qap,
        "QA power {}, expected at least {expected_qap} (10× post-NV29)",
        after.miner_power.quality_adj_power
    );
    Ok(())
}

/// Flush once each queue holds every pledged sector.
async fn wait_seal_batches(miner: &TestMiner, sectors: &[u64]) -> anyhow::Result<(Cid, Cid)> {
    let last = Mutex::new(HashMap::<u64, SectorState>::default());
    let pre = Mutex::new(None::<Cid>);
    let commit = Mutex::new(None::<Cid>);

    let waited = poll(
        &format!("sectors {sectors:?} queued for PreCommit and Commit"),
        || async {
            for &sector in sectors {
                let status = miner.api.sectors_status(sector).await?;
                note_sector_state(&last, sector, status.state);
                ensure_still_sealing(sector, status.state, "a batch flush")?;
            }

            // Lotus reports `Submit*` before the sector is queued, so a state count can flush short.
            if pre.lock().is_none() && same_sectors(&miner.api.pre_commit_pending().await?, sectors)
            {
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

            if commit.lock().is_none() && same_sectors(&miner.api.commit_pending().await?, sectors)
            {
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

/// Wait until every sector is `Proving` or `Available`, and do not flush again.
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
