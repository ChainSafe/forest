// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Worker key rotation through Forest's RPC: the genesis miner's owner queues a new BLS worker,
//! the change takes effect, and lotus-miner keeps producing blocks Lotus accepts. Validators check
//! a block's ticket, election proof and signature against the worker in the state `chain_finality`
//! epochs back, so for that long after the change `MinerGetBaseInfo` must still answer the old
//! key. After Lotus `TestWorkerKeyChange`, with the two messages its `lotus-miner actor` commands
//! send pushed by the trial itself, as `TestSolsticeWorkerHandover` does.
//!
//! `window_start` stops a few epochs into that window, where a wrong worker key shows first;
//! `whole_window` (ignored, about 2 h) runs through it and across the switch to the new key.

use super::api::TestMiner;
use super::call;
use super::devnet::{At, Devnet};
use crate::dev::subcommands::tests_cmd::helpers::{FOREST_TEST_PRELOADED_ADDRESS, block_on, trial};
use crate::networks::ChainConfig;
use crate::rpc::prelude::*;
use crate::rpc::types::MessageLookup;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::crypto::SignatureType;
use crate::shim::econ::TokenAmount;
use crate::shim::message::Message;
use anyhow::{Context as _, ensure};
use fil_actor_miner_state::v19::{ChangeWorkerAddressParams, Method};
use fvm_ipld_encoding::RawBytes;
use libtest_mimic::Trial;
use std::str::FromStr as _;

/// Epochs between two checks of the window.
const CHECK_EVERY: ChainEpoch = 10;

pub(super) fn tests() -> Vec<Trial> {
    vec![
        trial("worker_key::window_start", || block_on(window_start())),
        trial("worker_key::whole_window", || block_on(whole_window())).with_ignored_flag(true),
    ]
}

async fn window_start() -> anyhow::Result<()> {
    let net = Devnet::connect()?;
    let r = rotate(&net).await?;
    old_key_signs(&net, &r, r.confirmed + 2 * CHECK_EVERY).await
}

async fn whole_window() -> anyhow::Result<()> {
    let net = Devnet::connect()?;
    let r = rotate(&net).await?;
    // Validators read a round's worker from the parent state of the first tipset at height
    // `round - 899` or above, so blocks switch to the new key at round `confirmed + 899`, one
    // round earlier per null round right below the change or per epoch the deadline cron
    // applied it before the confirm. The trial does not predict the round: the old key is
    // asserted only up to a margin before it, and across it both nodes only have to agree at
    // every head.
    let switch = r.confirmed + ChainConfig::devnet().policy.chain_finality - 1;
    old_key_signs(&net, &r, switch - CHECK_EVERY).await?;
    new_key_signs(&net, &r, switch - 2 * CHECK_EVERY, switch + CHECK_EVERY).await
}

/// The miner, its worker keys, and the epoch the confirm executed at: the change took effect
/// then, or a few epochs earlier when the deadline cron applied it first.
struct Rotation {
    miner: Address,
    old_key: Address,
    new_key: Address,
    confirmed: ChainEpoch,
}

