// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Window PoSt through Forest's RPC: lotus-miner proves the genesis miner's deadline from what
//! Forest answers, and a dispute, the only check that verifies the proof, fails on both nodes.
//! Mirrors Lotus `TestWindowPostV1P1NV20` and `TestWindowPostDisputeFails`, with real 2 KiB proofs.

use super::api::TestMiner;
use super::devnet::{At, Devnet};
use crate::dev::subcommands::tests_cmd::helpers::{FOREST_TEST_PRELOADED_ADDRESS, block_on, trial};
use crate::rpc::prelude::*;
use crate::rpc::types::MessageFilter;
use crate::rpc::{LOOKBACK_NO_LIMIT, humanize_rpc_error};
use crate::shim::actors::miner::DeadlineInfo;
use crate::shim::address::Address;
use crate::shim::crypto::DomainSeparationTag;
use crate::shim::fvm_shared_latest::error::ExitCode;
use crate::shim::message::Message;
use anyhow::{Context as _, ensure};
use cid::Cid;
use fil_actor_miner_state::v19::{DisputeWindowedPoStParams, Method, SubmitWindowedPoStParams};
use fil_actors_shared::fvm_ipld_bitfield::BitField;
use fvm_ipld_encoding::RawBytes;
use itertools::Itertools as _;
use libtest_mimic::Trial;
use std::str::FromStr as _;

/// The miner actor's error for a dispute of a valid proof
/// (<https://github.com/filecoin-project/builtin-actors/blob/v19.0.1/actors/miner/src/lib.rs#L1081>).
const VALID_POST_DISPUTED: &str = "failed to dispute valid post";

pub(super) fn tests() -> Vec<Trial> {
    vec![trial("wdpost::valid_post_keeps_power", || {
        block_on(valid_post_keeps_power())
    })]
}

/// lotus-miner proves the first deadline with live sectors through Forest: the PoSt lands, the
/// miner keeps its power, and a dispute, which re-verifies the proof in Forest's VM, fails.
async fn valid_post_keeps_power() -> anyhow::Result<()> {
    let net = Devnet::connect()?;
    let miner = TestMiner::genesis()?.actor;

    // Before the deadline: what lotus-miner reads to schedule the proof and to pick its partitions.
    let at = net.synced().await?;
    at.same(|tsk| StateMinerInfo::request((miner, tsk))).await?;
    let power = at
        .same(|tsk| StateMinerPower::request((miner, tsk)))
        .await?;
    let current = at
        .same(|tsk| StateMinerProvingDeadline::request((miner, tsk)))
        .await?
        .0;
    let deadline_count = at
        .same(|tsk| StateMinerDeadlines::request((miner, tsk)))
        .await?
        .len();
    let mut first_live = None;
    for index in 0..u64::try_from(deadline_count)? {
        let partitions = at
            .same(|tsk| StateMinerPartitions::request((miner, index, tsk)))
            .await?;
        if partitions.iter().any(|p| !p.live_sectors.is_empty()) {
            first_live = Some((index, partitions));
            break;
        }
    }
    let (index, partitions) = first_live.context("the genesis miner has no live sectors")?;
    let deadline = current.for_index(index).next_not_elapsed();

    // lotus-miner submits at open + 4. The cron at the deadline's last epoch, `close - 1`, stores
    // the proof for disputes and faults any partition left unproven; queries read a tipset's
    // parent state, so the cron shows only in the tipsets after `close`.
    let at = net.wait_for_epoch(deadline.close + 1).await?;
    let post = submitted_post(&at, miner, &deadline).await?;
    let lookup = at
        .same(|tsk| StateSearchMsg::request((tsk, post, LOOKBACK_NO_LIMIT, true)))
        .await?
        .with_context(|| format!("no node finds the PoSt {post}"))?;
    // The actor accepts only one proof of the miner's proof type, committed to the chain's ticket
    // randomness (builtin-actors v19.0.1 `actors/miner/src/lib.rs` L531, L578, L661), so exit 0
    // also checks those.
    let exit = lookup.receipt.exit_code();
    ensure!(
        exit.is_success(),
        "the PoSt {post} failed with exit code {exit}"
    );

    // The rest of what lotus-miner built the proof from: the sectors it proves, the network
    // version that picks the proof type, the challenge seed and the ticket the proof commits to.
    // Gas estimates, the worker's wallet and the push are covered by the PoSt landing.
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
    at.same(|tsk| {
        StateGetRandomnessFromTickets::request((
            DomainSeparationTag::PoStChainCommit as i64,
            deadline.challenge,
            vec![],
            tsk,
        ))
    })
    .await?;

    // After the deadline: no sector faulted, so the PoSt also covered every partition, and the
    // power held. An onboarding run on the same devnet may add power, so only a drop fails.
    at.same(|tsk| StateMinerDeadlines::request((miner, tsk)))
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
        after.miner_power.quality_adj_power >= power.miner_power.quality_adj_power,
        "power dropped over the PoSt: {power:?} before, {after:?} after"
    );

    // A dispute of the valid proof re-verifies it and fails: simulated on both nodes, then on
    // chain. Its text also shows that the deadline stored the proof: without one the actor
    // answers "proof 0 not found" (`actors/miner/src/deadline_state.rs` L1369).
    let sender = Address::from_str(&FOREST_TEST_PRELOADED_ADDRESS)
        .context("parsing FOREST_TEST_PRELOADED_ADDRESS")?;
    let dispute = Message::builder()
        .from(sender)
        .to(miner)
        .method_num(Method::DisputeWindowedPoSt as u64)
        .params(RawBytes::serialize(DisputeWindowedPoStParams {
            deadline: index,
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
    // StateCall leaves out the inclusion cost, hence the margin. An explicit limit also skips the
    // gas estimate, which refuses a message that fails.
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
    // `landed` also pins a tipset above the dispute that Lotus accepted from Forest.
    let (_, lookup) = net.landed(pushed.cid()).await?;
    let exit = lookup.receipt.exit_code();
    ensure!(
        exit == ExitCode::USR_ILLEGAL_ARGUMENT,
        "the dispute {} exits {exit} on chain, expected {}",
        pushed.cid(),
        ExitCode::USR_ILLEGAL_ARGUMENT
    );
    Ok(())
}

/// Returns the one `SubmitWindowedPoSt` for `deadline` that both nodes list since it opened.
async fn submitted_post(
    at: &At<'_>,
    miner: Address,
    deadline: &DeadlineInfo,
) -> anyhow::Result<Cid> {
    let to_miner = MessageFilter {
        from: None,
        to: Some(miner),
    };
    let mut posts = Vec::new();
    for cid in at
        .same(|tsk| StateListMessages::request((to_miner.clone(), tsk, deadline.open)))
        .await?
    {
        let message = at.same(|_| ChainGetMessage::request((cid,))).await?;
        if message.method_num() != Method::SubmitWindowedPoSt as u64 {
            continue;
        }
        let params: SubmitWindowedPoStParams = message
            .params()
            .deserialize()
            .with_context(|| format!("decoding the parameters of {cid}"))?;
        if params.deadline == deadline.index {
            posts.push(cid);
        }
    }
    posts.into_iter().exactly_one().map_err(|mut posts| {
        anyhow::anyhow!(
            "expected one PoSt for deadline {} since epoch {}, found [{}]",
            deadline.index,
            deadline.open,
            posts.join(", ")
        )
    })
}
