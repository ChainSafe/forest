// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use anyhow::Context as _;
use clap::Args;
use num::Zero as _;
use once_cell::sync::Lazy;

use crate::{
    blocks::TipsetKey,
    db::rpc_db::RpcDb,
    networks::ChainConfig,
    rpc::{
        prelude::{ChainGetTipSetByHeight, ChainHead, StateGetActor, StateMinerProvingDeadline},
        RpcMethodExt as _,
    },
    shim::{
        actors::{
            miner::{
                self,
                ext::{DeadlineExt, MinerStateExt},
            },
            LoadActorStateFromBlockstore,
        },
        address::Address,
        econ::TokenAmount,
    },
};

/// Inspect FIP-0100 fees for a miner
#[derive(Debug, Args)]
pub struct FeesCommand {
    /// Miner address
    pub miner_address: Address,
    /// Number of proving periods to check
    #[arg(long, default_value_t = 1)]
    pub proving_periods: usize,
    /// Tipset epoch
    #[arg(long)]
    pub epoch: Option<i64>,
    /// Tipset key, e.g. `bafy2bzacear67vciqyqjyzn77pb75rvtvagsmydfhgupbija2kpdtewkvq2gy,bafy2bzacecoyawspvaoevxx6le5ud65euuplg4pq6au7bmv4dd6dhwiiwlzuq`
    #[arg(long)]
    pub tsk: Option<TipsetKey>,
}

impl FeesCommand {
    pub async fn run(self, client: crate::rpc::Client) -> anyhow::Result<()> {
        let Self {
            miner_address,
            proving_periods,
            epoch,
            tsk,
        } = self;
        static BURN_ADDRESS: Lazy<anyhow::Result<Address>> = Lazy::new(|| Ok("f099".parse()?));

        let client = Arc::new(client);
        let policy = ChainConfig::calibnet().policy;

        let tipset_key = match (epoch, tsk) {
            (Some(_), Some(_)) => {
                anyhow::bail!("specify either `--tsk` or `--epoch` but not both")
            }
            (Some(epoch), None) => ChainGetTipSetByHeight::call(&client, (epoch, None.into()))
                .await?
                .key()
                .clone(),
            (None, Some(tsk)) => tsk,
            (None, None) => ChainHead::call(&client, ()).await?.key().clone(),
        };

        let db = RpcDb::new(client.clone());

        let deadline_info =
            StateMinerProvingDeadline::call(&client, (miner_address, tipset_key.clone().into()))
                .await?;
        let miner_actor = StateGetActor::call(&client, (miner_address, tipset_key.clone().into()))
            .await?
            .context("miner actor not found")?;
        let miner_state = miner::State::load_from_blockstore(&db, &miner_actor)?;
        // let sectors = miner_state.load_sectors_ext(&db, None)?;
        let mut discrepancies = false;
        let mut total_miner_fee: TokenAmount = TokenAmount::zero();
        miner_state.for_each_deadline(&policy, &db, |deadline_i, deadline| {
            println!("Deadline {deadline_i}:");
            let mut total_sectors_fee = TokenAmount::zero();
            let mut total_fee_deduction = TokenAmount::zero();

            deadline.for_each(&db, |_, partition| {
                let sectors = miner_state.load_sectors_ext(&db, Some(&partition.live_sectors()))?;
                for (sector_i, sector) in sectors {
                    print!("\tSector {sector_i} daily fee:");
                    match &miner_state {
                        miner::State::V16(_) => {
                            if sector.daily_fee.is_zero() {
                                let sectors_amt: fil_actors_shared::v15::Array<
                                    fil_actor_miner_state::v15::SectorOnChainInfo,
                                    _,
                                > = fil_actors_shared::v15::Array::load(
                                    miner_state.sectors(),
                                    &db,
                                )?;
                                match sectors_amt.get(sector_i) {
                                    Ok(_) => {
                                        // if we can load it as v15, it's not migrated and has no fee
                                        println!("<legacy, not migrated>")
                                    }
                                    _ => {
                                        println!("<legacy>")
                                    }
                                }
                            } else {
                                println!("{}", sector.daily_fee)
                            }
                        }
                        _ => {
                            println!("<legacy, not migrated>")
                        }
                    };
                }

                // Iterate over expiration queue within each partition

                Ok(())
            })?;

            let correct = if deadline.daily_fee() != total_sectors_fee
                || deadline.daily_fee() != total_fee_deduction
            {
                discrepancies = true;
                "✗"
            } else {
                "✓"
            };
            println!("\t{correct} Deadline daily fee: {daily_fee}, power: {power} (sector fee sum: {total_sectors_fee}, expiration fee deduction sum:: {total_fee_deduction})", 
                daily_fee = deadline.daily_fee(),
                power = deadline.live_power_qa());

            Ok(())
        })?;
        Ok(())
    }
}
