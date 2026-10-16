// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! `eth_getTransactionByHash` pending transaction tests on the docker devnet.

use crate::dev::subcommands::tests_cmd::helpers::*;
use crate::rpc::eth::types::EthHash;
use crate::rpc::prelude::*;
use anyhow::{Context as _, ensure};
use cid::Cid;
use libtest_mimic::{Arguments, Trial};
use serde_json::{Value, json};
use std::str::FromStr as _;

/// `eth_getTransactionByHash` pending transaction tests
#[derive(Debug, clap::Args)]
pub struct EthPendingTxTestCommand {}

impl EthPendingTxTestCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let args = Arguments {
            test_threads: Some(1),
            ..Default::default()
        };
        libtest_mimic::run(&args, tests()).exit();
    }
}

fn tests() -> Vec<Trial> {
    vec![trial("pending_tx_from_lotus_is_found_by_its_hash", || {
        block_on(pending_tx_from_lotus_is_found_by_its_hash())
    })]
}

/// A transaction submitted through Lotus, so without a hash mapping on Forest, is found by its
/// Ethereum hash while it is still pending.
async fn pending_tx_from_lotus_is_found_by_its_hash() -> anyhow::Result<()> {
    let (forest, lotus) = (forest_client()?, lotus_client()?);
    let sender = sender().await?;

    // A nonce gap keeps the transaction pending until the gap is filled, so the lookup cannot
    // race the miner.
    let gap_nonce = mpool_nonce(&sender.cli)? + 1;
    // Lotus's mpool reads state at its own head, which trails the chain head the sender was
    // funded on.
    let gapped = poll("lotus to accept the gapped send", || async {
        match lotus_send_to_self(&sender.cli, Some(gap_nonce)) {
            Ok(cid) => Ok(Some(cid)),
            Err(e) if is_unknown_actor_error(&e) => Ok(None),
            Err(e) => Err(e),
        }
    })
    .await?;
    poll_until_pending_nonce(&sender.cli, gap_nonce).await?;
    let hash = lotus
        .call(EthGetTransactionHashByCid::request((gapped,))?)
        .await?
        .context("lotus does not know the transaction's hash")?;

    let tx = forest
        .call(EthGetTransactionByHash::request((hash,))?)
        .await?
        .context("forest does not return the pending transaction by its hash")?;
    // Lotus's answer for the same transaction; its block fields are `null` while pending, so only
    // the fields that identify the transaction are compared.
    let lotus_tx = lotus
        .call(EthGetTransactionByHash::request((hash,))?.map_ty::<Value>())
        .await?;
    for (field, forest_value) in [
        ("hash", json!(tx.hash)),
        ("from", json!(tx.from)),
        ("nonce", json!(tx.nonce)),
    ] {
        ensure!(
            lotus_tx.get(field) == Some(&forest_value),
            "{field}: forest {forest_value}, lotus {lotus_tx}"
        );
    }
    ensure!(
        tx.block_hash == EthHash::default(),
        "reported as mined while pending: {tx:?}"
    );

    // Filling the gap gets both mined, so nothing stays pending after the trial.
    lotus_send_to_self(&sender.cli, None)?;
    poll_until_message_executed(&forest, gapped).await?;
    Ok(())
}

/// Sends 0 FIL from `from` to itself through Lotus at `nonce`, or at the next nonce, and returns
/// the message CID.
fn lotus_send_to_self(from: &str, nonce: Option<u64>) -> anyhow::Result<Cid> {
    let nonce = nonce.map(|n| n.to_string());
    // `--force-send` skips the check that the nonce is the next one.
    let mut args = vec!["--force-send", "send", "--from", from];
    if let Some(nonce) = &nonce {
        args.extend(["--nonce", nonce]);
    }
    args.extend([from, "0"]);
    let out = lotus_exec(&args)?;
    // The CID follows the `Sending message ...` lines.
    let cid = out.lines().last().context("empty `lotus send` output")?;
    Cid::from_str(cid.trim())
        .with_context(|| format!("parsing the cid in `lotus send` output: {out}"))
}
