// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{ChannelInfo, Error, PaychStore, StateAccessor};
use crate::{ChannelAccessor, PaychFundsRes, VoucherInfo, DIR_INBOUND};
use actor::paych::SignedVoucher;
use address::Address;
use async_std::sync::{Arc, RwLock};
use blockstore::BlockStore;
use cid::Cid;
use jsonrpsee::raw::RawClient;
use jsonrpsee::transport::http::HttpTransportClient;
use num_bigint::BigInt;
use rpc_client::new_client;
use std::collections::HashMap;

pub struct Manager<DB> {
    // TODO need to add managerAPI (consists of some state and paych API calls)
    pub store: Arc<RwLock<PaychStore>>,
    pub sa: Arc<StateAccessor<DB>>,
    pub channels: Arc<RwLock<HashMap<String, Arc<ChannelAccessor<DB>>>>>,
    pub client: RawClient<HttpTransportClient>,
}

impl<DB> Manager<DB>
where
    DB: BlockStore,
{
    pub fn new(sa: StateAccessor<DB>, store: PaychStore) -> Self {
        Manager {
            store: Arc::new(RwLock::new(store)),
            sa: Arc::new(sa),
            channels: Arc::new(RwLock::new(HashMap::new())),
            client: new_client(),
        }
    }

    pub async fn track_inbound_channel(&mut self, ch: Address) -> Result<ChannelInfo, Error> {
        let mut store = self.store.write().await;

        // Check if channel is in store
        let ci = store.by_address(ch).await;
        match ci {
            Ok(_) => return ci,
            Err(err) => {
                if err != Error::ChannelNotTracked {
                    return Err(err);
                }
            }
        }
        let state_ci = self.sa.load_state_channel_info(ch, DIR_INBOUND).await?;
        // TODO add ability to get channel from state
        // TODO need to check if channel to address is in wallet
        store.track_channel(state_ci).await
    }

    pub async fn accessor_by_from_to(
        &self,
        from: Address,
        to: Address,
    ) -> Result<Arc<ChannelAccessor<DB>>, Error> {
        let channels = self.channels.read().await;
        let key = accessor_cache_key(&from, &to);

        // check if channel accessor is in cache without taking write lock
        let op = channels.get(&key);
        if let Some(channel) = op {
            return Ok(channel.clone());
        }
        drop(channels);

        // channel accessor is not in cache so take a write lock, and create new entry in cache
        let mut channel_write = self.channels.write().await;
        let ca = ChannelAccessor::new(&self);
        channel_write
            .insert(key.clone(), Arc::new(ca))
            .ok_or_else(|| Error::Other("insert new channel accesor".to_string()))?;
        let channel_check = self.channels.read().await;
        let op_locked = channel_check.get(&key);
        if let Some(channel) = op_locked {
            return Ok(channel.clone());
        }
        Err(Error::Other("could not find channel accessor".to_owned()))
    }

    // Add a channel accessor to the cache. Note that the
    // channel may not have been created yet, but we still want to reference
    // the same channel accessor for a given from/to, so that all attempts to
    // access a channel use the same lock (the lock on the accessor)
    pub async fn add_accessor_to_cache(
        &self,
        from: Address,
        to: Address,
    ) -> Result<Arc<ChannelAccessor<DB>>, Error> {
        let key = accessor_cache_key(&from, &to);
        let ca = ChannelAccessor::new(&self);
        let mut channels = self.channels.write().await;
        channels
            .insert(key, Arc::new(ca))
            .ok_or_else(|| Error::Other("inserting new channel accessor".to_string()))
    }

    pub async fn accessor_by_address(
        &self,
        ch: Address,
    ) -> Result<Arc<ChannelAccessor<DB>>, Error> {
        let store = self.store.read().await;
        let ci = store.by_address(ch).await?;
        self.accessor_by_from_to(ci.control, ci.target).await
    }

    pub async fn get_paych(
        &self,
        from: Address,
        to: Address,
        amt: BigInt,
    ) -> Result<PaychFundsRes, Error> {
        let chan_accesor = self.accessor_by_from_to(from, to).await?;
        Ok(chan_accesor.get_paych(from, to, amt).await?)
    }

    // GetPaychWaitReady waits until the create channel / add funds message with the
    // given message CID arrives.
    // The returned channel address can safely be used against the Manager methods.
    pub async fn get_paych_wait_ready(&self, _mcid: Cid) -> Result<Address, Error> {
        unimplemented!()
    }

    pub async fn list_channels(&self) -> Result<Vec<Address>, Error> {
        let store = self.store.read().await;
        store.list_channels().await
    }

    pub async fn get_channel_info(&self, addr: Address) -> Result<ChannelInfo, Error> {
        let ca = self.accessor_by_address(addr).await?;
        ca.get_channel_info(&addr).await
    }

    // Check if the given voucher is valid (is or could become spendable at some point).
    // If the channel is not in the store, fetches the channel from state (and checks that
    // the channel To address is owned by the wallet).
    pub async fn check_voucher_valid(
        &mut self,
        ch: Address,
        sv: SignedVoucher,
    ) -> Result<(), Error> {
        let ca = self.inbound_channel_accessor(ch).await?;
        let _ = ca.check_voucher_valid(ch, sv).await?;
        Ok(())
    }

    // Get an accessor for the given channel. The channel
    // must either exist in the store, or be an inbound channel that can be created
    // from state.
    pub async fn inbound_channel_accessor(
        &mut self,
        ch: Address,
    ) -> Result<Arc<ChannelAccessor<DB>>, Error> {
        // Make sure channel is in store, or can be fetched from state, and that
        // the channel To address is owned by the wallet
        let ci = self.track_inbound_channel(ch).await?;

        let from = ci.target;
        let to = ci.control;

        self.accessor_by_from_to(from, to).await
    }

    pub async fn add_voucher_outbound(
        &self,
        ch: Address,
        sv: SignedVoucher,
        proof: Vec<u8>,
        min_delta: BigInt,
    ) -> Result<BigInt, Error> {
        let ca = self.accessor_by_address(ch).await?;
        ca.add_voucher(ch, sv, proof, min_delta).await
    }

    pub async fn add_voucher_inbound(
        &mut self,
        ch: Address,
        sv: SignedVoucher,
        proof: Vec<u8>,
        min_delta: BigInt,
    ) -> Result<BigInt, Error> {
        let ca = self.inbound_channel_accessor(ch).await?;
        ca.add_voucher(ch, sv, proof, min_delta).await
    }

    pub async fn allocate_lane(&self, ch: Address) -> Result<u64, Error> {
        let ca = self.accessor_by_address(ch).await?;
        ca.allocate_lane(ch).await
    }

    pub async fn list_vouchers(&self, ch: Address) -> Result<Vec<VoucherInfo>, Error> {
        let ca = self.accessor_by_address(ch).await?;
        ca.list_vouchers(ch).await
    }

    pub async fn next_sequence_for_lane(&self, ch: Address, lane: u64) -> Result<u64, Error> {
        let ca = self.accessor_by_address(ch).await?;
        ca.next_sequence_for_lane(ch, lane).await
    }

    pub async fn settle(&self, addr: Address) -> Result<Cid, Error> {
        let ca = self.accessor_by_address(addr).await?;
        ca.settle(addr).await
    }

    pub async fn collect(&self, addr: Address) -> Result<Cid, Error> {
        let ca = self.accessor_by_address(addr).await?;
        ca.collect(addr).await
    }
}

fn accessor_cache_key(from: &Address, to: &Address) -> String {
    from.to_string() + "->" + &to.to_string()
}
