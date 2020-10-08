// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{ChannelInfo, Error, PaychStore, StateAccessor};
use crate::{ChannelAccessor, PaychFundsRes, VoucherInfo, DIR_INBOUND};
use actor::paych::SignedVoucher;
use address::Address;
use async_std::sync::{Arc, RwLock};
use async_std::task;
use blockstore::BlockStore;
use cid::Cid;
use message_pool::{MessagePool, MpoolRpcProvider};
use num_bigint::BigInt;
use std::collections::HashMap;
use wallet::KeyStore;

/// Thread safe payment channel management
pub struct Manager<DB, KS>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    pub store: Arc<RwLock<PaychStore>>,
    #[allow(clippy::type_complexity)]
    pub channels: Arc<RwLock<HashMap<String, Arc<ChannelAccessor<DB, KS>>>>>,
    pub state: Arc<ResourceAccessor<DB, KS>>,
}
/// Thread safe access to message pool and keystore resource for paychannel usage
pub struct ResourceAccessor<DB, KS>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    pub keystore: Arc<RwLock<KS>>,
    pub mpool: Arc<MessagePool<MpoolRpcProvider<DB>>>,
    pub sa: Arc<StateAccessor<DB>>,
}

struct ChannelAvailableFunds {
    // Channel is the address of the channel
    pub channel: Option<Address>,
    // From is the from address of the channel (channel creator)
    pub from: Address,
    // To is the to address of the channel
    pub to: Address,
    // ConfirmedAmt is the amount of funds that have been confirmed on-chain
    // for the channel
    pub confirmed_amt: BigInt,
    // PendingAmt is the amount of funds that are pending confirmation on-chain
    pub pending_amt: BigInt,
    // PendingWaitSentinel can be used with PaychGetWaitReady to wait for
    // confirmation of pending funds
    pub pending_wait_sentinel: Option<Cid>,
    // QueuedAmt is the amount that is queued up behind a pending request
    pub queued_amt: BigInt,
    // VoucherRedeemedAmt is the amount that is redeemed by vouchers on-chain
    // and in the local datastore
    pub voucher_redeemed_amt: BigInt,
}

