// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(dead_code)]

use crate::utils::encoding::hex;
use std::sync::LazyLock;
use std::time::Duration;
use std::{borrow::Cow, num::NonZeroUsize};

use super::{
    beacon_entries::BeaconEntry,
    signatures::{
        PublicKeyOnG1, PublicKeyOnG2, SignatureOnG1, SignatureOnG2, verify_messages_chained,
    },
};
use crate::prelude::*;
use crate::shim::clock::ChainEpoch;
use crate::shim::version::NetworkVersion;
use crate::utils::cache::SizeTrackingCache;
use crate::utils::misc::env::is_env_truthy;
use crate::utils::net::global_http_client;
use ambassador::{Delegate, delegatable_trait};
use backon::{ExponentialBuilder, Retryable};
use bls_signatures::Serialize as _;
use nonzero_ext::nonzero;
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};
use tracing::debug;
use url::Url;

/// Environmental Variable to ignore `Drand`. Lotus parallel is
/// `LOTUS_IGNORE_DRAND`
pub const IGNORE_DRAND_VAR: &str = "FOREST_IGNORE_DRAND";

/// Whether to ignore `Drand`.
pub static IGNORE_DRAND: LazyLock<bool> = LazyLock::new(|| is_env_truthy(IGNORE_DRAND_VAR));

/// Type of the `drand` network. `mainnet` is chained and `quicknet` is unchained.
/// For the details, see <https://github.com/filecoin-project/FIPs/blob/1bd887028ac1b50b6f2f94913e07ede73583da5b/FIPS/fip-0063.md#specification>
#[derive(PartialEq, Eq, Copy, Clone, Debug, SerdeSerialize, SerdeDeserialize)]
pub enum DrandNetwork {
    Mainnet,
    Quicknet,
    Incentinet,
}

impl DrandNetwork {
    pub fn is_unchained(&self) -> bool {
        matches!(self, Self::Quicknet)
    }

    pub fn is_chained(&self) -> bool {
        !self.is_unchained()
    }
}

#[derive(Debug, Clone, SerdeSerialize, SerdeDeserialize, Eq, PartialEq)]
/// Configuration used when initializing a `Drand` beacon.
pub struct DrandConfig<'a> {
    /// Public endpoints of the `Drand` service.
    /// See <https://drand.love/developer/http-api/#public-endpoints>
    pub servers: Vec<Url>,
    /// Info about the beacon chain, used to verify correctness of endpoint.
    pub chain_info: ChainInfo<'a>,
    /// Network type
    pub network_type: DrandNetwork,
}

/// Contains the vector of `BeaconPoint`, which are mappings of epoch to the
/// `Randomness` beacons used.
pub struct BeaconSchedule(pub Vec<BeaconPoint>);

impl BeaconSchedule {
    /// Returns the beacon entries for a given epoch.
    /// When the beacon for the given epoch is on a new beacon, randomness
    /// entries are taken from the last two rounds.
    pub async fn beacon_entries_for_block(
        &self,
        network_version: NetworkVersion,
        epoch: ChainEpoch,
        parent_epoch: ChainEpoch,
        prev: &BeaconEntry,
    ) -> anyhow::Result<Vec<BeaconEntry>> {
        let (cb_epoch, curr_beacon) = self.beacon_for_epoch(epoch)?;
        // Before quicknet upgrade, we had "chained" beacons, and so required two entries at a fork
        if curr_beacon.network().is_chained() {
            let (pb_epoch, _) = self.beacon_for_epoch(parent_epoch)?;
            if cb_epoch != pb_epoch {
                // Fork logic, take entries from the last two rounds of the new beacon.
                let round = curr_beacon.max_beacon_round_for_epoch(network_version, epoch)?;
                let out = vec![
                    curr_beacon.entry(round - 1).await?,
                    curr_beacon.entry(round).await?,
                ];
                return Ok(out);
            }
        }

        let max_round = curr_beacon.max_beacon_round_for_epoch(network_version, epoch)?;
        // We don't expect this to ever be the case
        if max_round == prev.round() {
            tracing::warn!(
                "Unexpected `max_round == prev.round()` condition, network_version: {network_version:?}, max_round: {max_round}, prev_round: {}",
                prev.round()
            );
            // Our chain has encountered two epochs before beacon chain has elapsed one,
            // return no beacon entries for this epoch.
            return Ok(vec![]);
        }

        let prev_round = if prev.round() == 0 {
            max_round - 1
        } else {
            prev.round()
        };

        let mut out = Vec::with_capacity(2);
        if curr_beacon.network().is_unchained() {
            // Newest-first, so a large gap fails on its first unavailable round:
            // <https://github.com/filecoin-project/lotus/blob/v1.35.1/chain/beacon/beacon.go#L152>
            for covered_epoch in (parent_epoch + 1..=epoch).rev() {
                let round =
                    curr_beacon.max_beacon_round_for_epoch(network_version, covered_epoch)?;
                out.push(curr_beacon.entry(round).await?);
            }
            out.reverse();
            Ok(out)
        } else {
            // Rounds elapsed since the last chain epoch, newest-first as above.
            for round in (prev_round + 1..=max_round).rev() {
                out.push(curr_beacon.entry(round).await?);
            }
            out.reverse();
            Ok(out)
        }
    }