/// Queues a new BLS worker from the owner and confirms it once effective.
async fn rotate(net: &Devnet) -> anyhow::Result<Rotation> {
    let miner = TestMiner::genesis()?.actor;
    let at = net.synced().await?;
    let info = at.same(|tsk| StateMinerInfo::request((miner, tsk))).await?;
    let old_key = at
        .same(|tsk| StateAccountKey::request((info.worker, tsk)))
        .await?;

    // The actor requires a BLS account that exists on chain; Forest's wallet signs with it later.
    let new_key = call(
        &net.forest,
        "forest",
        WalletNew::request((SignatureType::Bls,))?,
    )
    .await?;
    let sender = Address::from_str(&FOREST_TEST_PRELOADED_ADDRESS)
        .context("parsing FOREST_TEST_PRELOADED_ADDRESS")?;
    let fund = Message::transfer(sender, new_key, TokenAmount::from_whole(1));
    push(net, fund).await.context("funding the new worker")?;
    let new_id = net
        .synced()
        .await?
        .same(|tsk| StateLookupID::request((new_key, tsk)))
        .await?;

    // Only the owner may queue the change, which takes effect `worker_key_change_delay` epochs
    // later. The actor replaces the control addresses with the list sent, so the current ones go
    // along.
    let params = ChangeWorkerAddressParams {
        new_worker: (&new_key).into(),
        new_control_addresses: info.control_addresses.iter().map(Into::into).collect(),
    };
    let change = Message::builder()
        .from(info.owner)
        .to(miner)
        .method_num(Method::ChangeWorkerAddress as u64)
        .params(RawBytes::serialize(params)?)
        .build();
    push(net, change)
        .await
        .context("queuing the worker change")?;
    let pending = net
        .synced()
        .await?
        .same(|tsk| StateMinerInfo::request((miner, tsk)))
        .await?;
    ensure!(
        pending.new_worker == Some(new_id) && pending.worker == info.worker,
        "miner {miner}: pending worker {:?} and worker {}, expected {new_id} and {} unchanged",
        pending.new_worker,
        pending.worker,
        info.worker
    );

    // The confirmation applies the pending key once effective, unless the deadline cron already
    // did.
    net.wait_for_epoch(pending.worker_change_epoch).await?;
    let confirm = Message::builder()
        .from(info.owner)
        .to(miner)
        .method_num(Method::ConfirmChangeWorkerAddress as u64)
        .build();
    let confirmed = push(net, confirm)
        .await
        .context("confirming the worker change")?
        .height;
    let info = net
        .synced()
        .await?
        .same(|tsk| StateMinerInfo::request((miner, tsk)))
        .await?;
    ensure!(
        info.worker == new_id && info.new_worker.is_none(),
        "miner {miner}: worker {} with pending {:?} after the change, expected {new_id}",
        info.worker,
        info.new_worker
    );
    eprintln!("worker change confirmed at epoch {confirmed}: {old_key} -> {new_key}");
    Ok(Rotation {
        miner,
        old_key,
        new_key,
        confirmed,
    })
}

/// Fails unless, every `CHECK_EVERY` epochs from the confirm while below `until`, Lotus holds
/// Forest's head and both nodes answer the old key for the next round, the one validators check
/// it against.
async fn old_key_signs(net: &Devnet, r: &Rotation, until: ChainEpoch) -> anyhow::Result<()> {
    let mut epoch = r.confirmed;
    while epoch < until {
        let at = net.wait_for_epoch(epoch).await?;
        let key = worker_key(&at, r.miner).await?;
        ensure!(
            key == r.old_key,
            "round {} signs with {key}, expected {}",
            at.epoch() + 1,
            r.old_key
        );
        epoch += CHECK_EVERY;
    }
    Ok(())
}

/// Fails unless both nodes give the same `MinerGetBaseInfo` at every head from `from` to `until`
/// and answer the new key at the end. The switch from the old key lies in between, so a node
/// that switches a round early or late fails at that head instead of losing one block.
async fn new_key_signs(
    net: &Devnet,
    r: &Rotation,
    from: ChainEpoch,
    until: ChainEpoch,
) -> anyhow::Result<()> {
    let mut at = net.wait_for_epoch(from).await?;
    let mut key = worker_key(&at, r.miner).await?;
    while at.epoch() < until {
        at = net.wait_for_epoch(at.epoch() + 1).await?;
        key = worker_key(&at, r.miner).await?;
    }
    ensure!(
        key == r.new_key,
        "round {} signs with {key}, expected {}",
        at.epoch() + 1,
        r.new_key
    );
    Ok(())
}

/// Returns the worker both nodes answer for the round after `at`; fails if they differ or neither
/// has base info for the miner.
async fn worker_key(at: &At<'_>, miner: Address) -> anyhow::Result<Address> {
    let round = at.epoch() + 1;
    let info = at
        .same(|tsk| MinerGetBaseInfo::request((miner, round, tsk)))
        .await?
        .with_context(|| format!("no mining base info for {miner} at round {round}"))?;
    Ok(info.worker_key)
}

/// Pushes `message` through Forest and returns where it executed; fails unless both nodes find it
/// executed with exit code 0.
async fn push(net: &Devnet, message: Message) -> anyhow::Result<MessageLookup> {
    let pushed = call(
        &net.forest,
        "forest",
        MpoolPushMessage::request((message, None))?,
    )
    .await?;
    let cid = pushed.cid();
    let lookup = net
        .executed(cid)
        .await
        .with_context(|| format!("message {cid}"))?;
    let exit = lookup.receipt.exit_code();
    ensure!(exit.is_success(), "message {cid} exits {exit}");
    Ok(lookup)
}
