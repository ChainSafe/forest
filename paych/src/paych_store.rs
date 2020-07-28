// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::sync::{Arc, RwLock};
use std::collections::HashMap;
use actor::paych::SignedVoucher;
use address::Address;
use encoding::Cbor;
use super::errors::Error;
use serde::{Serialize, Deserialize};
use std::str::FromStr;

#[derive(Serialize, Deserialize)]
pub struct VoucherInfo {
    voucher: SignedVoucher,
    proof: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
pub struct ChannelInfo {
    channel: Address,
    control: Address,
    target: Address,
    direction: u64,
    vouchers: Vec<VoucherInfo>,
    next_lane: u64,
}

pub struct PaychStore {
    // use blockstore instead?
    ds: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl Cbor for ChannelInfo {}

impl PaychStore {
    /// Create new Pay Channel Store
    pub fn new(ds: HashMap<String, Vec<u8>>) -> Self {
        PaychStore{ ds: Arc::new(RwLock::new(ds))}
    }

    /// Add ChannelInfo to PaychStore
    pub async fn put_channel_info(&mut self, ci: &ChannelInfo) -> Result<(), Error> {
        let key = ci.channel.to_string();
        let value = ci.marshal_cbor().map_err(|err| Error::Other(err.to_string()))?;

        self.ds.write().await.insert(key, value);
        Ok(())
    }

    /// Get ChannelInfo for a given Channel Address
    pub async fn get_channel_info(&self, addr: &Address) -> Result<ChannelInfo, Error> {
        if let Some(k) = self.ds.read().await.get(&addr.to_string()) {
            let ci = ChannelInfo::unmarshal_cbor(&k).map_err(|err| Error::Other(err.to_string()))?;
            Ok(ci)
        } else {
            Err(Error::ChannelNotTracked)
        }
    }

    /// Track a ChannelInfo
    pub async fn track_channel(&mut self, ch: ChannelInfo) -> Result<(), Error> {
        match self.get_channel_info(&ch.channel).await {
            Err(Error::ChannelNotTracked) => self.put_channel_info(&ch).await,
            Ok(_) => Err(Error::DupChannelTracking),
            Err(err) => Err(err)
        }
    }

    /// Return a Vec of all ChannelInfo Addresses in paych_store
    pub async fn list_channels(&mut self) -> Result<Vec<Address>, Error> {
        let ds = self.ds.read().await;
        let res = ds.keys();
        let mut out = Vec::new();
        for addr_str in res {
            out.push(Address::from_str(addr_str).map_err(|err| Error::Other(err.to_string()))?)
        }
        Ok(out)
    }

    /// Find Channel Address given a specified filter function
    pub async fn find_channel(&mut self, filter: fn(&ChannelInfo) -> bool) -> Result<Address, Error> {
        let ds = self.ds.read().await;

        for val in ds.values() {
            let ci = ChannelInfo::unmarshal_cbor(val).map_err(|err| Error::Other(err.to_string()))?;
            if filter(&ci) {
                return Ok(ci.channel)
            }
        }
        Err(Error::NoAddress)
    }

    /// Allocate a lane for a given ChannelInfo
    pub async fn allocate_lane(&mut self, ch: Address) -> Result<u64, Error> {
        let mut ci = self.get_channel_info(&ch).await?;
        let out = ci.next_lane;
        ci.next_lane += 1;
        self.put_channel_info(&ci).await?;
        Ok(out)
    }

    /// Return Vec of all voucher infos for given ChannelInfo Address
    pub async fn vouchers_for_paych(&mut self, ch: &Address) -> Result<Vec<VoucherInfo>, Error> {
        let ci = self.get_channel_info(ch).await?;
        Ok(ci.vouchers)
    }
}

