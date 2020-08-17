// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::sync::{Arc, RwLock};
use std::collections::HashMap;
use actor::paych::SignedVoucher;
use address::Address;
use encoding::Cbor;
use super::errors::Error;
use serde::{Serialize, Deserialize, Serializer, Deserializer};
use std::str::FromStr;
use cid::Cid;
use num_bigint::{BigInt, bigint_ser::{BigIntDe, BigIntSer}};
use uuid::Uuid;

const DIR_INBOUND: u8 = 1;
const DIR_OUTBOUND: u8 = 2;
const DS_KEY_CHANNEL_INFO: &str = "ChannelInfo";
const DS_KEY_MSG_CID: &str = "MsgCid";

#[derive(Serialize, Deserialize, Clone)]
pub struct VoucherInfo {
    voucher: SignedVoucher,
    proof: Vec<u8>,
}

// TODO handle serializing channelinfo default cid by adding logic where if the cid is default then return
// TODO empty byte array

/// ChannelInfo keeps track of information about a channel
#[derive(Clone)]
pub struct ChannelInfo {
    id: String,
    channel: Option<Address>,
    control: Address,
    target: Address,
    direction: u8,
    vouchers: Vec<VoucherInfo>,
    next_lane: u64,
    // change to bigint
    amount: BigInt,
    pending_amount: BigInt,
    create_msg: Cid,
    add_funds_msg: Option<Cid>,
    settling: bool,
}