    pub fn beacon_for_epoch(&self, epoch: ChainEpoch) -> anyhow::Result<(ChainEpoch, &BeaconImpl)> {
        // Iterate over beacon schedule to find the latest randomness beacon to use.
        self.0
            .iter()
            .rev()
            .find(|upgrade| epoch >= upgrade.height)
            .map(|upgrade| (upgrade.height, &upgrade.beacon))
            .context("Invalid beacon schedule, no valid beacon")
    }
}

#[derive(Delegate, derive_more::From)]
#[delegate(Beacon)]
pub enum BeaconImpl {
    Drand(DrandBeacon),
    #[cfg(test)]
    Mock(crate::beacon::mock_beacon::MockBeacon),
}

/// Contains height at which the beacon is activated, as well as the beacon
/// itself.
pub struct BeaconPoint {
    height: ChainEpoch,
    beacon: BeaconImpl,
}

impl BeaconPoint {
    pub fn new(height: ChainEpoch, beacon: impl Into<BeaconImpl>) -> Self {
        let beacon = beacon.into();
        Self { height, beacon }
    }
}

/// Trait used as the interface to be able to retrieve bytes from a randomness
/// beacon.
#[delegatable_trait]
pub trait Beacon {
    /// Gets the `drand` network
    fn network(&self) -> DrandNetwork;

    /// Verify beacon entries that are sorted by round.
    fn verify_entries(&self, entries: &[BeaconEntry], prev: &BeaconEntry) -> anyhow::Result<bool>;

    /// Returns a `BeaconEntry` given a round. It fetches the `BeaconEntry` from a `Drand` node over [`gRPC`](https://grpc.io/)
    /// In the future, we will cache values, and support streaming.
    async fn entry(&self, round: u64) -> anyhow::Result<BeaconEntry>;

    /// Returns the most recent beacon round for the given Filecoin chain epoch.
    fn max_beacon_round_for_epoch(
        &self,
        network_version: NetworkVersion,
        fil_epoch: ChainEpoch,
    ) -> anyhow::Result<u64>;

    /// Unix timestamp (seconds) at which the given `drand` round is produced, or `None`
    /// (the default) if not derivable - meaning callers should not wait.
    fn beacon_round_timestamp(&self, _round: u64) -> Option<u64> {
        None
    }

    /// Fetches `round`, waiting only until that `drand` round is produced (per
    /// [`Self::beacon_round_timestamp`]) rather than until the later Filecoin epoch.
    async fn entry_when_available(&self, round: u64) -> anyhow::Result<BeaconEntry> {
        if let Some(round_ts) = self.beacon_round_timestamp(round) {
            let wait = beacon_round_wait(round_ts, chrono::Utc::now().timestamp());
            if !wait.is_zero() {
                tokio::time::sleep(wait).await;
            }
        }
        self.entry(round).await
    }
}

/// How long to wait until the `drand` round produced at `round_ts` (Unix seconds) is
/// available, given the current time `now`. Includes a 1s publish-latency buffer.
pub(crate) fn beacon_round_wait(round_ts: u64, now: i64) -> Duration {
    let now = now.max(0) as u64;
    Duration::from_secs(round_ts.saturating_add(1).saturating_sub(now))
}

#[derive(SerdeDeserialize, SerdeSerialize, Debug, Clone, PartialEq, Eq, Default)]
/// Contains all the info about a `Drand` beacon chain.
/// API reference: <https://drand.love/developer/http-api/#info>
/// note: `groupHash` does not exist in docs currently, but is returned.
pub struct ChainInfo<'a> {
    pub public_key: Cow<'a, str>,
    pub period: i32,
    pub genesis_time: i32,
    pub hash: Cow<'a, str>,
    #[serde(rename = "groupHash")]
    pub group_hash: Cow<'a, str>,
}

