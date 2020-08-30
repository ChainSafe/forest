use crate::{PaychStore, MsgListeners, StateAccessor, Manager, ChannelInfo, VoucherInfo};
use address::Address;
use cid::Cid;
use super::Error;
use flo_stream::Subscriber;
use num_bigint::BigInt;
use blockstore::BlockStore;
use async_std::sync::{RwLock, Arc};
use actor::paych::{SignedVoucher, LaneState, State as PaychState, UpdateChannelStateParams};
use actor::account::State as AccountState;
use std::collections::HashMap;
use chain::get_heaviest_tipset;
use std::ops::{Add, Sub};
use message::{SignedMessage, UnsignedMessage};
extern crate log;
use crypto::Signature;
use actor::paych::Method::UpdateChannelState;

// TODO need to add paychapi
pub struct ChannelAccessor<DB> {
    store: Arc<RwLock<PaychStore>>,
    msg_listeners: MsgListeners,
    sa: Arc<StateAccessor<DB>>,
    funds_req_queue: Arc<RwLock<Vec<FundsReq>>>
}

impl<DB> ChannelAccessor<DB>
where
DB: BlockStore {
    pub fn new(pm: &Manager<DB>) -> Self {
        ChannelAccessor {
            store: pm.store.clone(),
            msg_listeners: MsgListeners::new(),
            sa: pm.sa.clone(),
            funds_req_queue: Arc::new(RwLock::new(Vec::new()))
        }
    }

    pub async fn get_channel_info(&self, addr: &Address) -> Result<ChannelInfo, Error> {
        self.store.read().await.get_channel_info(addr).await
    }

    pub async fn check_voucher_valid(&self, ch: Address, sv: SignedVoucher) -> Result<HashMap<u64, LaneState>, Error> {
        let sm = self.sa.sm.read().await;
        if sv.channel_addr != ch {
            return Err(Error::Other("voucher channel address dpesm't match channel address".to_string()))
        }

        let (act, pch_state) = self.sa.load_paych_state(&ch).await?;
        let heaviest_ts = get_heaviest_tipset(sm.get_block_store().as_ref()).map_err(|_| Error::HeaviestTipset)?.ok_or_else(|| Error::HeaviestTipset)?;
        let cid = heaviest_ts.parent_state();
        let act_state: AccountState = sm.load_actor_state(&pch_state.from, cid).map_err(|err| Error::Other(err.to_string()))?;
        let from = act_state.address;

        let vb = sv.signing_bytes().map_err(|err| Error::Other(err.to_string()))?;

        let sig = sv.signature.clone();
        sig.ok_or_else(|| Error::Other("no sig".to_owned()))?.verify(&vb, &from).map_err(|err| Error::Other(err))?;

        let lane_states = self.lane_state(&pch_state, ch).await?;
        let ls = lane_states.get(&sv.lane).ok_or_else(|| Error::Other("No lane state for given nonce".to_owned()))?;
        if ls.nonce >= sv.nonce {
            return Err(Error::Other("nonce too low".to_owned()))
        }
        if ls.redeemed >= sv.amount {
            return Err(Error::Other("Voucher amount is lower than amount for voucher amount for voucher with lower nonce".to_owned()))
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
            return Err(Error::Other("Not enough funds in channel to cover voucher".to_owned()))
        }

        if merge_len != 0 {
            return Err(Error::Other("don't currently support paych lane merges".to_owned()))
        }

        return Ok(lane_states)
    }

    pub async fn check_voucher_spendable(&self, ch: Address, sv: SignedVoucher, secret: Vec<u8>, mut proof: Vec<u8>) -> Result<bool, Error> {
        let recipient = self.get_paych_recipient(&ch).await?;
        if (sv.extra != None) & (proof.len() != 0) {
            let mut store = self.store.write().await;
            let known = store.vouchers_for_paych(&ch).await?;
            for vi in known {
                if (proof == vi.proof) & (sv == vi.voucher) {
                    info!("using stored proof");
                    proof = vi.proof;
                    break
                }
                if proof.len() == 0 {
                    log::warn!("empty proof for voucher with validation")
                }
            }
        }
        // TODO need to do this


        let enc: UpdateChannelStateParams = UpdateChannelStateParams {
            sv,
            secret,
            proof
        };

        unimplemented!();
    }

    pub async fn get_paych_recipient(&self, ch: &Address) -> Result<Address, Error> {
        let sm = self.sa.sm.read().await;
        let heaviest_ts = get_heaviest_tipset(sm.get_block_store().as_ref()).map_err(|_| Error::HeaviestTipset)?.ok_or_else(|| Error::HeaviestTipset)?;
        let cid = heaviest_ts.parent_state();
        let state: PaychState = sm.load_actor_state(ch, cid).map_err(|err| Error::Other(err.to_string()))?;
        Ok(state.to)
    }

    pub async fn add_voucher(&self, ch: Address, sv: SignedVoucher, proof: Vec<u8>, min_delta: BigInt) -> Result<BigInt, Error> {
        let mut store = self.store.write().await;
        let mut ci = store.by_address(ch.clone()).await?;

        // Check if voucher has already been added
        for mut vi in ci.vouchers.iter_mut() {
            if sv != vi.voucher {
                continue
            }

            // This is a duplicate voucher.
            // Update the proof on the existing voucher
            if (proof.len() > 0) & (vi.proof != proof) {
                warn!("adding proof to stored voucher");
                vi.proof = proof.clone();
                store.put_channel_info(ci).await?;
                return Ok(BigInt::from(1))
            }
            warn!("Voucher re-added with matching proof");
            return Ok(BigInt::default())
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
            proof
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
        return Ok(max_sequence + 1)
    }

    // get the lanestates from chain, then apply all vouchers in the data store over the chain state
    pub async fn lane_state(&self, state: &PaychState, ch: Address) -> Result<HashMap<u64, LaneState>, Error> {
        // TODO should call update channel state with all vouchers to be fully correct (note taken from lotus)
        unimplemented!()
    }

    pub async fn total_redeemed_with_voucher(&self, lane_states: &HashMap<u64, LaneState>, sv: SignedVoucher) -> Result<BigInt, Error> {
        // implement call with merges
        if sv.merges.len() != 0 {
            return Err(Error::Other("merges not supported yet".to_string()))
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
        let umsg: UnsignedMessage = UnsignedMessage::builder().to(ch).from(ci.control).value(BigInt::default()).build().map_err(|err| Error::Other(err.to_string()))?;
        ci.settling = true;
        store.put_channel_info(ci).await?;
        // TODO need to push message to messagepool
        // need to return signed message cid
        unimplemented!();
        ci.settling = true;
        store.put_channel_info(ci)?;
        // TODO return msg cid
        unimplemented!()
    }

    pub async fn collect(&self, ch: Address) -> Result<Cid, Error> {
        let mut store = self.store.write().await;
        let ci = store.by_address(ch.clone()).await?;
        // TODO update method_num and add method_num to this message
        let umsg: UnsignedMessage = UnsignedMessage::builder().to(ch).from(ci.control).value(BigInt::default()).build().map_err(|err| Error::Other(err.to_string()))?;
        // TODO sign message with message pool and return signed message cid
        unimplemented!()
    }

    pub async fn get_paych(&self, from: Address, to: Address, amt: BigInt) -> Result<Address, Error> {
        // add the request to add funds to a queue and wait for the result
        let freq = FundsReq::new(from, to, amt);

        unimplemented!()
    }

    // Run operations in the queue
    pub async fn process_queue(&self) {
        // Remove cancelled requests
        self.filter_queue().await;

        unimplemented!()
    }

    pub async fn filter_queue(&self) {
        let mut queue = self.funds_req_queue.write().await;
        // Remove cancelled requests
        queue.retain(|val| val.active);
    }
}


/// Response to a channel or add funds request
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaychFundsRes {
    channel: Address,
    mcid: Cid,
    err: String,
}

/// Request to create a channel or add funds to a channel
#[derive(Clone)]
pub struct FundsReq {
    // this is set to None by default and will be added when? TODO
    promise: Option<Arc<Subscriber<PaychFundsRes>>>,
    from: Address,
    to: Address,
    amt: BigInt,
    active: bool,
    merge: Option<MergeFundsReq>,
}

impl FundsReq {
    pub fn new(from: Address, to: Address, amt: BigInt, ) -> Self {
        FundsReq {
            promise: None,
            from,
            to,
            amt,
            active: true,
            merge: None
        }
    }

    /// This is called when the funds request has been executed
    pub fn on_complete(&self, res: PaychFundsRes) {
        unimplemented!()
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
            return Some(MergeFundsReq { reqs, any_active })
        }
        None
    }

    pub fn check_active(&self) -> bool {
        for val in self.reqs.iter() {
            if val.active {
                return true
            }
        }
        // TODO cancell all active requests
        return false
    }

    pub fn on_complete(&self, res: PaychFundsRes) {
        for r in self.reqs.iter() {
            if r.active {
                r.on_complete(res.clone())
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
}

// pub async fn lane_state(&self, ch: Address, lane: u64) -> Result<LaneState, Error> {
//     let (_, state) = self.load_paych_state(&ch).await?;
//     let ls = find_lane(state.lane_states, lane).unwrap_or(LaneState {
//         id: lane,
//         redeemed: BigInt::default(),
//         nonce: 0,
//     });
//     unimplemented!()
// }