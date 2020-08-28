// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use actor::paych::SignedVoucher;
use address::Address;
use async_std::sync::{Arc, RwLock};
use cid::Cid;
use derive_builder::Builder;
use encoding::Cbor;
use log::warn;
use num_bigint::{
    bigint_ser::{BigIntDe, BigIntSer},
    BigInt,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::str::FromStr;
use uuid::Uuid;

pub(crate) const DIR_INBOUND: u8 = 1;
pub(crate) const DIR_OUTBOUND: u8 = 2;
const DS_KEY_CHANNEL_INFO: &str = "ChannelInfo";
const DS_KEY_MSG_CID: &str = "MsgCid";

#[derive(Serialize, Deserialize, Clone)]
pub struct VoucherInfo {
    pub voucher: SignedVoucher,
    pub proof: Vec<u8>,
}

// TODO handle serializing channelinfo default cid by adding logic where if the cid is default then return
// TODO empty byte array

/// ChannelInfo keeps track of information about a channel
#[derive(Clone, Builder)]
#[builder(name = "ChannelInfoBuilder")]
pub struct ChannelInfo {
    /// id is a uuid that is created upon adding to the paychstore
    #[builder(default)]
    id: String,
    /// Channel address can only be None if the channel hasn't been created yet
    #[builder(default)]
    channel: Option<Address>,
    /// Address of the account that created the channel
    pub control: Address,
    /// Address of the account on the other side of the channel
    pub target: Address,
    /// Direction indicates if the channel is inbound (this node is the target)
    /// or outbound (this node is the control)
    ///
    direction: u8,
    /// The list of all vouchers sent on the channel
    #[builder(default)]
    pub vouchers: Vec<VoucherInfo>,
    /// Number of the next lane that should be used when the client requests a new lane
    /// (ie makes a new voucher for a new deal)
    pub next_lane: u64,
    /// Amount to be added to the channel
    /// This amount is only used by get_paych to keep track of how much
    /// has locally been added to the channel. It should reflect the channel's
    /// Balance on chain as long as all operations occur in the same datastore
    #[builder(default)]
    amount: BigInt,
    /// The amount that's awaiting confirmation
    #[builder(default)]
    pending_amount: BigInt,
    /// The CID of a pending create message while waiting for confirmation
    #[builder(default)]
    create_msg: Option<Cid>,
    /// The CID of a pending add funds message while waiting for confirmation
    #[builder(default)]
    add_funds_msg: Option<Cid>,
    /// indicates whether or not the channel has entered into the settling state
    #[builder(default)]
    pub settling: bool,
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
            id,
            channel,
            control,
            target,
            direction,
            vouchers,
            next_lane,
            amount,
            pending_amount,
            create_msg,
            add_funds_msg,
            settling,
        };

        Ok(ci)
    }
}

impl ChannelInfo {
    pub fn builder() -> ChannelInfoBuilder {
        ChannelInfoBuilder::default()
    }
}

#[derive(Clone)]
pub struct PaychStore {
    // use blockstore instead?
    ds: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl Cbor for ChannelInfo {}

impl PaychStore {
    /// Create new Pay Channel Store
    pub fn new(ds: HashMap<String, Vec<u8>>) -> Self {
        PaychStore {
            ds: Arc::new(RwLock::new(ds)),
        }
    }

    /// Add ChannelInfo to PaychStore
    pub async fn put_channel_info(&mut self, mut ci: ChannelInfo) -> Result<(), Error> {
        if ci.id.len() == 0 {
            ci.id = Uuid::new_v4().to_string();
        }
        let key = key_for_channel(ci.channel.ok_or_else(|| Error::NoAddress)?.to_string());
        let value = ci
            .marshal_cbor()
            .map_err(|err| Error::Other(err.to_string()))?;

        self.ds.write().await.insert(key, value);
        Ok(())
    }

    /// Get ChannelInfo for a given Channel Address
    pub async fn get_channel_info(&self, addr: &Address) -> Result<ChannelInfo, Error> {
        if let Some(k) = self
            .ds
            .read()
            .await
            .get(&format!("ChannelInfo/{}", addr.to_string()))
        {
            let ci = ChannelInfo::unmarshal_cbor(&k)?;
            Ok(ci)
        } else {
            Err(Error::ChannelNotTracked)
        }
    }

    /// Track a ChannelInfo
    pub async fn track_channel(&mut self, ch: ChannelInfo) -> Result<(), Error> {
        match self
            .get_channel_info(&ch.channel.ok_or_else(|| Error::NoAddress)?)
            .await
        {
            Err(Error::ChannelNotTracked) => self.put_channel_info(ch).await,
            Ok(_) => Err(Error::DupChannelTracking),
            Err(err) => Err(err),
        }
    }

