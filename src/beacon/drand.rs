// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;
use std::{borrow::Cow, num::NonZeroUsize};

use super::{
    beacon_entries::BeaconEntry,
    signatures::{
        verify_messages_chained, PublicKeyOnG1, PublicKeyOnG2, SignatureOnG1, SignatureOnG2,
    },
};
use crate::shim::clock::ChainEpoch;
use crate::shim::version::NetworkVersion;
use crate::utils::net::global_http_client;
use anyhow::Context as _;
use async_trait::async_trait;
use bls_signatures::Serialize as _;
use itertools::Itertools;
use lru::LruCache;
use parking_lot::RwLock;
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

/// Environmental Variable to ignore `Drand`. Lotus parallel is
/// `LOTUS_IGNORE_DRAND`
pub const IGNORE_DRAND_VAR: &str = "IGNORE_DRAND";

/// Type of the `drand` network. `mainnet` is chained and `quicknet` is unchained.
/// For the details, see <https://github.com/filecoin-project/FIPs/blob/1bd887028ac1b50b6f2f94913e07ede73583da5b/FIPS/fip-0063.md#specification>
#[derive(PartialEq, Eq, Copy, Clone)]
pub enum DrandNetwork {
    Mainnet,
    Quicknet,
    Incentinet,
}

impl DrandNetwork {
    pub fn is_unchained(&self) -> bool {
        matches!(self, Self::Quicknet)
    }
}

#[derive(Clone)]
/// Configuration used when initializing a `Drand` beacon.
pub struct DrandConfig<'a> {
    /// URL endpoint to send JSON HTTP requests to.
    pub server: &'static str,
    /// Info about the beacon chain, used to verify correctness of endpoint.
    pub chain_info: ChainInfo<'a>,
    /// Network type
    pub network_type: DrandNetwork,
}

/// Contains the vector of `BeaconPoint`, which are mappings of epoch to the
/// `Randomness` beacons used.
pub struct BeaconSchedule(pub Vec<BeaconPoint>);

impl BeaconSchedule {
    /// Constructs a new, empty `BeaconSchedule<T>` with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        BeaconSchedule(Vec::with_capacity(capacity))
    }

    /// Returns the beacon entries for a given epoch.
    /// When the beacon for the given epoch is on a new beacon, randomness
    /// entries are taken from the last two rounds.
    pub async fn beacon_entries_for_block(
        &self,
        network_version: NetworkVersion,
        epoch: ChainEpoch,
        parent_epoch: ChainEpoch,
        prev: &BeaconEntry,
    ) -> Result<Vec<BeaconEntry>, anyhow::Error> {
        let (_cb_epoch, curr_beacon) = self.beacon_for_epoch(epoch)?;
        let (_pb_epoch, _) = self.beacon_for_epoch(parent_epoch)?;
        let max_round = curr_beacon.max_beacon_round_for_epoch(network_version, epoch);
        // We don't expect this to ever be the case
        if max_round == prev.round() {
            // Our chain has encountered two epochs before beacon chain has elapsed one,
            // return no beacon entries for this epoch.
            return Ok(vec![]);
        }
        // TODO(forest): https://github.com/ChainSafe/forest/issues/3572
        //               this is a sketchy way to handle the genesis block not
        //               having a entry
        let prev_round = if prev.round() == 0 {
            max_round - 1
        } else {
            prev.round()
        };

        // We only ever need one entry after nv22 (FIP-0063)
        if network_version > NetworkVersion::V21 {
            let entry = curr_beacon.entry(max_round).await?;
            Ok(vec![entry])
        } else {
            let mut cur = max_round;
            let mut out = Vec::new();
            while cur > prev_round {
                // Push all entries from rounds elapsed since the last chain epoch.
                let entry = curr_beacon.entry(cur).await?;
                cur = entry.round() - 1;
                out.push(entry);
            }
            out.reverse();
            Ok(out)
        }
    }

    pub fn beacon_for_epoch(&self, epoch: ChainEpoch) -> anyhow::Result<(ChainEpoch, &dyn Beacon)> {
        // Iterate over beacon schedule to find the latest randomness beacon to use.
        self.0
            .iter()
            .rev()
            .find(|upgrade| epoch >= upgrade.height)
            .map(|upgrade| (upgrade.height, upgrade.beacon.as_ref()))
            .context("Invalid beacon schedule, no valid beacon")
    }
}

/// Contains height at which the beacon is activated, as well as the beacon
/// itself.
pub struct BeaconPoint {
    pub height: ChainEpoch,
    pub beacon: Box<dyn Beacon>,
}

#[async_trait]
/// Trait used as the interface to be able to retrieve bytes from a randomness
/// beacon.
pub trait Beacon
where
    Self: Send + Sync + 'static,
{
    /// Verify beacon entries that are sorted by round.
    fn verify_entries(
        &self,
        entries: &[BeaconEntry],
        prev: &BeaconEntry,
    ) -> Result<bool, anyhow::Error>;

    /// Returns a `BeaconEntry` given a round. It fetches the `BeaconEntry` from a `Drand` node over [`gRPC`](https://grpc.io/)
    /// In the future, we will cache values, and support streaming.
    async fn entry(&self, round: u64) -> anyhow::Result<BeaconEntry>;

    /// Returns the most recent beacon round for the given Filecoin chain epoch.
    fn max_beacon_round_for_epoch(
        &self,
        network_version: NetworkVersion,
        fil_epoch: ChainEpoch,
    ) -> u64;
}

