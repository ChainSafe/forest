// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::num::NonZeroUsize;

use anyhow::Context as _;
use itertools::Itertools;
use num::{BigInt, Zero as _};

use crate::{
    rpc::{
        self, RpcMethodExt as _,
        chain::{
            ChainGetBlockMessages, ChainGetParentMessages, ChainGetParentReceipts, ChainGetTipSet,
            ChainGetTipSetByHeight, ChainHead,
        },
    },
    shim::econ::{BLOCK_GAS_LIMIT, TokenAmount},
};

/// View a segment of the chain
#[derive(Debug, clap::Args)]
pub struct ChainListCommand {
    /// Start epoch (default: current head)
    #[arg(long)]
    epoch: Option<u64>,
    /// Number of tipsets
    #[arg(long, default_value_t = NonZeroUsize::new(30).unwrap())]
    count: NonZeroUsize,
    #[arg(long)]
    /// View gas statistics for the chain
    gas_stats: bool,
}

impl ChainListCommand {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        let count = self.count.into();
        let mut ts = if let Some(epoch) = self.epoch {
            ChainGetTipSetByHeight::call(&client, (epoch as _, None.into())).await?
        } else {
            ChainHead::call(&client, ()).await?
        };
        let mut tipsets = Vec::with_capacity(count);
        loop {
            tipsets.push(ts.clone());
            if ts.epoch() == 0 || tipsets.len() >= count {
                break;
            }
            ts = ChainGetTipSet::call(&client, (ts.parents().into(),)).await?;
        }
        tipsets.reverse();

        for (i, ts) in tipsets.iter().enumerate() {
            if self.gas_stats {
                let base_fee = &ts.block_headers().first().parent_base_fee;
                let max_fee = base_fee * BLOCK_GAS_LIMIT;
                println!(
                    "{height}: {n} blocks (baseFee: {base_fee} -> maxFee: {max_fee} FIL)",
                    height = ts.epoch(),
                    n = ts.len()
                );
                for b in ts.block_headers() {
                    let msgs = ChainGetBlockMessages::call(&client, (*b.cid(),)).await?;
                    let len = msgs.bls_msg.len() + msgs.secp_msg.len();
                    let mut limit_sum = 0;
                    let mut premium_sum = TokenAmount::zero();
                    let mut premium_avg = BigInt::zero();
                    for m in &msgs.bls_msg {
                        limit_sum += m.gas_limit;
                        premium_sum += m.gas_premium.clone();
                    }
                    for m in &msgs.secp_msg {
                        limit_sum += m.message().gas_limit;
                        premium_sum += m.message().gas_premium.clone();
                    }

                    if len > 0 {
                        premium_avg = premium_sum.atto() / BigInt::from(len);
                    }

                    println!(
                        "\t{miner}: \t{len} msgs, gasLimit: {limit_sum} / {BLOCK_GAS_LIMIT} ({ratio:.2}), avgPremium: {premium_avg}",
                        miner = b.miner_address,
                        ratio = (limit_sum as f64) / (BLOCK_GAS_LIMIT as f64) * 100.0
                    );
                }
                if let Some(child_ts) = tipsets.get(i + 1) {
                    let msgs = ChainGetParentMessages::call(
                        &client,
                        (*child_ts.block_headers().first().cid(),),
                    )
                    .await?;
                    let limit_sum: u64 = msgs.iter().map(|m| m.message.gas_limit).sum();
                    let gas_used: u64 = {
                        let receipts = ChainGetParentReceipts::call(
                            &client,
                            (*child_ts.block_headers().first().cid(),),
                        )
                        .await?;
                        receipts.iter().map(|r| r.gas_used).sum()
                    };
                    let gas_efficiency = 100. * (gas_used as f64) / (limit_sum as f64);
                    let gas_capacity = 100. * (limit_sum as f64) / (BLOCK_GAS_LIMIT as f64);

                    println!(
                        "\ttipset: \t{n} msgs, {gas_used} ({gas_efficiency:.2}%) / {limit_sum} ({gas_capacity:.2}%)",
                        n = msgs.len()
                    );
                }
            } else {
                let epoch = ts.epoch();
                let time = chrono::DateTime::from_timestamp(ts.min_timestamp() as _, 0)
                    .context("invalid timestamp")?
                    .format("%b %e %X");
                let tsk = ts
                    .block_headers()
                    .iter()
                    .map(|h| format!("{}: {},", h.cid(), h.miner_address))
                    .join("");
                println!("{epoch}: ({time}) [ {tsk} ]");
            }
        }

        Ok(())
    }
}
