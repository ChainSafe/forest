// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Window PoSt through Forest's RPC: lotus-miner proves the genesis miner's first deadline with
//! live sectors from what Forest answers, and a dispute of the proof, the only check that
//! verifies it, fails on both nodes. Mirrors Lotus `TestWindowPostV1P1NV20` and
//! `TestWindowPostDisputeFails`, with real 2 KiB proofs.

use super::api::TestMiner;
use super::devnet::{At, Devnet};
use crate::dev::subcommands::tests_cmd::helpers::{
    FOREST_TEST_PRELOADED_ADDRESS, block_on, poll, trial,
};
use crate::rpc::prelude::*;
use crate::rpc::types::MinerPartitions;
use crate::rpc::{LOOKBACK_NO_LIMIT, humanize_rpc_error};
use crate::shim::actors::miner::DeadlineInfo;
use crate::shim::address::Address;
use crate::shim::crypto::DomainSeparationTag;
use crate::shim::fvm_shared_latest::error::ExitCode;
use crate::shim::message::Message;
use crate::shim::sector::RegisteredPoStProof;
use anyhow::{Context as _, bail, ensure};
use cid::Cid;
use fil_actor_miner_state::v19::{DisputeWindowedPoStParams, Method, SubmitWindowedPoStParams};
use fil_actors_shared::fvm_ipld_bitfield::BitField;
use fvm_ipld_encoding::RawBytes;
use itertools::Itertools as _;
use libtest_mimic::Trial;
use std::str::FromStr as _;

/// The miner actor's error for a dispute of a valid proof
const VALID_POST_DISPUTED: &str = "failed to dispute valid post";

pub(super) fn tests() -> Vec<Trial> {
    vec![trial("wdpost::valid_post_keeps_power", || {
        block_on(valid_post_keeps_power())
    })]
}