#[derive(SerdeDeserialize, SerdeSerialize, Debug, Clone)]
/// JSON beacon entry format. This matches the `drand` round JSON serialization
/// API reference: <https://drand.love/developer/http-api/#public-round>.
pub struct BeaconEntryJson {
    round: u64,
    randomness: String,
    signature: String,
    previous_signature: Option<String>,
}

/// `Drand` randomness beacon that can be used to generate randomness for the
/// Filecoin chain. Primary use is to satisfy the [Beacon] trait.
pub struct DrandBeacon {
    servers: Vec<Url>,
    hash: String,
    network: DrandNetwork,

    public_key: Vec<u8>,
    /// Interval between beacons, in seconds.
    interval: u64,
    drand_gen_time: u64,
    fil_gen_time: u64,
    fil_round_time: u64,

    /// Keeps track of verified beacon entries.
    verified_beacons: SizeTrackingCache<u64, Arc<BeaconEntry>>,
}

impl DrandBeacon {
    /// Construct a new `DrandBeacon`.
    pub fn new(genesis_ts: u64, interval: u64, config: &DrandConfig<'_>) -> Self {
        assert_ne!(genesis_ts, 0, "Genesis timestamp cannot be 0");
        const CACHE_SIZE: NonZeroUsize = nonzero!(1000usize);
        Self {
            servers: config.servers.clone(),
            hash: config.chain_info.hash.to_string(),
            network: config.network_type,
            public_key: hex::decode(config.chain_info.public_key.as_ref())
                .expect("invalid static encoding of drand hex public key"),
            interval: config.chain_info.period as u64,
            drand_gen_time: config.chain_info.genesis_time as u64,
            fil_round_time: interval,
            fil_gen_time: genesis_ts,
            verified_beacons: if config.network_type.is_unchained() {
                SizeTrackingCache::new_with_metrics("verified_beacons", CACHE_SIZE)
            } else {
                SizeTrackingCache::new_without_metrics_registry("verified_beacons", CACHE_SIZE)
            },
        }
    }

    fn is_verified(&self, entry: &BeaconEntry) -> bool {
        self.verified_beacons.get(&entry.round()).as_deref() == Some(entry)
    }

    /// Verify-and-cache a freshly fetched entry: `verify_entries` inserts verified rounds
    /// into `verified_beacons`. Only unchained rounds verify standalone, so chained ones are skipped.
    fn cache_fetched_entry(&self, entry: &BeaconEntry) {
        if !self.network.is_unchained() {
            return;
        }
        if !matches!(
            self.verify_entries(std::slice::from_ref(entry), entry),
            Ok(true)
        ) {
            debug!(
                round = entry.round(),
                "fetched drand entry failed verification"
            );
        }
    }
}

impl Beacon for DrandBeacon {
    fn network(&self) -> DrandNetwork {
        self.network
    }

