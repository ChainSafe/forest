// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::io::Write;

use crate::beacon::{Beacon as _, BeaconEntry, BeaconSchedule};
use crate::blocks::Tipset;
use crate::chain::index::{ChainIndex, ResolveNullTipset};
use crate::networks::ChainConfig;
use crate::prelude::*;
use crate::shim::clock::ChainEpoch;
use crate::shim::externs::Rand;
use crate::shim::version::NetworkVersion;
use crate::utils::encoding::blake2b_256;
use anyhow::bail;
use blake2b_simd::Params;
use byteorder::{BigEndian, WriteBytesExt};

/// Allows for deriving the randomness from a particular tipset.
#[derive(derive_more::Constructor)]
pub struct ChainRand {
    chain_config: Arc<ChainConfig>,
    tipset: Tipset,
    chain_index: ChainIndex,
    beacon: Arc<BeaconSchedule>,
}

impl ShallowClone for ChainRand {
    fn shallow_clone(&self) -> Self {
        ChainRand {
            chain_config: self.chain_config.shallow_clone(),
            tipset: self.tipset.shallow_clone(),
            chain_index: self.chain_index.shallow_clone(),
            beacon: self.beacon.shallow_clone(),
        }
    }
}

impl ChainRand {
    /// Gets 32 bytes of randomness for `ChainRand` parameterized by the
    /// `DomainSeparationTag`, `ChainEpoch`, Entropy from the ticket chain.
    pub async fn get_chain_randomness(
        &self,
        round: ChainEpoch,
        lookback: bool,
    ) -> anyhow::Result<[u8; 32]> {
        let this = self.shallow_clone();
        tokio::task::spawn_blocking(move || this.get_chain_randomness_blocking(round, lookback))
            .await?
    }

    /// Gets 32 bytes of randomness for `ChainRand` parameterized by the
    /// `DomainSeparationTag`, `ChainEpoch`, Entropy from the ticket chain.
    /// This call can be expensive and blocking, use [`Self::get_chain_randomness`]
    /// in async contexts to avoid exhausting Tokio worker threads.
    pub fn get_chain_randomness_blocking(
        &self,
        round: ChainEpoch,
        lookback: bool,
    ) -> anyhow::Result<[u8; 32]> {
        let ts = self.tipset.clone();

        if round > ts.epoch() {
            bail!("cannot draw randomness from the future");
        }

        let search_height = if round < 0 { 0 } else { round };

        let resolve = if lookback {
            ResolveNullTipset::TakeOlder
        } else {
            ResolveNullTipset::TakeNewer
        };
        let rand_ts =
            self.chain_index
                .load_required_tipset_by_height_blocking(search_height, ts, resolve)?;

        Ok(digest(
            rand_ts
                .min_ticket()
                .context("No ticket exists for block")?
                .vrfproof
                .as_bytes(),
        ))
    }

    /// network version 13 onward
    pub fn get_chain_randomness_v2_blocking(&self, round: ChainEpoch) -> anyhow::Result<[u8; 32]> {
        self.get_chain_randomness_blocking(round, false)
    }

    /// Randomness from the beacon entry that was used for `round`
    pub fn get_beacon_randomness_blocking(&self, round: ChainEpoch) -> anyhow::Result<[u8; 32]> {
        let beacon_entry = self.beacon_entry_for_epoch(round)?;
        Ok(digest(beacon_entry.signature()))
    }

    /// Non-blocking version of [`Self::get_beacon_randomness_blocking`]
    pub async fn get_beacon_randomness(&self, round: ChainEpoch) -> anyhow::Result<[u8; 32]> {
        let this = self.shallow_clone();
        tokio::task::spawn_blocking(move || this.get_beacon_randomness_blocking(round)).await?
    }

    /// Returns the beacon entry that was used for `epoch`, based on network
    /// version:
    /// <https://github.com/filecoin-project/lotus/blob/v1.36.0/chain/rand/rand.go#L192-L205>
    pub fn beacon_entry_for_epoch(&self, epoch: ChainEpoch) -> anyhow::Result<BeaconEntry> {
        let network_version = self.chain_config.network_version(epoch);
        if network_version >= NetworkVersion::V14 {
            self.beacon_entry_for_epoch_v3(epoch, network_version)
        } else if network_version == NetworkVersion::V13 {
            self.latest_beacon_entry_for_epoch(epoch, false)
        } else {
            self.latest_beacon_entry_for_epoch(epoch, true)
        }
    }

