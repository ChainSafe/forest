// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::beacon_entries::BeaconEntry;
use ahash::AHashMap;
use async_std::sync::RwLock;
use async_trait::async_trait;
use bls_signatures::{PublicKey, Serialize, Signature};
use byteorder::{BigEndian, WriteBytesExt};
use clock::ChainEpoch;
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};
use sha2::Digest;
use std::borrow::Cow;
use std::error;
use std::sync::Arc;

/// Enviromental Variable to ignore Drand. Lotus parallel is LOTUS_IGNORE_DRAND
pub const IGNORE_DRAND_VAR: &str = "IGNORE_DRAND";

/// Coeffiencients of the publicly available Drand keys.
/// This is shared by all participants on the Drand network.
#[derive(Clone, Debug, SerdeSerialize, SerdeDeserialize)]
pub struct DrandPublic {
    pub coefficient: Vec<u8>,
}

impl DrandPublic {
    pub fn key(&self) -> PublicKey {
        PublicKey::from_bytes(&self.coefficient).unwrap()
    }
}

#[derive(Clone)]
pub struct DrandConfig<'a> {
    pub server: &'static str,
    pub chain_info: ChainInfo<'a>,
}

pub struct BeaconSchedule<T>(pub Vec<BeaconPoint<T>>);

impl<T> BeaconSchedule<T>
where
    T: Beacon,
{
    pub async fn beacon_entries_for_block(
        &self,
        epoch: ChainEpoch,
        parent_epoch: ChainEpoch,
        prev: &BeaconEntry,
    ) -> Result<Vec<BeaconEntry>, Box<dyn error::Error>> {
        let (cb_epoch, curr_beacon) = self.beacon_for_epoch(epoch)?;
        let (pb_epoch, _) = self.beacon_for_epoch(parent_epoch)?;
        if cb_epoch != pb_epoch {
            // Fork logic
            let round = curr_beacon.max_beacon_round_for_epoch(epoch);
            let mut entries = Vec::with_capacity(2);
            entries.push(curr_beacon.entry(round - 1).await?);
            entries.push(curr_beacon.entry(round).await?);
            return Ok(entries);
        }
        let max_round = curr_beacon.max_beacon_round_for_epoch(epoch);
        if max_round == prev.round() {
            return Ok(vec![]);
        }
        // TODO: this is a sketchy way to handle the genesis block not having a beacon entry
        let prev_round = if prev.round() == 0 {
            max_round - 1
        } else {
            prev.round()
        };

        let mut cur = max_round;
        let mut out = Vec::new();
        while cur > prev_round {
            let entry = curr_beacon.entry(cur).await?;
            cur = entry.round() - 1;
            out.push(entry);
        }
        out.reverse();
        Ok(out)
    }

    pub fn beacon_for_epoch(
        &self,
        epoch: ChainEpoch,
    ) -> Result<(ChainEpoch, &T), Box<dyn error::Error>> {
        Ok(self
            .0
            .iter()
            .rev()
            .find(|upgrade| epoch >= upgrade.height)
            .map(|upgrade| (upgrade.height, upgrade.beacon.as_ref()))
            .ok_or("Invalid beacon schedule, no valid beacon")?)
    }
}

pub struct BeaconPoint<T> {
    pub height: ChainEpoch,
    pub beacon: Arc<T>,
}

#[async_trait]
pub trait Beacon
where
    Self: Sized,
{
    /// Verify a new beacon entry against the most recent one before it.
    async fn verify_entry(
        &self,
        curr: &BeaconEntry,
        prev: &BeaconEntry,
    ) -> Result<bool, Box<dyn error::Error>>;

    /// Returns a BeaconEntry given a round. It fetches the BeaconEntry from a Drand node over GRPC
    /// In the future, we will cache values, and support streaming.
    async fn entry(&self, round: u64) -> Result<BeaconEntry, Box<dyn error::Error>>;

    fn max_beacon_round_for_epoch(&self, fil_epoch: ChainEpoch) -> u64;
}

