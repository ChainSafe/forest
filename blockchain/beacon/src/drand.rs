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
use std::convert::TryFrom;
use std::error;

/// Default endpoint for the drand beacon node.
pub const DEFAULT_DRAND_URL: &str = "https://api.drand.sh";

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

#[derive(SerdeDeserialize, SerdeSerialize, Debug, Clone)]
pub struct ChainInfo {
    public_key: String,
    period: i32,
    genesis_time: i32,
    hash: String,
    #[serde(rename = "groupHash")]
    group_hash: String,
}

#[derive(SerdeDeserialize, SerdeSerialize, Debug, Clone)]
pub struct BeaconEntryJson {
    round: u64,
    randomness: String,
    signature: String,
    previous_signature: String,
}

pub struct DrandBeacon {
    url: Cow<'static, str>,

    pub_key: DrandPublic,
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
        url: impl Into<Cow<'static, str>>,
        pub_key: DrandPublic,
        genesis_ts: u64,
        interval: u64,
    ) -> Result<Self, Box<dyn error::Error>> {
        if genesis_ts == 0 {
            panic!("Genesis timestamp cannot be 0")
        }
        let url = url.into();
        let chain_info: ChainInfo = surf::get(&format!("{}/info", &url)).recv_json().await?;
        let remote_pub_key = hex::decode(chain_info.public_key)?;
        if remote_pub_key != pub_key.coefficient {
            return Err(Box::try_from(
                "Drand pub key from config is different than one on drand servers",
            )?);
        }

        Ok(Self {
            url,
            pub_key,
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
        if sig_match && !self.local_cache.read().await.contains_key(&curr.round()) {
            self.local_cache
                .write()
                .await
                .insert(curr.round(), curr.clone());
        }
        Ok(sig_match)
    }

    async fn entry(&self, round: u64) -> Result<BeaconEntry, Box<dyn error::Error>> {
        match self.local_cache.read().await.get(&round) {
            Some(cached_entry) => Ok(cached_entry.clone()),
            None => {
                let url = format!("{}/public/{}", self.url, round);
                let resp: BeaconEntryJson = surf::get(&url).recv_json().await?;
                Ok(BeaconEntry::new(resp.round, hex::decode(resp.signature)?))
            }
        }
    }

    fn max_beacon_round_for_epoch(&self, fil_epoch: ChainEpoch) -> u64 {
        let latest_ts =
            fil_epoch as u64 * self.fil_round_time + self.fil_gen_time - self.fil_round_time;
        // TODO: self.interval has to be converted to seconds. Dont know what it is right now
        (latest_ts - self.drand_gen_time) / self.interval
    }
}
