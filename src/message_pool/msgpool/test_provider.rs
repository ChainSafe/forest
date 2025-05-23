// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Contains mock implementations for testing internal `MessagePool` APIs

use std::{convert::TryFrom, sync::Arc};

use crate::blocks::RawBlockHeader;
use crate::blocks::VRFProof;
use crate::blocks::{CachingBlockHeader, ElectionProof, Ticket, Tipset, TipsetKey};
use crate::chain::HeadChange;
use crate::cid_collections::CidHashMap;
use crate::message::{ChainMessage, Message as MessageTrait, SignedMessage};
use crate::shim::{address::Address, econ::TokenAmount, message::Message, state_tree::ActorState};
use ahash::HashMap;
use async_trait::async_trait;
use cid::Cid;
use num::BigInt;
use parking_lot::Mutex;
use tokio::sync::broadcast;

use crate::message_pool::{Error, provider::Provider};
use tokio::sync::broadcast::{Receiver as Subscriber, Sender as Publisher};

/// Structure used for creating a provider when writing tests involving message
/// pool
pub struct TestApi {
    pub inner: Mutex<TestApiInner>,
    pub publisher: Publisher<HeadChange>,
}

#[derive(Default)]
pub struct TestApiInner {
    bmsgs: CidHashMap<Vec<SignedMessage>>,
    state_sequence: HashMap<Address, u64>,
    balances: HashMap<Address, TokenAmount>,
    tipsets: Vec<Tipset>,
    max_actor_pending_messages: u64,
}

impl Default for TestApi {
    /// Create a new `TestApi`
    fn default() -> Self {
        let (publisher, _) = broadcast::channel(1);
        TestApi {
            inner: Mutex::new(TestApiInner {
                max_actor_pending_messages: 20000,
                ..TestApiInner::default()
            }),
            publisher,
        }
    }
}

impl TestApi {
    /// Constructor for a `TestApi` with custom number of max pending messages
    pub fn with_max_actor_pending_messages(max_actor_pending_messages: u64) -> Self {
        let (publisher, _) = broadcast::channel(1);
        TestApi {
            inner: Mutex::new(TestApiInner {
                max_actor_pending_messages,
                ..TestApiInner::default()
            }),
            publisher,
        }
    }

    /// Set the state sequence for an Address for `TestApi`
    pub fn set_state_sequence(&self, addr: &Address, sequence: u64) {
        self.inner.lock().set_state_sequence(addr, sequence)
    }

    /// Set the state balance for an Address for `TestApi`
    pub fn set_state_balance_raw(&self, addr: &Address, bal: TokenAmount) {
        self.inner.lock().set_state_balance_raw(addr, bal)
    }

    /// Set the block messages for `TestApi`
    pub fn set_block_messages(&self, h: &CachingBlockHeader, msgs: Vec<SignedMessage>) {
        self.inner.lock().set_block_messages(h, msgs)
    }

    /// Set the heaviest tipset for `TestApi`
    pub fn set_heaviest_tipset(&self, ts: Arc<Tipset>) {
        self.publisher.send(HeadChange::Apply(ts)).unwrap();
    }

    pub fn next_block(&self) -> CachingBlockHeader {
        self.inner.lock().next_block()
    }
}

impl TestApiInner {
    /// Set the state sequence for an Address for `TestApi`
    pub fn set_state_sequence(&mut self, addr: &Address, sequence: u64) {
        self.state_sequence.insert(*addr, sequence);
    }

    /// Set the state balance for an Address for `TestApi`
    pub fn set_state_balance_raw(&mut self, addr: &Address, bal: TokenAmount) {
        self.balances.insert(*addr, bal);
    }

    /// Set the block messages for `TestApi`
    pub fn set_block_messages(&mut self, h: &CachingBlockHeader, msgs: Vec<SignedMessage>) {
        self.bmsgs.insert(*h.cid(), msgs);
        self.tipsets.push(Tipset::from(h))
    }

    pub fn next_block(&mut self) -> CachingBlockHeader {
        mock_block_with_parents(
            self.tipsets
                .last()
                .unwrap_or(&Tipset::from(mock_block(1, 1))),
            1,
            1,
        )
    }
}

#[async_trait]
impl Provider for TestApi {
    fn subscribe_head_changes(&self) -> Subscriber<HeadChange> {
        self.publisher.subscribe()
    }

    fn get_heaviest_tipset(&self) -> Arc<Tipset> {
        Arc::new(Tipset::from(create_header(1)))
    }

    fn put_message(&self, _msg: &ChainMessage) -> Result<Cid, Error> {
        Ok(Cid::default())
    }