async fn valid_post_keeps_power() -> anyhow::Result<()> {
    let net = Devnet::connect()?;
    let miner = TestMiner::genesis()?.actor;

    let at = net.synced().await?;
    let info = at.same(|tsk| StateMinerInfo::request((miner, tsk))).await?;
    let power = at
        .same(|tsk| StateMinerPower::request((miner, tsk)))
        .await?;
    let (deadline, partitions) = first_live_deadline(&at, miner).await?;

    // The PoSt lotus-miner submitted while the deadline was open.
    let at = net.wait_for_epoch(deadline.close).await?;
    let (post, params) = submitted_post(&at, miner, &deadline).await?;
    let exit = at
        .same(|tsk| StateSearchMsg::request((tsk, post, LOOKBACK_NO_LIMIT, true)))
        .await?
        .with_context(|| format!("no node finds the PoSt {post}"))?
        .receipt
        .exit_code();
    ensure!(
        exit.is_success(),
        "the PoSt {post} failed with exit code {exit}"
    );
    let proven = params.partitions.iter().map(|p| p.index).collect_vec();
    ensure!(
        proven == (0..u64::try_from(partitions.len())?).collect_vec(),
        "the PoSt {post} covers partitions {proven:?} of the {} in the deadline",
        partitions.len()
    );
    let proof_type = RegisteredPoStProof::from(info.window_post_proof_type);
    let proof_types = params.proofs.iter().map(|p| p.post_proof).collect_vec();
    ensure!(
        proof_types
            .iter()
            .all(|&t| RegisteredPoStProof::from(t) == proof_type),
        "the PoSt {post} uses {proof_types:?}, the miner's proof type is {proof_type:?}"
    );

    // What lotus-miner built the proof from, as both nodes answer it.
    let sectors = BitField::union(partitions.iter().map(|p| &p.live_sectors));
    at.same(|tsk| StateMinerSectors::request((miner, Some(sectors.clone()), tsk)))
        .await?;
    at.same(|tsk| StateNetworkVersion::request((tsk,))).await?;
    let entropy = fvm_ipld_encoding::to_vec(&miner)?;
    at.same(|tsk| {
        StateGetRandomnessFromBeacon::request((
            DomainSeparationTag::WindowedPoStChallengeSeed as i64,
            deadline.challenge,
            entropy.clone(),
            tsk,
        ))
    })
    .await?;
    let commit_rand = at
        .same(|tsk| {
            StateGetRandomnessFromTickets::request((
                DomainSeparationTag::PoStChainCommit as i64,
                params.chain_commit_epoch,
                vec![],
                tsk,
            ))
        })
        .await?;
    ensure!(
        commit_rand == params.chain_commit_rand.0,
        "the PoSt {post} committed to the ticket randomness {:?} of epoch {}, the nodes answer {commit_rand:?}",
        params.chain_commit_rand.0,
        params.chain_commit_epoch
    );

    // The cron at the deadline's last epoch stores the proof for disputes and faults the
    // partitions left unproven.
    let at = poll("the PoSt to be disputable", || async {
        let at = net.synced().await?;
        let deadlines = at
            .same(|tsk| StateMinerDeadlines::request((miner, tsk)))
            .await?;
        let stored = deadlines
            .get(usize::try_from(deadline.index)?)
            .is_some_and(|d| d.disputable_proof_count > 0);
        Ok(stored.then_some(at))
    })
    .await?;
    let faults = at
        .same(|tsk| StateMinerFaults::request((miner, tsk)))
        .await?;
    ensure!(
        faults.is_empty(),
        "sectors faulted after the PoSt: {faults:?}"
    );
    let after = at
        .same(|tsk| StateMinerPower::request((miner, tsk)))
        .await?;
    ensure!(
        after == power,
        "power changed over the PoSt: {power:?} before, {after:?} after"
    );

    // Disputing the valid proof re-verifies it in each node's VM and fails: simulated on both
    // nodes, then on chain through Forest, in a block Lotus validates.
    let sender = Address::from_str(&FOREST_TEST_PRELOADED_ADDRESS)
        .context("parsing FOREST_TEST_PRELOADED_ADDRESS")?;
    let dispute = Message::builder()
        .from(sender)
        .to(miner)
        .method_num(Method::DisputeWindowedPoSt as u64)
        .params(RawBytes::serialize(DisputeWindowedPoStParams {
            deadline: deadline.index,
            post_index: 0,
        })?)
        .build();
    let call = at
        .same(|tsk| StateCall::request((dispute.clone(), tsk)))
        .await?;
    let receipt = call
        .msg_rct
        .context("no receipt for the simulated dispute")?;
    ensure!(
        receipt.exit_code() == ExitCode::USR_ILLEGAL_ARGUMENT
            && call.error.contains(VALID_POST_DISPUTED),
        "the simulated dispute exits {} with `{}`, expected {} with `{VALID_POST_DISPUTED}`",
        receipt.exit_code(),
        call.error,
        ExitCode::USR_ILLEGAL_ARGUMENT
    );
    // Nothing estimates gas for a failing message; the margin covers the inclusion cost the
    // simulation leaves out.
    let gas_limit = receipt.gas_used() * 13 / 10;
    let pushed = net
        .forest
        .call(MpoolPushMessage::request((
            dispute.into_builder().gas_limit(gas_limit).build(),
            None,
        ))?)
        .await
        .context("forest MpoolPushMessage of the dispute")
        .map_err(humanize_rpc_error)?;
    let exit = net.executed(pushed.cid()).await?.receipt.exit_code();
    ensure!(
        exit == ExitCode::USR_ILLEGAL_ARGUMENT,
        "the dispute {} exits {exit} on chain, expected {}",
        pushed.cid(),
        ExitCode::USR_ILLEGAL_ARGUMENT
    );
    Ok(())
}

/// Returns the next instance of the first deadline with live sectors, and its partitions.
async fn first_live_deadline(
    at: &At<'_>,
    miner: Address,
) -> anyhow::Result<(DeadlineInfo, Vec<MinerPartitions>)> {
    let current = at
        .same(|tsk| StateMinerProvingDeadline::request((miner, tsk)))
        .await?
        .0;
    let count = at
        .same(|tsk| StateMinerDeadlines::request((miner, tsk)))
        .await?
        .len();
    for index in 0..u64::try_from(count)? {
        let partitions = at
            .same(|tsk| StateMinerPartitions::request((miner, index, tsk)))
            .await?;
        if partitions.iter().any(|p| !p.live_sectors.is_empty()) {
            return Ok((current.for_index(index).next_not_elapsed(), partitions));
        }
    }
    bail!("the genesis miner has no live sectors")
}

/// Returns the one `SubmitWindowedPoSt` for `deadline` that both nodes list since it opened.
async fn submitted_post(
    at: &At<'_>,
    miner: Address,
    deadline: &DeadlineInfo,
) -> anyhow::Result<(Cid, SubmitWindowedPoStParams)> {
    super::submit_windowed_posts(at, miner, deadline)
        .await?
        .into_iter()
        .exactly_one()
        .map_err(|posts| {
            anyhow::anyhow!(
                "expected one PoSt for deadline {} since epoch {}, found [{}]",
                deadline.index,
                deadline.open,
                posts.map(|(cid, _)| cid).join(", ")
            )
        })
}