#[async_trait]
impl Beacon for Box<dyn Beacon> {
    fn verify_entries(
        &self,
        entries: &[BeaconEntry],
        prev: &BeaconEntry,
    ) -> Result<bool, anyhow::Error> {
        self.as_ref().verify_entries(entries, prev)
    }

    async fn entry(&self, round: u64) -> Result<BeaconEntry, anyhow::Error> {
        self.as_ref().entry(round).await
    }

    fn max_beacon_round_for_epoch(
        &self,
        network_version: NetworkVersion,
        fil_epoch: ChainEpoch,
    ) -> u64 {
        self.as_ref()
            .max_beacon_round_for_epoch(network_version, fil_epoch)
    }
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
    server: &'static str,
    hash: String,
    network: DrandNetwork,

    public_key: Vec<u8>,
    /// Interval between beacons, in seconds.
    interval: u64,
    drand_gen_time: u64,
    fil_gen_time: u64,
    fil_round_time: u64,

    /// Keeps track of verified beacon entries.
    verified_beacons: RwLock<LruCache<u64, BeaconEntry>>,
}

impl DrandBeacon {
    /// Construct a new `DrandBeacon`.
    pub fn new(genesis_ts: u64, interval: u64, config: &DrandConfig<'_>) -> Self {
        assert_ne!(genesis_ts, 0, "Genesis timestamp cannot be 0");
        const CACHE_SIZE: usize = 1000;
        Self {
            server: config.server,
            hash: config.chain_info.hash.to_string(),
            network: config.network_type,
            public_key: hex::decode(config.chain_info.public_key.as_ref())
                .expect("invalid static encoding of drand hex public key"),
            interval: config.chain_info.period as u64,
            drand_gen_time: config.chain_info.genesis_time as u64,
            fil_round_time: interval,
            fil_gen_time: genesis_ts,
            verified_beacons: RwLock::new(LruCache::new(
                NonZeroUsize::new(CACHE_SIZE).expect("Infallible"),
            )),
        }
    }
}

#[async_trait]
impl Beacon for DrandBeacon {
    fn verify_entries<'a>(
        &self,
        entries: &'a [BeaconEntry],
        mut prev: &'a BeaconEntry,
    ) -> Result<bool, anyhow::Error> {
        let mut validated = vec![];
        let is_valid = if self.network.is_unchained() {
            let mut messages = vec![];
            let mut signatures = vec![];
            let pk = PublicKeyOnG2::from_bytes(&self.public_key)?;
            {
                let cache = self.verified_beacons.read();
                for entry in entries.iter() {
                    if cache.contains(&entry.round()) {
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
                let cache = self.verified_beacons.read();
                for curr in entries.iter() {
                    if prev.round() > 0 && !cache.contains(&curr.round()) {
                        messages.push(BeaconEntry::message_chained(curr.round(), prev.signature()));
                        signatures.push(SignatureOnG2::from_bytes(curr.signature())?);
                        validated.push(curr);
                    }

                    prev = curr;
                }
            }

            verify_messages_chained(
                &pk,
                messages.iter().map(AsRef::as_ref).collect_vec().as_slice(),
                &signatures,
            )
        };

        if is_valid && !validated.is_empty() {
            let mut cache = self.verified_beacons.write();
            assert!(cache.cap().get() < validated.len());
            for entry in validated {
                cache.put(entry.round(), entry.clone());
            }
        }

        Ok(is_valid)
    }

    async fn entry(&self, round: u64) -> anyhow::Result<BeaconEntry> {
        let cached: Option<BeaconEntry> = self.verified_beacons.read().peek(&round).cloned();
        match cached {
            Some(cached_entry) => Ok(cached_entry),
            None => {
                async fn fetch_entry(url: impl reqwest::IntoUrl) -> anyhow::Result<BeaconEntry> {
                    let resp: BeaconEntryJson = global_http_client()
                        .get(url)
                        .timeout(Duration::from_secs(1))
                        .send()
                        .await?
                        .error_for_status()?
                        .json()
                        .await?;
                    anyhow::Ok(BeaconEntry::new(resp.round, hex::decode(resp.signature)?))
                }

                let url = format!("{}/{}/public/{round}", self.server, self.hash);
                Ok(
                    backoff::future::retry(backoff::ExponentialBackoff::default(), || async {
                        Ok(fetch_entry(&url).await?)
                    })
                    .await?,
                )
            }
        }
    }

    fn max_beacon_round_for_epoch(
        &self,
        network_version: NetworkVersion,
        fil_epoch: ChainEpoch,
    ) -> u64 {
        let latest_ts =
            ((fil_epoch as u64 * self.fil_round_time) + self.fil_gen_time) - self.fil_round_time;
        if network_version <= NetworkVersion::V15 {
            // Algorithm for nv15 and below
            (latest_ts - self.drand_gen_time) / self.interval
        } else {
            // Algorithm for nv16 and above
            if latest_ts < self.drand_gen_time {
                return 1;
            }

            let from_genesis = latest_ts - self.drand_gen_time;
            // we take the time from genesis divided by the periods in seconds, that
            // gives us the number of periods since genesis.  We also add +1 because
            // round 1 starts at genesis time.
            from_genesis / self.interval + 1
        }
    }
}