#[derive(SerdeDeserialize, SerdeSerialize, Debug, Clone, PartialEq, Default)]
pub struct ChainInfo<'a> {
    pub public_key: Cow<'a, str>,
    pub period: i32,
    pub genesis_time: i32,
    pub hash: Cow<'a, str>,
    #[serde(rename = "groupHash")]
    pub group_hash: Cow<'a, str>,
}

#[derive(SerdeDeserialize, SerdeSerialize, Debug, Clone)]
pub struct BeaconEntryJson {
    round: u64,
    randomness: String,
    signature: String,
    previous_signature: String,
}

pub struct DrandBeacon {
    url: &'static str,

    pub_key: DrandPublic,
    /// Interval between beacons, in seconds.
    interval: u64,
    drand_gen_time: u64,
    fil_gen_time: u64,
    fil_round_time: u64,

    /// Keeps track of computed beacon entries.
    local_cache: RwLock<AHashMap<u64, BeaconEntry>>,
}

impl DrandBeacon {
    /// Construct a new DrandBeacon.
    pub async fn new(
        genesis_ts: u64,
        interval: u64,
        config: &DrandConfig<'_>,
    ) -> Result<Self, Box<dyn error::Error>> {
        if genesis_ts == 0 {
            panic!("Genesis timestamp cannot be 0")
        }

        let chain_info = &config.chain_info;

        if cfg!(debug_assertions) {
            let remote_chain_info: ChainInfo = surf::get(&format!("{}/info", &config.server))
                .recv_json()
                .await?;
            debug_assert!(&remote_chain_info == chain_info);
        }

        Ok(Self {
            url: config.server,
            pub_key: DrandPublic {
                coefficient: hex::decode(chain_info.public_key.as_ref())?,
            },
            interval: chain_info.period as u64,
            drand_gen_time: chain_info.genesis_time as u64,
            fil_round_time: interval,
            fil_gen_time: genesis_ts,
            local_cache: Default::default(),
        })
    }
}
/// This struct allows you to talk to a Drand node over GRPC.
/// Use this to source randomness and to verify Drand beacon entries.
#[async_trait]
impl Beacon for DrandBeacon {
    async fn verify_entry(
        &self,
        curr: &BeaconEntry,
        prev: &BeaconEntry,
    ) -> Result<bool, Box<dyn error::Error>> {
        // TODO: Handle Genesis better
        if prev.round() == 0 {
            return Ok(true);
        }

        // Hash the messages
        let mut msg: Vec<u8> = Vec::with_capacity(104);
        msg.extend_from_slice(prev.data());
        msg.write_u64::<BigEndian>(curr.round())?;
        // H(prev sig | curr_round)
        let digest = sha2::Sha256::digest(&msg);
        // Hash to G2
        let digest = bls_signatures::hash(&digest);
        // Signature
        let sig = Signature::from_bytes(curr.data())?;
        let sig_match = bls_signatures::verify(&sig, &[digest], &[self.pub_key.key()]);

        // Cache the result
        let contains_curr = self.local_cache.read().await.contains_key(&curr.round());
        if sig_match && !contains_curr {
            self.local_cache
                .write()
                .await
                .insert(curr.round(), curr.clone());
        }
        Ok(sig_match)
    }

    async fn entry(&self, round: u64) -> Result<BeaconEntry, Box<dyn error::Error>> {
        let cached: Option<BeaconEntry> = self.local_cache.read().await.get(&round).cloned();
        match cached {
            Some(cached_entry) => Ok(cached_entry),
            None => {
                let url = format!("{}/public/{}", self.url, round);
                let resp: BeaconEntryJson = surf::get(&url).recv_json().await?;
                Ok(BeaconEntry::new(resp.round, hex::decode(resp.signature)?))
            }
        }
    }

    fn max_beacon_round_for_epoch(&self, fil_epoch: ChainEpoch) -> u64 {
        let latest_ts =
            ((fil_epoch as u64 * self.fil_round_time) + self.fil_gen_time) - self.fil_round_time;
        (latest_ts - self.drand_gen_time) / self.interval
    }
}
