// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Worker key rotation through Forest's RPC: the genesis miner's owner queues a new BLS worker,
//! the change takes effect, and lotus-miner keeps producing blocks Lotus accepts. Validators check
//! a block's ticket, election proof and signature against the worker in the state `chain_finality`
//! epochs back, so for that long after the change `MinerGetBaseInfo` must still answer the old
//! key. Mirrors Lotus `TestWorkerKeyChange` and `TestSolsticeWorkerHandover`.
//!
//! `window_start` stops a few epochs into that window, where the chain stalls without the fix;
//! `whole_window` (ignored, about 2 h) runs through it to the switch to the new key.

use super::api::TestMiner;
use super::call;
use super::devnet::{At, Devnet};
use crate::dev::subcommands::tests_cmd::helpers::{FOREST_TEST_PRELOADED_ADDRESS, block_on, trial};
use crate::networks::ChainConfig;
use crate::rpc::prelude::*;
use crate::rpc::types::{ApiTipsetKey, MessageLookup};
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
    let rotation = rotate(&net).await?;
    old_key_signs(&net, &rotation, 3 * CHECK_EVERY).await
}

async fn whole_window() -> anyhow::Result<()> {
    let net = Devnet::connect()?;
    let rotation = rotate(&net).await?;
    let lookback = ChainConfig::devnet().policy.chain_finality;
    old_key_signs(&net, &rotation, lookback).await?;
    new_key_signs(&net, &rotation, lookback).await
}

/// The miner, its worker keys, and the first epoch whose state has the new one.
struct Rotation {
    miner: Address,
    old_key: Address,
    new_key: Address,
    effective: ChainEpoch,
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
    push(net, "funding the new worker", fund).await?;
    let new_id = net
        .synced()
        .await?
        .same(|tsk| StateLookupID::request((new_key, tsk)))
        .await?;

    // Only the owner may queue the change, which takes effect `worker_key_change_delay` epochs later.
    let params = ChangeWorkerAddressParams {
        new_worker: (&new_key).into(),
        new_control_addresses: vec![],
    };
    let change = Message::builder()
        .from(info.owner)
        .to(miner)
        .method_num(Method::ChangeWorkerAddress as u64)
        .params(RawBytes::serialize(params)?)
        .build();
    push(net, "queuing the worker change", change).await?;
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

    // The confirmation applies the pending key once effective, unless the deadline cron already did.
    net.wait_for_epoch(pending.worker_change_epoch).await?;
    let confirm = Message::builder()
        .from(info.owner)
        .to(miner)
        .method_num(Method::ConfirmChangeWorkerAddress as u64)
        .build();
    let mut effective = push(net, "confirming the worker change", confirm)
        .await?
        .height;
    while worker_at(net, miner, effective - 1).await? == new_id {
        effective -= 1;
    }
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
    eprintln!("worker changed at epoch {effective}: {old_key} -> {new_key}");
    Ok(Rotation {
        miner,
        old_key,
        new_key,
        effective,
    })
}

/// Every `CHECK_EVERY` epochs over the first `window` epochs after the change, stopping
/// `CHECK_EVERY` short of it: Lotus accepts Forest's head, and both nodes still answer the old
/// key, the one validators check the next block against. The gap covers the head overshooting a
/// checkpoint by an epoch or two, and the null rounds right before the change, each of which
/// brings the switch to the new key an epoch closer.
async fn old_key_signs(net: &Devnet, r: &Rotation, window: ChainEpoch) -> anyhow::Result<()> {
    let mut epoch = r.effective;
    while epoch < r.effective + window - CHECK_EVERY {
        let at = net.wait_for_epoch(epoch).await?;
        ensure_worker_key(&at, r.miner, r.old_key).await?;
        epoch += CHECK_EVERY;
    }
    Ok(())
}

/// Ten epochs past the window, safely behind the switch, both nodes answer the new key; twenty
/// epochs later blocks are still accepted, so the new key signs them.
async fn new_key_signs(net: &Devnet, r: &Rotation, lookback: ChainEpoch) -> anyhow::Result<()> {
    let at = net.wait_for_epoch(r.effective + lookback + 10).await?;
    ensure_worker_key(&at, r.miner, r.new_key).await?;
    net.wait_for_epoch(r.effective + lookback + 30).await?;
    Ok(())
}

/// Both nodes answer `key` as the worker signing the round after `at`.
async fn ensure_worker_key(at: &At<'_>, miner: Address, key: Address) -> anyhow::Result<()> {
    let round = at.epoch() + 1;
    let info = at
        .same(|tsk| MinerGetBaseInfo::request((miner, round, tsk)))
        .await?
        .with_context(|| format!("no mining base info for {miner} at round {round}"))?;
    ensure!(
        info.worker_key == key,
        "round {round} signs with {}, expected {key}",
        info.worker_key
    );
    Ok(())
}

/// Pushes `message`, described by `label`, through Forest and returns where it executed, which
/// must be with exit code 0.
async fn push(net: &Devnet, label: &str, message: Message) -> anyhow::Result<MessageLookup> {
    let pushed = call(
        &net.forest,
        "forest",
        MpoolPushMessage::request((message, None))?,
    )
    .await
    .with_context(|| label.to_owned())?;
    let cid = pushed.cid();
    let lookup = net
        .executed(cid)
        .await
        .with_context(|| format!("{label}: {cid}"))?;
    let exit = lookup.receipt.exit_code();
    ensure!(exit.is_success(), "{label}: {cid} exits {exit}");
    Ok(lookup)
}

/// The miner's worker in the state at `epoch`, or at the nearest older tipset when `epoch` is
/// null, as Forest has it.
async fn worker_at(net: &Devnet, miner: Address, epoch: ChainEpoch) -> anyhow::Result<Address> {
    let tipset = call(
        &net.forest,
        "forest",
        ChainGetTipSetByHeight::request((epoch, ApiTipsetKey(None)))?,
    )
    .await?;
    let key = ApiTipsetKey(Some(tipset.key().clone()));
    let info = call(
        &net.forest,
        "forest",
        StateMinerInfo::request((miner, key))?,
    )
    .await?;
    Ok(info.worker)
}
