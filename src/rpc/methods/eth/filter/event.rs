// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::FilterError;
use crate::blocks::{Tipset, TipsetKey};
use crate::rpc::eth::filter::ActorEventBlock;
use crate::rpc::eth::{filter::Filter, FilterID};
use crate::rpc::Arc;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::executor::Receipt;
use crate::shim::message::Message;
use crate::shim::state_tree::ActorID;
use ahash::AHashMap as HashMap;
use cid::Cid;
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::any::Any;
use std::error::Error;
use std::time::SystemTime;
use tokio::sync::mpsc::Sender;

// Define the constants for event flags.
const EVENT_FLAG_INDEXED_KEY: u8 = 0b00000001;
const EVENT_FLAG_INDEXED_VALUE: u8 = 0b00000010;

pub fn is_indexed_value(b: u8) -> bool {
    b & (EVENT_FLAG_INDEXED_KEY | EVENT_FLAG_INDEXED_VALUE) > 0
}

type AddressResolver = Arc<dyn Fn(&(), ActorID, &Tipset) -> (Address, bool) + Send + Sync>;
type LoadExecutedMessagesFn =
    Arc<dyn Fn(&Tipset, &Tipset) -> Result<Vec<ExecutedMessage>, Box<dyn Error>> + Send + Sync>;

#[derive(Clone, Debug)]
pub struct Event {
    emitter: ActorID,
    entries: Vec<EventEntry>,
}

#[derive(Clone, Debug)]
pub struct EventEntry {
    flags: u8,
    key: String,
    codec: u64,
    value: Vec<u8>,
}

#[derive(Clone, Debug)]
struct ExecutedMessage {
    msg: Message,
    rct: Option<Receipt>,
    evs: Vec<Event>,
}

impl ExecutedMessage {
    fn message(&self) -> &Message {
        &self.msg
    }

    fn receipt(&self) -> Option<&Receipt> {
        self.rct.as_ref()
    }

    fn events(&self) -> &[Event] {
        &self.evs
    }
}

#[derive(Clone)]
struct TipsetEvents {
    rct_ts: Arc<Tipset>,
    msg_ts: Arc<Tipset>,
    load: LoadExecutedMessagesFn,
    ems: OnceCell<Vec<ExecutedMessage>>,
}

impl TipsetEvents {
    fn height(&self) -> i64 {
        self.msg_ts.epoch()
    }

    fn cid(&self) -> Result<Cid, Box<dyn Error>> {
        Ok(self.msg_ts.key().cid()?)
    }

    fn messages(&self) -> Result<&Vec<ExecutedMessage>, Box<dyn Error>> {
        self.ems
            .get_or_try_init(|| (self.load)(&self.msg_ts, &self.rct_ts))
    }
}

#[derive(Debug, Clone)]
struct CollectedEvent {
    entries: Vec<EventEntry>,
    emitter_addr: Address,
    event_idx: usize,
    reverted: bool,
    height: ChainEpoch,
    tipset_key: TipsetKey,
    msg_idx: usize,
    msg_cid: Cid,
}

#[derive(Debug)]
pub struct EventFilter {
    id: FilterID,
    min_height: ChainEpoch,
    max_height: ChainEpoch,
    tipset_cid: Cid,
    addresses: Vec<Address>,
    keys_with_codec: HashMap<String, Vec<ActorEventBlock>>,
    max_results: usize,
    collected: Mutex<Vec<CollectedEvent>>,
    last_taken: Mutex<SystemTime>,
    sub_channel: Mutex<Option<Sender<Box<dyn Any + Send>>>>,
}

impl EventFilter {
    fn new(
        id: FilterID,
        min_height: ChainEpoch,
        max_height: ChainEpoch,
        tipset_cid: Cid,
        addresses: Vec<Address>,
        keys_with_codec: HashMap<String, Vec<ActorEventBlock>>,
        max_results: usize,
    ) -> Self {
        EventFilter {
            id,
            min_height,
            max_height,
            tipset_cid,
            addresses,
            keys_with_codec,
            max_results,
            collected: Mutex::new(Vec::new()),
            last_taken: Mutex::new(SystemTime::now()),
            sub_channel: Mutex::new(None),
        }
    }

    fn match_tipset(&self, te: &TipsetEvents) -> bool {
        if self.tipset_cid != Cid::default() {
            match te.cid() {
                Ok(ts_cid) => self.tipset_cid == ts_cid,
                Err(_) => false,
            }
        } else {
            (self.min_height < 0 || self.min_height <= te.height())
                && (self.max_height < 0 || self.max_height >= te.height())
        }
    }

    fn match_address(&self, address: &Address) -> bool {
        self.addresses.is_empty() || self.addresses.contains(address)
    }