    /// Return a Vec of all ChannelInfo Addresses in paych_store
    pub async fn list_channels(&self) -> Result<Vec<Address>, Error> {
        let ds = self.ds.read().await;
        let res = ds.keys();
        let mut out = Vec::new();
        for addr_str in res {
            if addr_str.starts_with("ChannelInfo/") {
                out.push(
                    Address::from_str(addr_str.trim_start_matches("ChannelInfo/"))
                        .map_err(|err| Error::Other(err.to_string()))?,
                )
            } else {
                warn!("invalid ChannelInfo Channel Address: {}", addr_str);
                continue;
            }
        }
        Ok(out)
    }

    /// Find a single channel using teh given filter, if no channel matches, return ChannelNotTrackedError
    pub async fn find_chan(
        &self,
        filter: Box<dyn Fn(&ChannelInfo) -> bool>,
    ) -> Result<ChannelInfo, Error> {
        let mut ci = self.find_chans(filter, 1).await?;

        if ci.is_empty() {
            return Err(Error::ChannelNotTracked);
        }
        // previous check ensures unwrap does not fail
        return Ok(ci.pop().unwrap());
    }

    /// Loop over all channels, return Vec of all channels that fit given filter, specify max to be the max length
    /// of returned Vec, set max to 0 for Vec of all channels that fit the given filter
    pub async fn find_chans(
        &self,
        filter: Box<dyn Fn(&ChannelInfo) -> bool>,
        max: usize,
    ) -> Result<Vec<ChannelInfo>, Error> {
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
        self.put_channel_info(ci).await?;
        Ok(out)
    }

    /// Return Vec of all voucher infos for given ChannelInfo Address
    pub async fn vouchers_for_paych(&self, ch: &Address) -> Result<Vec<VoucherInfo>, Error> {
        let ci = self.get_channel_info(ch).await?;
        Ok(ci.vouchers)
    }

    /// get the ChannelInfo that matches given Address
    pub async fn by_address(&self, addr: Address) -> Result<ChannelInfo, Error> {
        let ds = self.ds.read().await;
        for val in ds.values() {
            let ci = ChannelInfo::unmarshal_cbor(val)?;
            if ci.channel.ok_or_else(|| Error::NoAddress)? == addr {
                return Ok(ci);
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
                return Ok(ci);
            }
        }
        Err(Error::ChannelNotTracked)
    }

    /// this method is called when a new message is sent
    pub async fn save_new_message(&mut self, channel_id: String, mcid: Cid) -> Result<(), Error> {
        let mut ds = self.ds.write().await;
        let k = key_for_msg(&mcid);
        let mi: MsgInfo = MsgInfo {
            channel_id,
            msg_cid: mcid,
            received: false,
            err: "".to_string(),
        };
        let bytes = mi
            .marshal_cbor()
            .map_err(|err| Error::Other(err.to_string()))?;
        ds.insert(k, bytes);
        Ok(())
    }

    /// this method is called when teh result of a message is received
    /// TODO need to see if the message is already in the data store and if it is then do we replace kv pair with updated one
    pub async fn save_msg_result(
        &mut self,
        mcid: Cid,
        msg_err: Option<Error>,
    ) -> Result<(), Error> {
        let mut ds = self.ds.write().await;
        let k = key_for_msg(&mcid);
        let mut minfo = self.get_message(mcid).await?;
        if msg_err.is_some() {
            minfo.err = msg_err.unwrap().to_string();
        }
        let b = minfo
            .marshal_cbor()
            .map_err(|err| Error::Other(err.to_string()))?;
        ds.insert(k, b);
        Ok(())
    }

    /// Return first outbound channel that has not been settles with given to and from address
    pub async fn outbound_active_by_from_to(
        &self,
        from: Address,
        to: Address,
    ) -> Result<ChannelInfo, Error> {
        let ds = self.ds.read().await;

        for val in ds.values() {
            let ci = ChannelInfo::unmarshal_cbor(val)?;
            if ci.direction == DIR_OUTBOUND {
                continue;
            }
            if ci.settling {
                continue;
            }
            if (ci.control == from) & (ci.target == to) {
                return Ok(ci);
            }
        }
        Err(Error::ChannelNotTracked)
    }

    /// This function is used on start up to find channels where a create channel or add funds message
    /// has been sent, but node was shut down before response was received
    pub async fn with_pending_add_funds(&mut self) -> Result<Vec<ChannelInfo>, Error> {
        self.find_chans(
            Box::new(|ci| {
                if ci.direction != DIR_OUTBOUND {
                    return false;
                }
                if ci.add_funds_msg.is_none() {
                    return false;
                }
                // TODO  need to figure out smarter way to do this
                return (ci.create_msg.as_ref().unwrap().clone() != Cid::default())
                    | (ci.add_funds_msg.as_ref().unwrap().clone() != Cid::default());
            }),
            0,
        )
        .await
    }

