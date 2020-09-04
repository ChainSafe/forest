// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Error;
use crate::{ChannelInfo, Manager, MsgListeners, PaychStore, StateAccessor, VoucherInfo};
use actor::account::State as AccountState;
use actor::init::ExecParams;
use actor::paych::{
    ConstructorParams, LaneState, SignedVoucher, State as PaychState, UpdateChannelStateParams,
};
use address::Address;
use async_std::sync::{Arc, RwLock};
use blockstore::BlockStore;
use chain::get_heaviest_tipset;
use cid::Cid;
use flo_stream::{MessagePublisher, Publisher, Subscriber};
use message::UnsignedMessage;
use num_bigint::BigInt;
use std::collections::HashMap;
use std::ops::{Add, Sub};
extern crate log;
use actor::Serialized;
use futures::StreamExt;

// TODO need to add paychapi (ability to access chain, mpool and wallet stuff)
pub struct ChannelAccessor<DB> {
    store: Arc<RwLock<PaychStore>>,
    msg_listeners: MsgListeners,
    sa: Arc<StateAccessor<DB>>,
    funds_req_queue: Arc<RwLock<Vec<FundsReq>>>,
}

impl<DB> ChannelAccessor<DB>
where
    DB: BlockStore,
{
    pub fn new(pm: &Manager<DB>) -> Self {
        ChannelAccessor {
            store: pm.store.clone(),
            msg_listeners: MsgListeners::new(),
            sa: pm.sa.clone(),
            funds_req_queue: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn get_channel_info(&self, addr: &Address) -> Result<ChannelInfo, Error> {
        self.store.read().await.get_channel_info(addr).await
    }

    pub async fn check_voucher_valid(
        &self,
        ch: Address,
        sv: SignedVoucher,
    ) -> Result<HashMap<u64, LaneState>, Error> {
        let sm = self.sa.sm.read().await;
        if sv.channel_addr != ch {
            return Err(Error::Other(
                "voucher channel address dpesm't match channel address".to_string(),
            ));
        }

        let (act, pch_state) = self.sa.load_paych_state(&ch).await?;
        let heaviest_ts = get_heaviest_tipset(sm.get_block_store().as_ref())
            .map_err(|_| Error::HeaviestTipset)?
            .ok_or_else(|| Error::HeaviestTipset)?;
        let cid = heaviest_ts.parent_state();
        let act_state: AccountState = sm
            .load_actor_state(&pch_state.from, cid)
            .map_err(|err| Error::Other(err.to_string()))?;
        let from = act_state.address;

        let vb = sv
            .signing_bytes()
            .map_err(|err| Error::Other(err.to_string()))?;

        let sig = sv.signature.clone();
        sig.ok_or_else(|| Error::Other("no sig".to_owned()))?
            .verify(&vb, &from)
            .map_err(|err| Error::Other(err))?;

        let lane_states = self.lane_state(&pch_state, ch).await?;
        let ls = lane_states
            .get(&sv.lane)
            .ok_or_else(|| Error::Other("No lane state for given nonce".to_owned()))?;
        if ls.nonce >= sv.nonce {
            return Err(Error::Other("nonce too low".to_owned()));
        }
        if ls.redeemed >= sv.amount {
            return Err(Error::Other("Voucher amount is lower than amount for voucher amount for voucher with lower nonce".to_owned()));
        }

        // Total redeemed is the total redeemed amount for all lanes, including
        // the new voucher
        // eg
        //
        // lane 1 redeemed:            3
        // lane 2 redeemed:            2
        // voucher for lane 1:         5
        //
        // Voucher supersedes lane 1 redeemed, therefore
        // effective lane 1 redeemed:  5
        //
        // lane 1:  5
        // lane 2:  2
        //          -
        // total:   7
        let merge_len = sv.merges.len();
        let total_redeemed = self.total_redeemed_with_voucher(&lane_states, sv).await?;

        // Total required balance = total redeemed + to send
        // must not exceed actor balance
        let new_total = total_redeemed + BigInt::from(pch_state.to_send);
        if BigInt::from(act.balance) < new_total {
            return Err(Error::Other(
                "Not enough funds in channel to cover voucher".to_owned(),
            ));
        }

        if merge_len != 0 {
            return Err(Error::Other(
                "don't currently support paych lane merges".to_owned(),
            ));
        }

        return Ok(lane_states);
    }

    pub async fn check_voucher_spendable(
        &self,
        ch: Address,
        sv: SignedVoucher,
        secret: Vec<u8>,
        mut proof: Vec<u8>,
    ) -> Result<bool, Error> {
        let _recipient = self.get_paych_recipient(&ch).await?;
        if (sv.extra != None) & (proof.len() != 0) {
            let store = self.store.read().await;
            let known = store.vouchers_for_paych(&ch).await?;
            for vi in known {
                if (proof == vi.proof) & (sv == vi.voucher) {
                    info!("using stored proof");
                    proof = vi.proof;
                    break;
                }
                if proof.len() == 0 {
                    log::warn!("empty proof for voucher with validation")
                }
            }
        }

        let _enc: UpdateChannelStateParams = UpdateChannelStateParams { sv, secret, proof };
        // TODO figure out what remaining lotus code means and how it would translate here
        unimplemented!();
    }

    pub async fn get_paych_recipient(&self, ch: &Address) -> Result<Address, Error> {
        let sm = self.sa.sm.read().await;
        let heaviest_ts = get_heaviest_tipset(sm.get_block_store().as_ref())
            .map_err(|_| Error::HeaviestTipset)?
            .ok_or_else(|| Error::HeaviestTipset)?;
        let cid = heaviest_ts.parent_state();
        let state: PaychState = sm
            .load_actor_state(ch, cid)
            .map_err(|err| Error::Other(err.to_string()))?;
        Ok(state.to)
    }

    pub async fn add_voucher(
        &self,
        ch: Address,
        sv: SignedVoucher,
        proof: Vec<u8>,
        min_delta: BigInt,
    ) -> Result<BigInt, Error> {
        let mut store = self.store.write().await;
        let mut ci = store.by_address(ch.clone()).await?;

        // Check if voucher has already been added
        for mut vi in ci.vouchers.iter_mut() {
            if sv != vi.voucher {
                continue;
            }

            // This is a duplicate voucher.
            // Update the proof on the existing voucher
            if (proof.len() > 0) & (vi.proof != proof) {
                warn!("adding proof to stored voucher");
                vi.proof = proof.clone();
                store.put_channel_info(ci).await?;
                return Ok(BigInt::from(1));
            }
            warn!("Voucher re-added with matching proof");
            return Ok(BigInt::default());
        }

        // Check voucher validity
        let lane_states = self.check_voucher_valid(ch, sv.clone()).await?;

        // the change in value is teh delta between the voucher amount and the highest
        // previous voucher amount for the lane
        let mut redeemed = BigInt::default();
        let lane_state = lane_states.get(&sv.lane);
        if let Some(redeem) = lane_state {
            redeemed = redeem.redeemed.clone();
        }

        let delta = sv.amount.clone() - redeemed;

        if min_delta > delta {
            return Err(Error::Other("supplied toekn amount too  low".to_string()));
        }

        ci.vouchers.push(VoucherInfo {
            voucher: sv.clone(),
            proof,
        });

        if ci.next_lane <= sv.lane {
            ci.next_lane += 1;
        }

        store.put_channel_info(ci).await?;
        Ok(delta)
    }

    pub async fn allocate_lane(&self, ch: Address) -> Result<u64, Error> {
        // TODO should this take into account lane state? (TODO pulled from lotus)
        let mut store = self.store.write().await;
        // TODO check this because there is likely to be some issues with locking
        store.allocate_lane(ch).await
    }

    pub async fn list_vouchers(&self, ch: Address) -> Result<Vec<VoucherInfo>, Error> {
        let store = self.store.read().await;
        // TODO possibly add some sort of filtering
        store.vouchers_for_paych(&ch).await
    }

    pub async fn next_sequence_for_lane(&self, ch: Address, lane: u64) -> Result<u64, Error> {
        let store = self.store.read().await;
        // TODO should lane state be taken into account?
        let vouchers = store.vouchers_for_paych(&ch).await?;

        let mut max_sequence = 0;

        for v in vouchers {
            if v.voucher.lane == lane {
                if max_sequence < v.voucher.nonce {
                    max_sequence = v.voucher.nonce;
                }
            }
        }
        return Ok(max_sequence + 1);
    }

    // get the lanestates from chain, then apply all vouchers in the data store over the chain state
    pub async fn lane_state(
        &self,
        _state: &PaychState,
        _ch: Address,
    ) -> Result<HashMap<u64, LaneState>, Error> {
        // TODO should call update channel state with all vouchers to be fully correct (note taken from lotus)
        unimplemented!()
    }

    pub async fn total_redeemed_with_voucher(
        &self,
        lane_states: &HashMap<u64, LaneState>,
        sv: SignedVoucher,
    ) -> Result<BigInt, Error> {
        // implement call with merges
        if sv.merges.len() != 0 {
            return Err(Error::Other("merges not supported yet".to_string()));
        }

        let mut total = BigInt::default();
        for ls in lane_states.values() {
            let val = total.add(ls.nonce);
            total = val
        }

        let lane_ret = lane_states.get(&sv.lane);
        if let Some(lane) = lane_ret {
            // If the voucher is for an existing lane, and the voucher nonce is higher than the lane nonce
            if sv.nonce > lane.nonce {
                // add the delta between the redeemed amount and the voucher
                // amount to the total
                total += sv.amount.sub(&lane.redeemed);
            }
        } else {
            // If the voucher is not for an existing lane, add its value
            total += sv.amount
        }

        Ok(total)
    }

    pub async fn settle(&self, ch: Address) -> Result<Cid, Error> {
        let mut store = self.store.write().await;
        let mut ci = store.by_address(ch.clone()).await?;
        // TODO update method_num and add method_num to this message
        let _umsg: UnsignedMessage = UnsignedMessage::builder()
            .to(ch)
            .from(ci.control)
            .value(BigInt::default())
            .build()
            .map_err(|err| Error::Other(err.to_string()))?;
        // TODO need to push message to messagepool
        // need to return signed message cid
        ci.settling = true;
        store.put_channel_info(ci).await?;
        // TODO return msg cid
        unimplemented!()
    }

    pub async fn collect(&self, ch: Address) -> Result<Cid, Error> {
        let store = self.store.read().await;
        let ci = store.by_address(ch.clone()).await?;
        // TODO update method_num and add method_num to this message
        let _umsg: UnsignedMessage = UnsignedMessage::builder()
            .to(ch)
            .from(ci.control)
            .value(BigInt::default())
            .build()
            .map_err(|err| Error::Other(err.to_string()))?;
        // TODO sign message with message pool and return signed message cid
        unimplemented!()
    }

    // getPaych ensures that a channel exists between the from and to addresses,
    // and adds the given amount of funds.
    // If the channel does not exist a create channel message is sent and the
    // message CID is returned.
    // If the channel does exist an add funds message is sent and both the channel
    // address and message CID are returned.
    // If there is an in progress operation (create channel / add funds), getPaych
    // blocks until the previous operation completes, then returns both the channel
    // address and the CID of the new add funds message.
    // If an operation returns an error, subsequent waiting operations will still
    // be attempted.
    pub async fn get_paych(
        &self,
        from: Address,
        to: Address,
        amt: BigInt,
    ) -> Result<PaychFundsRes, Error> {
        // add the request to add funds to a queue and wait for the result
        let freq = FundsReq::new(from, to, amt);
        let mut sub = freq.promise().await;
        self.enqueue(freq).await?;

        // if there is no promise, block and wait until one is made
        loop {
            let f = sub.next().await;
            if f.is_some() {
                let promise = f.clone().unwrap();
                return Ok(promise);
            }
        }
    }

    pub async fn enqueue(&self, task: FundsReq) -> Result<(), Error> {
        let mut funds_req_vec = self.funds_req_queue.write().await;
        funds_req_vec.push(task);
        drop(funds_req_vec);
        self.process_queue().await
    }

    /// Run operations in the queue
    pub async fn process_queue(&self) -> Result<(), Error> {
        // Remove cancelled requests
        self.filter_queue().await;

        let funds_req_queue = self.funds_req_queue.read().await;

        // if funds req queue is empty return
        if funds_req_queue.len() == 0 {
            return Ok(());
        }

        // Merge all pending requests into one.
        // For example if there are pending requests for 3, 2, 4 then
        // amt = 3 + 2 + 4 = 9
        let mut merged = MergeFundsReq::new(funds_req_queue.clone())
            .ok_or_else(|| Error::Other("MergeFunds creation".to_owned()))?;
        let amt = merged.sum();
        if amt == BigInt::default() {
            // Note: The amount can be zero if requests are cancelled while
            // building the mergedFundsReq
            return Ok(());
        }

        // drop read lock to allow process_task to acquire write lock on self
        // TODO check if this is necessary
        drop(funds_req_queue);

        let res = self.process_task(merged.from()?, merged.to()?, amt).await;

        // If the task is waiting on an external event (eg something to appear on
        // chain) it will return
        if res.is_none() {
            // Stop processing the fundsReqQueue and wait. When the event occurs it will
            // call process_queue() again
            return Ok(());
        }

        let mut queue = self.funds_req_queue.write().await;
        queue.clear();

        merged.on_complete(res.unwrap()).await;
        Ok(())
    }

    /// Remove all inactive fund requests from self
    pub async fn filter_queue(&self) {
        let mut queue = self.funds_req_queue.write().await;
        // Remove cancelled requests
        queue.retain(|val| val.active);
    }

    // processTask checks the state of the channel and takes appropriate action
    // (see description of getPaych).
    // Note that process_task may be called repeatedly in the same state, and should
    // return none if there is no state change to be made (eg when waiting for a
    // message to be confirmed on chain)
    pub async fn process_task(
        &self,
        from: Address,
        to: Address,
        amt: BigInt,
    ) -> Option<PaychFundsRes> {
        // Get the payment channel for the from/to addresses.
        // Note: It's ok if we get ErrChannelNotTracked. It just means we need to
        // create a channel.
        let store = self.store.write().await;
        let channel_info_res = store.outbound_active_by_from_to(from, to).await;
        if channel_info_res.is_err() {
            let err = channel_info_res.err().unwrap();
            if err == Error::ChannelNotTracked {
                return Some(PaychFundsRes {
                    channel: None,
                    mcid: None,
                    err: Some(err),
                });
            }

            // If a channel has not yet been created, create one.
            let mcid = self.create_paych(from, to, amt).await;
            if mcid.is_err() {
                let err = mcid.err().unwrap();
                return Some(PaychFundsRes {
                    channel: None,
                    mcid: None,
                    err: Some(err),
                });
            }
            return Some(PaychFundsRes {
                channel: None,
                mcid: Some(mcid.ok()?),
                err: None,
            });
        }

        // If the create channel message has been sent but the channel hasn't
        // been created on chain yet
        let channel_info = channel_info_res.ok()?;
        if channel_info.create_msg.is_some() {
            // Wait for the channel to be created before trying again
            return None;
        }

        // If add funds message was sent to the chain but hasn't been confirmed to cover the
        // amount for the request
        if channel_info.add_funds_msg != None {
            // Wait for the add funds message to be confirmed before trying again
            return None;
        }

        // We need to add more funds, so send an add funds message to
        // cover the amount for this request
        let mcid = self.add_funds(&channel_info, amt).await.ok()?;

        Some(PaychFundsRes {
            channel: channel_info.channel.clone(),
            mcid: Some(mcid),
            err: None,
        })
    }

    // createPaych sends a message to create the channel and returns the message cid
    pub async fn create_paych(
        &self,
        from: Address,
        to: Address,
        amt: BigInt,
    ) -> Result<Cid, Error> {
        let params: ConstructorParams = ConstructorParams {
            from: from.clone(),
            to: to.clone(),
        };
        let serialized =
            Serialized::serialize(params).map_err(|err| Error::Other(err.to_string()))?;
        let exec: ExecParams = ExecParams {
            code_cid: Default::default(),
            constructor_params: serialized,
        };
        let param = Serialized::serialize(exec).map_err(|err| Error::Other(err.to_string()))?;
        let _msg: UnsignedMessage = UnsignedMessage::builder()
            .from(from)
            .to(to)
            .value(amt.clone())
            .params(param)
            .build()
            .map_err(|err| Error::Other(err.to_string()))?;
        // TODO sign message and push to message pool then get smsg cid
        let mcid = Cid::default();

        // create a new channel in the store
        let mut store = self.store.write().await;
        let _ci = store.create_channel(from, to, mcid.clone(), amt).await?;

        // TODO add functionality to wait for mcid to appear on chain and store the address of the created paych

        Ok(mcid)
    }

    pub async fn add_funds(&self, ci: &ChannelInfo, amt: BigInt) -> Result<Cid, Error> {
        let to = ci
            .channel
            .clone()
            .ok_or_else(|| Error::Other("no addr".to_owned()))?;
        let from = ci.control.clone();
        let _msg: UnsignedMessage = UnsignedMessage::builder()
            .to(to)
            .from(from)
            .value(amt.clone())
            .method_num(0)
            .build()
            .unwrap();
        // TODO sign msg and get the cid of signed message
        let mcid = Cid::default();

        let mut store = self.store.write().await;

        // If there's an error reading or writing to the store just log an error.
        // For now we're assuming it's unlikely to happen in practice.
        // Later we may want to implement a transactional approach, whereby
        // we record to the store that we're going to send a message, send
        // the message, and then record that the message was sent.
        let ci_res = store.by_channel_id(&ci.id).await;
        match ci_res {
            Ok(mut channel_info) => {
                channel_info.pending_amount = amt;
                channel_info.add_funds_msg = Some(mcid.clone());

                // call mutate function
                let res = store.put_channel_info(channel_info.clone()).await;
                if res.is_err() {
                    warn!("Error writing channel info to store: {}", res.unwrap_err());
                }
            }
            Err(err) => warn!("Error reading channel info from store: {}", err),
        }

        let res = store.save_new_message(ci.id.clone(), mcid.clone()).await;
        if res.is_err() {
            warn!("saving add funds message cid: {}", res.unwrap_err())
        }

        // need to add ability to wait for mcid to appear on the chain

        Ok(mcid)
    }
}

/// Response to a channel or add funds request
/// This struct will contain EITHER channel OR mcid OR err
#[derive(Clone, Debug)]
pub struct PaychFundsRes {
    pub channel: Option<Address>,
    pub mcid: Option<Cid>,
    pub err: Option<Error>,
}

/// Request to create a channel or add funds to a channel
#[derive(Clone)]
pub struct FundsReq {
    // this is set to None by default and will be added when? TODO
    promise: Option<PaychFundsRes>,
    from: Address,
    to: Address,
    amt: BigInt,
    active: bool,
    merge: Option<MergeFundsReq>,
    publisher: Arc<RwLock<Publisher<PaychFundsRes>>>,
}

impl FundsReq {
    pub fn new(from: Address, to: Address, amt: BigInt) -> Self {
        FundsReq {
            promise: None,
            from,
            to,
            amt,
            active: true,
            merge: None,
            publisher: Arc::new(RwLock::new(Publisher::new(100))),
        }
    }

    // This will be the pub sub impl that is equivalent to the channel interface of Lotus
    pub async fn promise(&self) -> Subscriber<PaychFundsRes> {
        self.publisher.write().await.subscribe()
    }

    /// This is called when the funds request has been executed
    pub async fn on_complete(&mut self, res: PaychFundsRes) {
        self.promise = Some(res.clone());
        let mut publisher = self.publisher.write().await;
        publisher.publish(res.clone());
    }

    pub fn cancel(&mut self) {
        self.active = false;
        let m = self.merge.clone();
        if m.is_some() {
            m.unwrap().check_active();
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn set_merge_parent(&mut self, m: MergeFundsReq) {
        self.merge = Some(m);
    }
}

// mergedFundsReq merges together multiple add funds requests that are queued
// up, so that only one message is sent for all the requests (instead of one
// message for each request)
#[derive(Clone)]
pub struct MergeFundsReq {
    reqs: Vec<FundsReq>,
    any_active: bool,
}

impl MergeFundsReq {
    pub fn new(reqs: Vec<FundsReq>) -> Option<Self> {
        let mut any_active = false;
        for i in reqs.iter() {
            if i.active {
                any_active = true
            }
        }
        if any_active {
            return Some(MergeFundsReq { reqs, any_active });
        }
        None
    }

    pub fn check_active(&self) -> bool {
        for val in self.reqs.iter() {
            if val.active {
                return true;
            }
        }
        // TODO cancel all active requests
        return false;
    }

    pub async fn on_complete(&mut self, res: PaychFundsRes) {
        for r in self.reqs.iter_mut() {
            if r.active {
                r.on_complete(res.clone()).await
            }
        }
    }

    /// Return sum of the amounts in all active funds requests
    pub fn sum(&self) -> BigInt {
        let mut sum = BigInt::default();
        for r in self.reqs.iter() {
            if r.active {
                sum = sum.add(&r.amt)
            }
        }
        sum
    }

    pub fn from(&self) -> Result<Address, Error> {
        if self.reqs.is_empty() {
            return Err(Error::Other("Empty FundsReq vec".to_owned()));
        }
        Ok(self.reqs[0].from)
    }

    pub fn to(&self) -> Result<Address, Error> {
        if self.reqs.is_empty() {
            return Err(Error::Other("Empty FundsReq vec".to_owned()));
        }
        Ok(self.reqs[0].to)
    }
}