    fn match_keys(&self, entries: &[EventEntry]) -> bool {
        if self.keys_with_codec.is_empty() {
            return true;
        }

        let mut matched = HashMap::new();
        for ee in entries.iter().filter(|e| is_indexed_value(e.flags)) {
            if matched.contains_key(&ee.key) {
                continue;
            }

            if let Some(wantlist) = self.keys_with_codec.get(&ee.key) {
                if wantlist
                    .iter()
                    .any(|w| w.value == ee.value && w.codec == ee.codec)
                {
                    matched.insert(ee.key.clone(), true);
                }
            }

            if matched.len() == self.keys_with_codec.len() {
                return true;
            }
        }

        false
    }
}

impl Filter for EventFilter {
    fn id(&self) -> FilterID {
        self.id.clone()
    }

    fn last_taken(&self) -> SystemTime {
        *self.last_taken.lock()
    }

    fn set_sub_channel(&self, sub_channel: Sender<Box<dyn Any + Send>>) {
        let mut sc = self.sub_channel.lock();
        *sc = Some(sub_channel);
        self.collected.lock().clear();
    }

    fn clear_sub_channel(&self) {
        let mut sc = self.sub_channel.lock();
        *sc = None;
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

trait FilterExt: Filter {
    fn take_collected_events(&mut self) -> Vec<CollectedEvent>;
    fn collect_events(
        &mut self,
        te: &TipsetEvents,
        revert: bool,
        resolver: &dyn Fn(ActorID, &Tipset) -> Option<Address>,
    );
}

impl FilterExt for EventFilter {
    fn take_collected_events(&mut self) -> Vec<CollectedEvent> {
        let mut collected_lock = self.collected.lock();
        let collected = collected_lock.clone();
        collected_lock.clear();
        *self.last_taken.lock() = SystemTime::now();
        collected
    }

    fn collect_events(
        &mut self,
        te: &TipsetEvents,
        revert: bool,
        resolver: &dyn Fn(ActorID, &Tipset) -> Option<Address>,
    ) {
        if !self.match_tipset(te) {
            return;
        }

        let mut address_lookups: HashMap<ActorID, Address> = HashMap::new();
        let ems = te.messages().unwrap();

        let mut event_count = 0;

        for (msg_idx, em) in ems.iter().enumerate() {
            for ev in em.events() {
                let addr = if let Some(addr) = address_lookups.get(&ev.emitter) {
                    *addr
                } else {
                    match resolver(ev.emitter, &te.rct_ts) {
                        Some(addr) => {
                            address_lookups.insert(ev.emitter, addr);
                            addr
                        }
                        None => continue,
                    }
                };

                if !self.match_address(&addr) || !self.match_keys(&ev.entries) {
                    continue;
                }

                let collected_event = CollectedEvent {
                    entries: ev.entries.clone(),
                    emitter_addr: addr,
                    event_idx: event_count,
                    reverted: revert,
                    height: te.height(),
                    tipset_key: te.msg_ts.key().clone(),
                    msg_cid: em.message().cid(),
                    msg_idx,
                };

                if let Some(ref ch) = *self.sub_channel.lock() {
                    tokio::task::block_in_place(|| {
                        futures::executor::block_on(ch.send(Box::new(collected_event.clone()))).ok()
                    });
                    continue;
                }

                let mut collected_lock = self.collected.lock();
                if self.max_results > 0 && collected_lock.len() == self.max_results {
                    collected_lock.remove(0);
                }
                collected_lock.push(collected_event);
                event_count += 1;
            }
        }
    }
}

pub struct EventFilterManager {
    filters: Mutex<HashMap<FilterID, Arc<dyn Filter>>>,
    max_filter_results: usize,
}

impl EventFilterManager {
    pub fn new(max_filter_results: usize) -> Arc<Self> {
        Arc::new(Self {
            filters: Mutex::new(HashMap::new()),
            max_filter_results,
        })
    }

    pub fn install(
        &self,
        min_height: ChainEpoch,
        max_height: ChainEpoch,
        tipset_cid: Cid,
        addresses: Vec<Address>,
        keys_with_codec: HashMap<String, Vec<ActorEventBlock>>,
        _exclude_reverted: bool,
    ) -> Result<Arc<dyn Filter>, FilterError> {
        let id = FilterID::new()?;

        let filter = Arc::new(EventFilter {
            id: id.clone(),
            min_height,
            max_height,
            tipset_cid,
            addresses,
            keys_with_codec,
            max_results: self.max_filter_results,
            collected: Mutex::new(Vec::new()),
            last_taken: Mutex::new(SystemTime::now()),
            sub_channel: Mutex::new(None),
        });

        self.filters.lock().insert(id, filter.clone());

        Ok(filter)
    }

    pub fn remove(&self, id: &FilterID) -> Result<(), FilterError> {
        let mut filters = self.filters.lock();
        if filters.remove(id).is_none() {
            return Err(FilterError::NotFound);
        }
        Ok(())
    }
}