    fn verify_entries<'a>(
        &self,
        entries: &'a [BeaconEntry],
        prev: &'a BeaconEntry,
    ) -> anyhow::Result<bool> {
        let mut validated = vec![];
        let is_valid = if self.network.is_unchained() {
            let mut messages = vec![];
            let mut signatures = vec![];
            let pk = PublicKeyOnG2::from_bytes(&self.public_key)?;
            {
                // Deduplicate by round. See Lotus issue: https://github.com/filecoin-project/lotus/issues/13349
                for entry in entries.iter().unique_by(|e| e.round()) {
                    if self.is_verified(entry) {
                        continue;
                    }

                    messages.push(BeaconEntry::message_unchained(entry.round()));
                    signatures.push(SignatureOnG1::from_bytes(entry.signature())?);
                    validated.push(entry);
                }
            }

            pk.verify_batch(
                messages.iter().map(AsRef::as_ref).collect_vec().as_slice(),
                signatures.iter().collect_vec().as_slice(),
            )
        } else {
            let mut messages = vec![];
            let mut signatures = vec![];

            let pk = PublicKeyOnG1::from_bytes(&self.public_key)?;
            {
                let prev_curr_pairs = std::iter::once(prev)
                    .chain(entries.iter())
                    .unique_by(|e| e.round())
                    .tuple_windows::<(_, _)>();
                for (prev, curr) in prev_curr_pairs {
                    if prev.round() > 0 && !self.is_verified(curr) {
                        messages.push(BeaconEntry::message_chained(curr.round(), prev.signature()));
                        signatures.push(SignatureOnG2::from_bytes(curr.signature())?);
                        validated.push(curr);
                    }
                }
            }

            verify_messages_chained(
                &pk,
                messages.iter().map(AsRef::as_ref).collect_vec().as_slice(),
                &signatures,
            )
        };

        if is_valid && !validated.is_empty() {
            let capacity = self.verified_beacons.capacity() as usize;
            if capacity < validated.len() {
                tracing::warn!(%capacity, validated_len=%validated.len(), "verified_beacons.capacity() is too small");
            }
            for entry in validated {
                self.verified_beacons
                    .insert(entry.round(), Arc::new(entry.clone()));
            }
        }

        Ok(is_valid)
    }

    async fn entry(&self, round: u64) -> anyhow::Result<BeaconEntry> {
        if let Some(cached_entry) = self.verified_beacons.get(&round) {
            return Ok(Arc::unwrap_or_clone(cached_entry));
        }

        async fn fetch_entry_from_url(url: impl reqwest::IntoUrl) -> anyhow::Result<BeaconEntry> {
            let resp: BeaconEntryJson = global_http_client()
                .get(url)
                // More tolerance on slow networks
                .timeout(Duration::from_secs(15))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            anyhow::Ok(BeaconEntry::new(resp.round, hex::decode(resp.signature)?))
        }

        async fn fetch_entry(
            urls: impl Iterator<Item = impl reqwest::IntoUrl>,
        ) -> anyhow::Result<BeaconEntry> {
            let mut errors = vec![];
            for url in urls {
                match fetch_entry_from_url(url).await {
                    Ok(e) => return Ok(e),
                    Err(e) => errors.push(e),
                }
            }
            anyhow::bail!(
                "Aggregated errors:\n{}",
                errors.into_iter().map(|e| e.to_string()).join("\n\n")
            );
        }

        let urls: Vec<_> = self
            .servers
            .iter()
            .map(|server| anyhow::Ok(server.join(&format!("{}/public/{round}", self.hash))?))
            .try_collect()?;
        let entry = (|| fetch_entry(urls.iter().cloned()))
            .retry(ExponentialBuilder::default())
            .notify(|err, dur| {
                debug!(
                    "retrying fetch_entry after {}: {err:#}",
                    humantime::format_duration(dur)
                );
            })
            .await?;
        // Callers assume the entry is for the round they asked for. Round 0 is served
        // as "latest", so it answers with a different round by design:
        // <https://github.com/drand/drand/blob/v2.1.6/handler/http/server.go#L367>
        anyhow::ensure!(
            round == 0 || entry.round() == round,
            "drand returned round {} for round {round}",
            entry.round()
        );
        self.cache_fetched_entry(&entry);
        Ok(entry)
    }

    fn max_beacon_round_for_epoch(
        &self,
        network_version: NetworkVersion,
        fil_epoch: ChainEpoch,
    ) -> anyhow::Result<u64> {
        // Lotus wraps and returns a garbage round instead:
        // <https://github.com/filecoin-project/lotus/blob/v1.35.1/chain/beacon/drand/drand.go#L227>
        let out_of_range = || anyhow::anyhow!("epoch {fil_epoch} has no drand round");
        let latest_ts = u64::try_from(fil_epoch)
            .ok()
            .and_then(|epoch| epoch.checked_mul(self.fil_round_time))
            .and_then(|ts| ts.checked_add(self.fil_gen_time))
            .and_then(|ts| ts.checked_sub(self.fil_round_time))
            .ok_or_else(out_of_range)?;
        if network_version <= NetworkVersion::V15 {
            // Algorithm for nv15 and below
            Ok(latest_ts
                .checked_sub(self.drand_gen_time)
                .ok_or_else(out_of_range)?
                / self.interval)
        } else {
            // Algorithm for nv16 and above
            if latest_ts < self.drand_gen_time {
                return Ok(1);
            }

            let from_genesis = latest_ts - self.drand_gen_time;
            // we take the time from genesis divided by the periods in seconds, that
            // gives us the number of periods since genesis.  We also add +1 because
            // round 1 starts at genesis time.
            Ok(from_genesis / self.interval + 1)
        }
    }

    fn beacon_round_timestamp(&self, round: u64) -> Option<u64> {
        // Round 1 is produced at drand genesis; round n at genesis + (n-1)*period.
        Some(
            self.drand_gen_time
                .saturating_add(round.saturating_sub(1).saturating_mul(self.interval)),
        )
    }
}
