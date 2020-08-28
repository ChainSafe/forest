use crate::{PaychStore, MsgListeners, StateAccessor, Manager, ChannelInfo, VoucherInfo};
use address::Address;
use cid::Cid;
use super::Error;
use flo_stream::Subscriber;
use num_bigint::BigInt;
use blockstore::BlockStore;
use async_std::sync::{RwLock, Arc};
use actor::paych::{SignedVoucher, LaneState, State as PaychState};
use actor::account::State as AccountState;
use std::collections::HashMap;
use chain::get_heaviest_tipset;
use std::ops::{Add, Sub};
use message::{SignedMessage, UnsignedMessage};
extern crate log;

// TODO need to add paychapi
pub struct ChannelAccessor<DB> {
    store: Arc<RwLock<PaychStore>>,
    msg_listeners: MsgListeners,
    sa: Arc<StateAccessor<DB>>
}

impl<DB> ChannelAccessor<DB>
where
DB: BlockStore {
    pub fn new(pm: &Manager<DB>) -> Self {
        ChannelAccessor {
            store: pm.store.clone(),
            msg_listeners: MsgListeners::new(),
            sa: pm.sa.clone()
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

        unimplemented!();
        // let vb = sv

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
        unimplemented!()
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
    pub async fn lane_state(&self, state: PaychState, ch: Address) -> Result<HashMap<u64, LaneState>, Error> {
        // TODO should call update channel state with all vouchers to be fully correct (note taken from lotus)
        unimplemented!()
    }

    pub async fn total_redeemed_with_voucher(&self, lane_states: HashMap<u64, LaneState>, sv: SignedVoucher) -> Result<BigInt, Error> {
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
        // TODO need to push message to messagepool
        ci.settling = true;
        store.put_channel_info(ci).await?;
        // need to return signed message cid
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
}

// mergedFundsReq merges together multiple add funds requests that are queued
// up, so that only one message is sent for all the requests (instead of one
// message for each request)
#[derive(Clone)]
pub struct MergeFundsReq {
    reqs: Vec<FundsReq>
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