    /// Get channel info given channel ID
    pub async fn by_change_id(&self, channel_id: String) -> Result<ChannelInfo, Error> {
        let ds = self.ds.read().await;
        let res = ds
            .get(&channel_id)
            .ok_or_else(|| Error::ChannelNotTracked)?;
        let ci = ChannelInfo::unmarshal_cbor(res)?;
        Ok(ci)
    }

    /// Create a new new outbound channel for given parameters
    pub async fn create_channel(
        &mut self,
        from: Address,
        to: Address,
        create_msg_cid: Cid,
        amt: BigInt,
    ) -> Result<ChannelInfo, Error> {
        let ci = ChannelInfo {
            id: "".to_string(),
            channel: None,
            vouchers: Vec::new(),
            direction: DIR_OUTBOUND,
            next_lane: 0,
            control: from,
            target: to,
            create_msg: Some(create_msg_cid.clone()),
            pending_amount: amt,
            amount: BigInt::default(),
            add_funds_msg: None,
            settling: false,
        };
        self.put_channel_info(ci.clone()).await?;
        self.save_new_message(ci.id.clone(), create_msg_cid).await?;
        Ok(ci)
    }

    /// Remove a channel with given channel ID
    pub async fn remove_channel(&mut self, channel_id: String) -> Result<(), Error> {
        let mut ds = self.ds.write().await;
        ds.remove(&format!("{}/{}", DS_KEY_CHANNEL_INFO, channel_id))
            .ok_or_else(|| Error::ChannelNotTracked)?;
        Ok(())
    }
}

fn key_for_channel(channel_id: String) -> String {
    return format!("{}/{}", DS_KEY_CHANNEL_INFO, channel_id);
}

fn key_for_msg(mcid: &Cid) -> String {
    return format!("{}/{}", DS_KEY_MSG_CID, mcid.to_string());
}

/// MsgInfo stores information about a create channel / add funds message that has been sent
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct MsgInfo {
    channel_id: String,
    msg_cid: Cid,
    received: bool,
    err: String,
}

impl Cbor for MsgInfo {}

#[cfg(test)]
mod tests {
    use super::*;
    use async_std::task;
    use crypto::SignatureType;

    #[test]
    fn test_store() {
        task::block_on(async {
            let mut store = PaychStore::new(HashMap::new());
            let addrs = store.list_channels().await.unwrap();
            assert_eq!(addrs.len(), 0);

            let chan1 = Address::new_id(100);
            let chan2 = Address::new_id(200);
            let to1 = Address::new_id(101);
            let to2 = Address::new_id(201);
            let from1 = Address::new_id(102);
            let from2 = Address::new_id(202);

            let mut ci1 = ChannelInfo {
                id: "".to_string(),
                channel: Some(chan1.clone()),
                vouchers: vec![VoucherInfo {
                    voucher: SignedVoucher::default(),
                    proof: Vec::new(),
                }],
                direction: DIR_OUTBOUND,
                next_lane: 0,
                control: from1.clone(),
                target: to1.clone(),
                create_msg: None,
                pending_amount: BigInt::default(),
                amount: BigInt::default(),
                add_funds_msg: None,
                settling: false,
            };

            let mut ci2 = ChannelInfo {
                id: "".to_string(),
                channel: Some(chan2.clone()),
                vouchers: vec![VoucherInfo {
                    voucher: SignedVoucher::default(),
                    proof: Vec::new(),
                }],
                direction: DIR_OUTBOUND,
                next_lane: 0,
                control: from2.clone(),
                target: to2.clone(),
                create_msg: None,
                pending_amount: BigInt::default(),
                amount: BigInt::default(),
                add_funds_msg: None,
                settling: false,
            };

            // Track channels
            assert!(store.track_channel( ci1.clone()).await.is_ok());
            assert!(store.track_channel(ci2).await.is_ok());

            // make sure that tracking a channel twice throws error
            assert!(store.track_channel(ci1).await.is_err());

            let addrs = store.list_channels().await.unwrap();
            // Make sure that number of channel addresses in paychstore is 2 and that the proper
            // addresses have been saved
            assert_eq!(addrs.len(), 2);
            assert!(addrs.contains(&chan1));
            assert!(addrs.contains(&chan2));

            // Test to make sure that attempted to get vouchers for non-existent channel will error
            assert!(store
                .vouchers_for_paych(&mut Address::new_id(300))
                .await
                .is_err());

            // Allocate lane for channel
            let lane = store.allocate_lane(chan1.clone()).await.unwrap();
            assert_eq!(lane, 0);

            // Allocate lane for next channel
            let lane2 = store.allocate_lane(chan1.clone()).await.unwrap();
            assert_eq!(lane2, 1);

            //  Make sure that allocating a lane for non-existent channel will error
            assert!(store.allocate_lane(Address::new_id(300)).await.is_err())
        });
    }
}