impl Serialize for ChannelInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
    {
        (
            &self.id,
            &self.channel,
            &self.control,
            &self.target,
            &self.direction,
            &self.vouchers,
            &self.next_lane,
            BigIntSer(&self.amount),
            BigIntSer(&self.pending_amount),
            &self.create_msg,
            &self.add_funds_msg,
            &self.settling,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ChannelInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
        where
            D: Deserializer<'de>,
    {
        let (
        id,
        channel,
        control,
        target,
        direction,
        vouchers,
        next_lane,
        BigIntDe(amount),
        BigIntDe(pending_amount),
        create_msg,
        add_funds_msg,
        settling,
        ) = Deserialize::deserialize(deserializer)?;

        let ci = ChannelInfo {
            id, channel, control, target, direction, vouchers, next_lane, amount, pending_amount, create_msg, add_funds_msg, settling
        };

        Ok(ci)
    }
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
    pub async fn put_channel_info(&mut self, ci: &mut ChannelInfo) -> Result<(), Error> {
        if ci.id.len() == 0 {
            ci.id = Uuid::new_v4().to_string();
        }
        let key = key_for_channel(ci.channel.ok_or_else(|| Error::NoAddress)?.to_string());
        let value = ci.marshal_cbor().map_err(|err| Error::Other(err.to_string()))?;

        self.ds.write().await.insert(key, value);
        Ok(())
    }

    /// Get ChannelInfo for a given Channel Address
    pub async fn get_channel_info(&self, addr: &Address) -> Result<ChannelInfo, Error> {
        if let Some(k) = self.ds.read().await.get(&addr.to_string()) {
            let ci = ChannelInfo::unmarshal_cbor(&k)?;
            Ok(ci)
        } else {
            Err(Error::ChannelNotTracked)
        }
    }

    /// Track a ChannelInfo
    pub async fn track_channel(&mut self, ch: &mut ChannelInfo) -> Result<(), Error> {
        match self.get_channel_info(&ch.channel.ok_or_else(|| Error::NoAddress)?).await {
            Err(Error::ChannelNotTracked) => self.put_channel_info(ch).await,
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

    /// Find a single channel using teh given filter, if no channel matches, return ChannelNotTrackedError
    pub async fn find_chan(&self, filter: Box<dyn Fn(&ChannelInfo) -> bool>) -> Result<ChannelInfo, Error> {
        let mut ci = self.find_chans(filter, 1).await?;

        if ci.is_empty() {
            return Err(Error::ChannelNotTracked)
        }
        // previous check ensures unwrap does not fail
        return Ok(ci.pop().unwrap())
    }

    /// Loop over all channels, return Vec of all channels that fit given filter, specify max to be the max length
    /// of returned Vec, set max to 0 for Vec of all channels that fit the given filter
    pub async fn find_chans(&self, filter: Box<dyn Fn(&ChannelInfo) -> bool>, max: usize) -> Result<Vec<ChannelInfo>, Error> {
        let ds = self.ds.read().await;
        let mut matches = Vec::new();

        for val in ds.values() {
            let ci = ChannelInfo::unmarshal_cbor(val)?;
            if filter(&ci) {
                matches.push(ci);
                if matches.len() == max {
                    return Ok(matches);
                }
            }
        }
        Ok(matches)
    }

    /// Allocate a lane for a given ChannelInfo
    pub async fn allocate_lane(&mut self, ch: Address) -> Result<u64, Error> {
        let mut ci = self.get_channel_info(&ch).await?;
        let out = ci.next_lane;
        ci.next_lane += 1;
        self.put_channel_info(&mut ci).await?;
        Ok(out)
    }

    /// Return Vec of all voucher infos for given ChannelInfo Address
    pub async fn vouchers_for_paych(&mut self, ch: &Address) -> Result<Vec<VoucherInfo>, Error> {
        let ci = self.get_channel_info(ch).await?;
        Ok(ci.vouchers)
    }

    /// get the ChannelInfo that matches given Address
    pub async fn by_address(&self, addr: Address) -> Result<ChannelInfo, Error> {
        let ds = self.ds.read().await;
        for val in ds.values() {
            let ci = ChannelInfo::unmarshal_cbor(val)?;
            if ci.channel.ok_or_else(|| Error::NoAddress)? == addr {
                return Ok(ci)
            }
        }
        Err(Error::ChannelNotTracked)
    }

    /// Get the message info for a given message CID
    pub async fn get_message(&self, mcid: Cid) -> Result<MsgInfo, Error> {
        let ds = self.ds.read().await;
        let k = key_for_msg(&mcid);
        let val = ds.get(&k).ok_or_else(|| Error::NoVal)?;
        let minfo = MsgInfo::unmarshal_cbor(val.as_slice())?;
        Ok(minfo)
    }

    /// get the vannel associated with a message
    pub async fn by_message_cid(&self, mcid: Cid) -> Result<ChannelInfo, Error> {
        let ds = self.ds.read().await;
        let minfo = self.get_message(mcid).await?;
        for val in ds.values() {
            let ci = ChannelInfo::unmarshal_cbor(val)?;
            if ci.id == minfo.channel_id {
                return Ok(ci)
            }
        }
        Err(Error::ChannelNotTracked)
    }

    /// this method is called when a new message is sent
    pub async fn save_new_message(&mut self, channel_id: String, mcid: Cid) -> Result<(), Error> {
        let mut ds = self.ds.write().await;
        let k = key_for_msg(&mcid);
        let mi: MsgInfo = MsgInfo {channel_id, msg_cid: mcid, received: false, err: "".to_string()};
        let bytes = mi.marshal_cbor().map_err(|err| Error::Other(err.to_string()))?;
        ds.insert(k, bytes);
        Ok(())
    }

    /// this method is called when teh result of a message is received
    /// TODO need to see if the message is already in the data store and if it is then do we replace kv pair with updated one
    pub async fn save_msg_result(&mut self, mcid: Cid, msg_err: Option<Error>) -> Result<(), Error> {
        let mut ds = self.ds.write().await;
        let k = key_for_msg(&mcid);
        let mut minfo = self.get_message(mcid).await?;
        if msg_err.is_some() {
            minfo.err = msg_err.unwrap().to_string();
        }
        let b = minfo.marshal_cbor().map_err(|err| Error::Other(err.to_string()))?;
        ds.insert(k, b);
        Ok(())
    }

    /// Return first outbound channel that has not been settles with given to and from address
    pub async fn outbound_active_by_from_to(&self, from: Address, to: Address) -> Result<ChannelInfo, Error> {
        let ds = self.ds.read().await;

        for val in ds.values() {
            let ci = ChannelInfo::unmarshal_cbor(val)?;
            if ci.direction == DIR_OUTBOUND {
                continue
            }
            if ci.settling {
                continue
            }
            if (ci.control == from) & (ci.target == to) {
                return Ok(ci)
            }
        }
        Err(Error::ChannelNotTracked)
    }

    /// This function is used on start up to find channels where a create channel or add funds message
    /// has been sent, but node was shut down before response was received
    pub async fn with_pending_add_funds(&mut self) -> Result<Vec<ChannelInfo>, Error> {
        self.find_chans(Box::new(|ci| {
            if ci.direction != DIR_OUTBOUND {
                return false;
            }
            if ci.add_funds_msg.is_none() {
                return false;
            }
            // need to figure out smarter way to do this
            return (ci.create_msg != Cid::default()) | (ci.add_funds_msg.as_ref().unwrap().clone() != Cid::default())
        }), 0).await
    }

    /// Get channel info given channel ID
    pub async fn by_change_id(&self, channel_id: String) -> Result<ChannelInfo, Error> {
        let ds = self.ds.read().await;
        let res = ds.get(&channel_id).ok_or_else(|| Error::ChannelNotTracked)?;
        let ci = ChannelInfo::unmarshal_cbor(res)?;
        Ok(ci)
    }

    /// Create a new new outbound channel for given parameters
    pub async fn create_channel(&mut self, from: Address, to: Address, create_msg_cid: Cid, amt: BigInt) -> Result<ChannelInfo, Error> {
        let mut ci = ChannelInfo{
            id: "".to_string(),
            channel: None,
            vouchers: Vec::new(),
            direction: DIR_OUTBOUND,
            next_lane: 0,
            control: from,
            target: to,
            create_msg: create_msg_cid.clone(),
            pending_amount: amt,
            amount: BigInt::default(),
            add_funds_msg: None,
            settling: false
        };
        self.put_channel_info(&mut ci).await?;
        self.save_new_message(ci.id.clone(), create_msg_cid).await?;
        Ok(ci)
    }

    /// Remove a channel with given channel ID
    pub async fn remove_channel(&mut self, channel_id: String) -> Result<(), Error> {
        let mut ds = self.ds.write().await;
        ds.remove(&format!("{}/{}", DS_KEY_CHANNEL_INFO, channel_id)).ok_or_else(|| Error::ChannelNotTracked)?;
        Ok(())
    }
}

fn key_for_channel(channel_id: String) -> String {
    return format!("{}/{}", DS_KEY_CHANNEL_INFO, channel_id)
}

fn key_for_msg(mcid: &Cid) -> String {
    return format!("{}/{}", DS_KEY_MSG_CID, mcid.to_string())
}

/// MsgInfo stores information about a create channel / add funds message that has been sent
#[derive(Serialize, Deserialize)]
pub struct MsgInfo {
    channel_id: String,
    msg_cid: Cid,
    received: bool,
    err: String
}

impl Cbor for MsgInfo {}

#[cfg(test)]
mod tests {
    use super::*;
    use async_std::task;

    #[test]
    fn test_store() {
        task::block_on(async {
            let mut store = PaychStore::new(HashMap::new());
            let addrs = store.list_channels().await.unwrap();
            assert_eq!(addrs.len(), 0);

            let addr = Address::new_bls()

        });
        v
    }
}