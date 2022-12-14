// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::{bail, Context};
use blake2b_simd::Params;
use byteorder::{BigEndian, WriteBytesExt};
use forest_beacon::{Beacon, BeaconEntry, BeaconSchedule, DrandBeacon};
use forest_blocks::{Tipset, TipsetKeys};
use forest_chain::ChainStore;
use forest_db::Store;
use forest_encoding::blake2b_256;
use forest_networks::ChainConfig;
use fvm::externs::Rand;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::clock::ChainEpoch;
use std::io::Write;
use std::sync::Arc;

/// Allows for deriving the randomness from a particular tipset.
pub struct ChainRand<DB> {
    chain_config: Arc<ChainConfig>,
    blks: TipsetKeys,
    cs: Arc<ChainStore<DB>>,
    beacon: Arc<BeaconSchedule<DrandBeacon>>,
    async_handle: tokio::runtime::Handle,
}

impl<DB> Clone for ChainRand<DB> {
    fn clone(&self) -> Self {
        ChainRand {
            chain_config: self.chain_config.clone(),
            blks: self.blks.clone(),
            cs: self.cs.clone(),
            beacon: self.beacon.clone(),
            async_handle: self.async_handle.clone(),
        }
    }
}

impl<DB> ChainRand<DB>
where
    DB: Blockstore + Store + Send + Sync,
{
    pub fn new(
        chain_config: Arc<ChainConfig>,
        blks: TipsetKeys,
        cs: Arc<ChainStore<DB>>,
        beacon: Arc<BeaconSchedule<DrandBeacon>>,
        async_handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            chain_config,
            blks,
            cs,
            beacon,
            async_handle,
        }
    }

    /// Gets 32 bytes of randomness for `ChainRand` parameterized by the `DomainSeparationTag`, `ChainEpoch`,
    /// Entropy from the ticket chain.
    pub async fn get_chain_randomness(
        &self,
        blocks: &TipsetKeys,
        pers: i64,
        round: ChainEpoch,
        entropy: &[u8],
        lookback: bool,
    ) -> anyhow::Result<[u8; 32]> {
        let ts = self.cs.tipset_from_keys(blocks).await?;

        if round > ts.epoch() {
            bail!("cannot draw randomness from the future");
        }

        let search_height = if round < 0 { 0 } else { round };

        let rand_ts = self
            .cs
            .tipset_by_height(search_height, ts, lookback)
            .await?;

        draw_randomness(
            rand_ts
                .min_ticket()
                .context("No ticket exists for block")?
                .vrfproof
                .as_bytes(),
            pers,
            round,
            entropy,
        )
    }

    /// network version 0-12
    pub async fn get_chain_randomness_v1(
        &self,
        blocks: &TipsetKeys,
        pers: i64,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> anyhow::Result<[u8; 32]> {
        self.get_chain_randomness(blocks, pers, round, entropy, true)
            .await
    }

    /// network version 13 onward
    pub async fn get_chain_randomness_v2(
        &self,
        blocks: &TipsetKeys,
        pers: i64,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> anyhow::Result<[u8; 32]> {
        self.get_chain_randomness(blocks, pers, round, entropy, false)
            .await
    }

    /// network version 0-12; with look-back
    pub async fn get_beacon_randomness_v1(
        &self,
        blocks: &TipsetKeys,
        pers: i64,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> anyhow::Result<[u8; 32]> {
        self.get_beacon_randomness(blocks, pers, round, entropy, true)
            .await
    }

    /// network version 13; without look-back
    pub async fn get_beacon_randomness_v2(
        &self,
        blocks: &TipsetKeys,
        pers: i64,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> anyhow::Result<[u8; 32]> {
        self.get_beacon_randomness(blocks, pers, round, entropy, false)
            .await
    }

    /// network version 14 onward
    pub async fn get_beacon_randomness_v3(
        &self,
        blocks: &TipsetKeys,
        pers: i64,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> anyhow::Result<[u8; 32]> {
        if round < 0 {
            return self
                .get_beacon_randomness_v2(blocks, pers, round, entropy)
                .await;
        }

        let beacon_entry = self.extract_beacon_entry_for_epoch(blocks, round).await?;
        draw_randomness(beacon_entry.data(), pers, round, entropy)
    }

    /// Gets 32 bytes of randomness for `ChainRand` parameterized by the `DomainSeparationTag`, `ChainEpoch`,
    /// Entropy from the latest beacon entry.
    pub async fn get_beacon_randomness(
        &self,
        blocks: &TipsetKeys,
        pers: i64,
        round: ChainEpoch,
        entropy: &[u8],
        lookback: bool,
    ) -> anyhow::Result<[u8; 32]> {
        let rand_ts: Arc<Tipset> = self
            .get_beacon_randomness_tipset(blocks, round, lookback)
            .await?;
        let be = self.cs.latest_beacon_entry(&rand_ts).await?;
        draw_randomness(be.data(), pers, round, entropy)
    }

    pub async fn extract_beacon_entry_for_epoch(
        &self,
        blocks: &TipsetKeys,
        epoch: ChainEpoch,
    ) -> anyhow::Result<BeaconEntry> {
        let mut rand_ts: Arc<Tipset> = self
            .get_beacon_randomness_tipset(blocks, epoch, false)
            .await?;
        let (_, beacon) = self.beacon.beacon_for_epoch(epoch)?;
        let round =
            beacon.max_beacon_round_for_epoch(self.chain_config.network_version(epoch), epoch);

        for _ in 0..20 {
            let cbe = rand_ts.blocks()[0].beacon_entries();
            for v in cbe {
                if v.round() == round {
                    return Ok(v.clone());
                }
            }

            rand_ts = self.cs.tipset_from_keys(rand_ts.parents()).await?;
        }

        bail!(
            "didn't find beacon for round {:?} (epoch {:?})",
            round,
            epoch
        )
    }

    pub async fn get_beacon_randomness_tipset(
        &self,
        blocks: &TipsetKeys,
        round: ChainEpoch,
        lookback: bool,
    ) -> anyhow::Result<Arc<Tipset>> {
        let ts = self.cs.tipset_from_keys(blocks).await?;

        if round > ts.epoch() {
            bail!("cannot draw randomness from the future");
        }

        let search_height = if round < 0 { 0 } else { round };

        self.cs
            .tipset_by_height(search_height, ts, lookback)
            .await
            .map_err(|e| e.into())
    }
}

impl<DB> Rand for ChainRand<DB>
where
    DB: Blockstore + Store + Send + Sync,
{
    fn get_chain_randomness(
        &self,
        pers: i64,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> anyhow::Result<[u8; 32]> {
        tokio::task::block_in_place(move || {
            self.async_handle
                .block_on(self.get_chain_randomness_v2(&self.blks, pers, round, entropy))
        })
    }

    fn get_beacon_randomness(
        &self,
        pers: i64,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> anyhow::Result<[u8; 32]> {
        tokio::task::block_in_place(move || {
            self.async_handle
                .block_on(self.get_beacon_randomness_v3(&self.blks, pers, round, entropy))
        })
    }
}

/// Computes a pseudo random 32 byte `Vec`.
pub fn draw_randomness(
    rbase: &[u8],
    pers: i64,
    round: ChainEpoch,
    entropy: &[u8],
) -> anyhow::Result<[u8; 32]> {
    let mut state = Params::new().hash_length(32).to_state();
    state.write_i64::<BigEndian>(pers)?;
    let vrf_digest = blake2b_256(rbase);
    state.write_all(&vrf_digest)?;
    state.write_i64::<BigEndian>(round)?;
    state.write_all(entropy)?;
    let mut ret = [0u8; 32];
    ret.clone_from_slice(state.finalize().as_bytes());
    Ok(ret)
}