    fn latest_beacon_entry_for_epoch(
        &self,
        epoch: ChainEpoch,
        lookback: bool,
    ) -> anyhow::Result<BeaconEntry> {
        let rand_ts = self.get_beacon_randomness_tipset_blocking(epoch, lookback)?;
        Ok(self.chain_index.latest_beacon_entry(rand_ts)?)
    }

    fn beacon_entry_for_epoch_v3(
        &self,
        epoch: ChainEpoch,
        network_version: NetworkVersion,
    ) -> anyhow::Result<BeaconEntry> {
        if epoch < 0 {
            return self.latest_beacon_entry_for_epoch(epoch, false);
        }
        let mut rand_ts: Tipset = self.get_beacon_randomness_tipset_blocking(epoch, false)?;
        let (_, beacon) = self.beacon.beacon_for_epoch(epoch)?;
        let round = beacon.max_beacon_round_for_epoch(network_version, epoch)?;

        for _ in 0..20 {
            let cbe = &rand_ts.block_headers().first().beacon_entries;
            for v in cbe {
                if v.round() == round {
                    return Ok(v.clone());
                }
            }

            rand_ts = self.chain_index.load_required_tipset(rand_ts.parents())?;
        }

        bail!("didn't find beacon for round {round:?} (epoch {epoch:?})")
    }

    pub fn get_beacon_randomness_tipset_blocking(
        &self,
        round: ChainEpoch,
        lookback: bool,
    ) -> anyhow::Result<Tipset> {
        let ts = self.tipset.clone();

        if round > ts.epoch() {
            bail!("cannot draw randomness from the future");
        }

        let search_height = if round < 0 { 0 } else { round };

        let resolve = if lookback {
            ResolveNullTipset::TakeOlder
        } else {
            ResolveNullTipset::TakeNewer
        };

        self.chain_index
            .load_required_tipset_by_height_blocking(search_height, ts, resolve)
            .map_err(|e| e.into())
    }
}

impl Rand for ChainRand {
    fn get_chain_randomness(&self, round: ChainEpoch) -> anyhow::Result<[u8; 32]> {
        // Inspect and log errors as this is only called in `FVM` and errors are not propagated to the caller
        self.get_chain_randomness_v2_blocking(round)
            .inspect_err(|e| {
                tracing::warn!(
                    "get_chain_randomness failed, round: {round}, ts@{}: {}, error: {e:#?}",
                    self.tipset.epoch(),
                    self.tipset.key()
                );
            })
    }

