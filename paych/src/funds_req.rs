// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Error;
use address::Address;
use async_std::sync::{Arc, RwLock};
use cid::Cid;
use flo_stream::{MessagePublisher, Publisher, Subscriber};
use num_bigint::BigInt;
use std::ops::Add;

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
    promise: Option<PaychFundsRes>,
    from: Address,
    to: Address,
    amt: BigInt,
    pub active: bool,
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
    // TODO can be removed
    pub fn cancel(&mut self) {
        self.active = false;
        let m = self.merge.clone();
        if let Some(ma) = m {
            ma.check_active();
        }
    }
    pub fn is_active(&self) -> bool {
        self.active
    }
    /// sets the merge that this req is part of
    pub fn set_merge_parent(&mut self, m: MergeFundsReq) {
        self.merge = Some(m);
    }
}

/// merges together multiple add funds requests that are queued
/// up, so that only one message is sent for all the requests (instead of one
/// message for each request)
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
        false
    }
    /// Called when the queue has executed the mergeFundsReq.
    /// Calls onComplete on each fundsReq in the mergeFundsReq.
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