impl<DB, KS> Manager<DB, KS>
where
    DB: BlockStore + Send + Sync,
    KS: KeyStore + Send + Sync + 'static,
{
    pub fn new(store: PaychStore, state: ResourceAccessor<DB, KS>) -> Self {
        Manager {
            store: Arc::new(RwLock::new(store)),
            state: Arc::new(state),
            channels: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    /// Start restarts tracking of any messages that were sent to chain.
    pub async fn start(&self) -> Result<(), Error> {
        self.restart_pending().await
    }

    async fn restart_pending(&mut self) -> Result<(), Error> {
        let mut st = self.store.write().await;
        let cis = st.with_pending_add_funds().await?;
        // TODO ask about the group err usage
        for ci in cis {
            if let Some(msg) = ci.create_msg {
                let ca = self.accessor_by_from_to(ci.control, ci.target).await?;
                // TODO ask if this should be blocking
                task::spawn(async move || {
                    ca.wait_paych_create_msg(ci.id, msg).await?;
                });
                return Ok(());
            } else if let Some(msg) = ci.add_funds_msg {
                let ch = ci
                    .channel
                    .ok_or_else(|| Error::Other("error retrieving channel".to_string()))?;
                let ca = self.accessor_by_address(ch).await?;

                // TODO ask if this should be blocking
                task::spawn(async move || {
                    ca.wait_add_funds_msg(ci.id, msg).await?;
                });
                return Ok(());
            }
        }
        Ok(())
    }

    pub async fn available_funds(&self, ch: Address) -> Result<(), Error> {
        let ca = self.accessor_by_address(ch).await?;

        let ci = ca.get_channel_info(ch).await?;

        ca.available_funds(ci.id)
    }

    pub async fn available_funds_by_from_to(
        &self,
        from: Address,
        to: Address,
    ) -> Result<(), Error> {
        let mut st = self.store.read().await;
        let ca = self.accessor_by_from_to(from, to).await?;

        if let Err(e) = st.outbound_active_by_from_to(from, to).await {
            if e == Error::ChannelNotTracked {
                // If there is no active channel between from / to we still want to
                // return an empty ChannelAvailableFunds, so that clients can check
                // for the existence of a channel between from / to without getting
                // an error.
            }
        }
    }

    /// Ensures that a channel exists between the from and to addresses,
    /// and adds the given amount of funds.
    pub async fn get_paych(
        &self,
        from: Address,
        to: Address,
        amt: BigInt,
    ) -> Result<PaychFundsRes, Error> {
        let chan_accesor = self.accessor_by_from_to(from, to).await?;
        Ok(chan_accesor.get_paych(from, to, amt).await?)
    }

    // TODO !!!
    async fn track_inbound_channel(&mut self, ch: Address) -> Result<ChannelInfo, Error> {
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
        let state_ci = self
            .state
            .sa
            .load_state_channel_info(ch, DIR_INBOUND)
            .await?;

        // TODO add ability to get channel from state
        // TODO need to check if channel to address is in wallet
        store.track_channel(state_ci).await
    }

    async fn accessor_by_from_to(
        &self,
        from: Address,
        to: Address,
    ) -> Result<Arc<ChannelAccessor<DB, KS>>, Error> {
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
            .ok_or_else(|| Error::Other("insert new channel accessor".to_string()))?;
        let channel_check = self.channels.read().await;
        let op_locked = channel_check.get(&key);
        if let Some(channel) = op_locked {
            return Ok(channel.clone());
        }
        Err(Error::Other("could not find channel accessor".to_owned()))
    }

    /// Add a channel accessor to the cache. Note that the
    /// channel may not have been created yet, but we still want to reference
    /// the same channel accessor for a given from/to, so that all attempts to
    /// access a channel use the same lock (the lock on the accessor)
    async fn _add_accessor_to_cache(
        &self,
        from: Address,
        to: Address,
    ) -> Result<Arc<ChannelAccessor<DB, KS>>, Error> {
        let key = accessor_cache_key(&from, &to);
        let ca = ChannelAccessor::new(&self);
        let mut channels = self.channels.write().await;
        channels
            .insert(key, Arc::new(ca))
            .ok_or_else(|| Error::Other("inserting new channel accessor".to_string()))
    }

    async fn accessor_by_address(
        &self,
        ch: Address,
    ) -> Result<Arc<ChannelAccessor<DB, KS>>, Error> {
        let store = self.store.read().await;
        let ci = store.by_address(ch).await?;
        self.accessor_by_from_to(ci.control, ci.target).await
    }

    // Waits until the create channel / add funds message with the
    // given message CID arrives.
    // The returned channel address can safely be used against the Manager methods.
    pub async fn get_paych_wait_ready(&self, mcid: Cid) -> Result<Address, Error> {
        // First check if the message has completed
        let st = self.store.read().await;
        let msg_info = st.get_message(mcid).await?;

        // if the create channel / add funds message failed, return an Error
        if msg_info.err.len() > 0 {
            // do something
        }

        // if the message has completed successfully
        if msg_info.received {
            // get the channel address

            let ci = self.store.by_message_cid(mcid).await?;

            if ci.channel.is_none() {
                // panic err
            }
            Ok(ci.channel.unwrap())
        }
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

    /// Check if the given voucher is valid (is or could become spendable at some point).
    /// If the channel is not in the store, fetches the channel from state (and checks that
    /// the channel To address is owned by the wallet).
    pub async fn check_voucher_valid(
        &mut self,
        ch: Address,
        sv: SignedVoucher,
    ) -> Result<(), Error> {
        let ca = self.inbound_channel_accessor(ch).await?;
        let _ = ca.check_voucher_valid(ch, sv).await?;
        Ok(())
    }

    /// Get an accessor for the given channel. The channel
    /// must either exist in the store, or be an inbound channel that can be created
    /// from state.
    pub async fn inbound_channel_accessor(
        &mut self,
        ch: Address,
    ) -> Result<Arc<ChannelAccessor<DB, KS>>, Error> {
        // Make sure channel is in store, or can be fetched from state, and that
        // the channel To address is owned by the wallet
        let ci = self.track_inbound_channel(ch).await?;

        let from = ci.target;
        let to = ci.control;

        self.accessor_by_from_to(from, to).await
    }
    /// Adds a voucher for an outbound channel.
    /// Returns an error if the channel is not already in the store.
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
    /// Adds a voucher for an inbound channel.
    /// If the channel is not in the store, fetches the channel from state (and checks that
    /// the channel To address is owned by the wallet).
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