    fn get_beacon_randomness(&self, round: ChainEpoch) -> anyhow::Result<[u8; 32]> {
        // Inspect and log errors as this is only called in `FVM` and errors are not propagated to the caller
        self.get_beacon_randomness_blocking(round).inspect_err(|e| {
            tracing::warn!(
                "get_beacon_randomness failed, round: {round}, ts@{}: {}, error: {e:#?}",
                self.tipset.epoch(),
                self.tipset.key()
            );
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
    let vrf_digest = digest(rbase);
    state.write_all(&vrf_digest)?;
    state.write_i64::<BigEndian>(round)?;
    state.write_all(entropy)?;
    let mut ret = [0u8; 32];
    ret.clone_from_slice(state.finalize().as_bytes());
    Ok(ret)
}

/// Computes a pseudo random 32 byte `Vec` from digest
pub fn draw_randomness_from_digest(
    digest: &[u8; 32],
    pers: i64,
    round: ChainEpoch,
    entropy: &[u8],
) -> anyhow::Result<[u8; 32]> {
    let mut state = Params::new().hash_length(32).to_state();
    state.write_i64::<BigEndian>(pers)?;
    state.write_all(digest)?;
    state.write_i64::<BigEndian>(round)?;
    state.write_all(entropy)?;
    let mut ret = [0u8; 32];
    ret.clone_from_slice(state.finalize().as_bytes());
    Ok(ret)
}

/// Computes a 256-bit digest.
/// See <https://github.com/filecoin-project/ref-fvm/blob/master/fvm/CHANGELOG.md#360-2023-08-18>
pub fn digest(rbase: &[u8]) -> [u8; 32] {
    blake2b_256(rbase)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beacon::{BeaconPoint, BeaconSchedule, mock_beacon::MockBeacon};
    use crate::blocks::{CachingBlockHeader, RawBlockHeader};
    use crate::chain::store::index::tests::persist_tipset;
    use crate::db::MemoryDB;
    use crate::make_height;
    use crate::networks::{Height, HeightInfo};
    use crate::shim::version::NetworkVersion;
    use crate::test_utils::dummy_ticket;
    use rstest::rstest;

    const MAINNET_GENESIS_TIMESTAMP: u64 = 1598306400;

    fn tipset_at(parent: Option<&Tipset>, epoch: ChainEpoch, rounds: &[u64]) -> Tipset {
        Tipset::from(CachingBlockHeader::new(RawBlockHeader {
            parents: parent.map(|p| p.key().clone()).unwrap_or_default(),
            epoch,
            timestamp: epoch as u64,
            ticket: dummy_ticket(epoch as u8),
            beacon_entries: rounds
                .iter()
                .map(|r| BeaconEntry::new(*r, vec![*r as u8; 32]))
                .collect(),
            ..Default::default()
        }))
    }

    fn chain_rand(
        chain_config: ChainConfig,
        beacon: BeaconSchedule,
        chain: &[Tipset],
    ) -> ChainRand {
        let db: Arc<MemoryDB> = Arc::new(MemoryDB::default());
        for tipset in chain {
            persist_tipset(tipset, &db);
        }
        let genesis = chain
            .first()
            .expect("chain starts at genesis")
            .shallow_clone();
        let head = chain.last().expect("chain has a head").shallow_clone();
        let chain_index = ChainIndex::new(db, genesis);
        ChainRand::new(Arc::new(chain_config), head, chain_index, Arc::new(beacon))
    }

    #[test]
    fn beacon_entry_at_genesis_should_return_latest_tipset_entry() {
        let chain_config = ChainConfig::mainnet();
        let beacon: BeaconSchedule = chain_config.get_beacon_schedule(MAINNET_GENESIS_TIMESTAMP);
        let genesis = tipset_at(None, 0, &[0]);

        let entry = chain_rand(chain_config, beacon, &[genesis])
            .beacon_entry_for_epoch(0)
            .expect("epoch 0 resolves to the genesis beacon entry");

        assert_eq!(entry.round(), 0);
        assert_eq!(entry.signature(), &vec![0; 32]);
    }

    const BEFORE_NULLS: ChainEpoch = 10;
    const AFTER_NULLS: ChainEpoch = 16;

    fn chain_with_null_rounds() -> Vec<Tipset> {
        let mut chain = vec![tipset_at(None, 0, &[0])];
        for epoch in 1..=BEFORE_NULLS {
            let parent = chain.last().expect("non-empty").shallow_clone();
            chain.push(tipset_at(Some(&parent), epoch, &[epoch as u64]));
        }
        let parent = chain.last().expect("non-empty").shallow_clone();
        let covered: Vec<u64> = ((BEFORE_NULLS + 1) as u64..=AFTER_NULLS as u64).collect();
        chain.push(tipset_at(Some(&parent), AFTER_NULLS, &covered));
        chain
    }

    fn chain_config_with_upgrade(upgrade: Option<(Height, HeightInfo)>) -> ChainConfig {
        ChainConfig {
            genesis_network: NetworkVersion::V12,
            height_infos: upgrade.into_iter().collect(),
            ..ChainConfig::devnet()
        }
    }

    const NULL_EPOCH: ChainEpoch = 14;

    // for each upgrade we should fetch the randomness differently
    // https://github.com/filecoin-project/lotus/blob/70e807ea17bddeec4e5551540f345e2dee28d53e/chain/rand/rand_test.go#L27
    #[rstest]
    #[case::before_nv13(None, BEFORE_NULLS as u64)]
    #[case::at_nv13(Some(make_height!(Hyperdrive, 0)), AFTER_NULLS as u64)]
    #[case::from_nv14(Some(make_height!(Chocolate, 0)), NULL_EPOCH as u64)]
    fn null_round_beacon_entry(
        #[case] upgrade: Option<(Height, HeightInfo)>,
        #[case] expected_round: u64,
    ) {
        let chain_config = chain_config_with_upgrade(upgrade);
        let beacon = BeaconSchedule(vec![BeaconPoint::new(0, MockBeacon::default())]);

        let entry = chain_rand(chain_config, beacon, &chain_with_null_rounds())
            .beacon_entry_for_epoch(NULL_EPOCH)
            .expect("resolves through the null run");

        assert_eq!(entry.round(), expected_round);
    }
}