    fn get_actor_after(&self, addr: &Address, ts: &Tipset) -> Result<ActorState, Error> {
        let inner = self.inner.lock();
        let mut msgs: Vec<SignedMessage> = Vec::new();
        for b in ts.block_headers() {
            if let Some(ms) = inner.bmsgs.get(b.cid()) {
                for m in ms {
                    if &m.from() == addr {
                        msgs.push(m.clone());
                    }
                }
            }
        }
        let balance = match inner.balances.get(addr) {
            Some(b) => b.clone(),
            None => TokenAmount::from_atto(10_000_000_000_u64),
        };

        msgs.sort_by_key(|m| m.sequence());
        let mut sequence: u64 = inner.state_sequence.get(addr).copied().unwrap_or_default();
        for m in msgs {
            if m.sequence() != sequence {
                break;
            }
            sequence += 1;
        }
        let actor = ActorState::new(
            // Account Actor code (v10, calibnet)
            Cid::try_from("bafk2bzacebhfuz3sv7duvk653544xsxhdn4lsmy7ol7k6gdgancyctvmd7lnq")
                .unwrap(),
            Cid::default(),
            balance,
            sequence,
            None,
        );

        Ok(actor)
    }

    fn messages_for_block(
        &self,
        h: &CachingBlockHeader,
    ) -> Result<(Vec<Message>, Vec<SignedMessage>), Error> {
        let inner = self.inner.lock();
        let v: Vec<Message> = Vec::new();
        let thing = inner.bmsgs.get(h.cid());

        match thing {
            Some(s) => Ok((v, s.clone())),
            None => {
                let temp: Vec<SignedMessage> = Vec::new();
                Ok((v, temp))
            }
        }
    }

    fn load_tipset(&self, tsk: &TipsetKey) -> Result<Arc<Tipset>, Error> {
        let inner = self.inner.lock();
        for ts in &inner.tipsets {
            if tsk == ts.key() {
                return Ok(ts.clone().into());
            }
        }
        Err(Error::InvalidToAddr)
    }

    fn chain_compute_base_fee(&self, _ts: &Tipset) -> Result<TokenAmount, Error> {
        Ok(TokenAmount::from_atto(100))
    }

    fn max_actor_pending_messages(&self) -> u64 {
        self.inner.lock().max_actor_pending_messages
    }
}

pub fn create_header(weight: u64) -> CachingBlockHeader {
    CachingBlockHeader::new(RawBlockHeader {
        miner_address: Address::new_id(0),
        weight: BigInt::from(weight),
        ..Default::default()
    })
}

pub fn mock_block(weight: u64, ticket_sequence: u64) -> CachingBlockHeader {
    let addr = Address::new_id(1234561);
    let c = Cid::try_from("bafyreicmaj5hhoy5mgqvamfhgexxyergw7hdeshizghodwkjg6qmpoco7i").unwrap();

    let fmt_str = format!("===={ticket_sequence}=====");
    let ticket = Ticket::new(VRFProof::new(fmt_str.clone().into_bytes()));
    let election_proof = ElectionProof {
        win_count: 0,
        vrfproof: VRFProof::new(fmt_str.into_bytes()),
    };
    let weight_inc = BigInt::from(weight);
    CachingBlockHeader::new(RawBlockHeader {
        miner_address: addr,
        election_proof: Some(election_proof),
        ticket: Some(ticket),
        message_receipts: c,
        messages: c,
        state_root: c,
        weight: weight_inc,
        ..Default::default()
    })
}

pub fn mock_block_with_parents(
    parents: &Tipset,
    weight: u64,
    ticket_sequence: u64,
) -> CachingBlockHeader {
    let addr = Address::new_id(1234561);
    let c = Cid::try_from("bafyreicmaj5hhoy5mgqvamfhgexxyergw7hdeshizghodwkjg6qmpoco7i").unwrap();

    let height = parents.epoch() + 1;

    let mut weight_inc = BigInt::from(weight);
    weight_inc = &parents.block_headers().first().weight + weight_inc;
    let fmt_str = format!("===={ticket_sequence}=====");
    let ticket = Ticket::new(VRFProof::new(fmt_str.clone().into_bytes()));
    let election_proof = ElectionProof {
        win_count: 0,
        vrfproof: VRFProof::new(fmt_str.into_bytes()),
    };
    CachingBlockHeader::new(RawBlockHeader {
        miner_address: addr,
        election_proof: Some(election_proof),
        ticket: Some(ticket),
        parents: parents.key().clone(),
        message_receipts: c,
        messages: c,
        state_root: parents.block_headers().first().state_root,
        weight: weight_inc,
        epoch: height,
        ..Default::default()
    })
}
