// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Contains mock implementations for testing internal MessagePool APIs

use crate::msgpool::{Publisher, Subscriber};
use crate::provider::Provider;
use crate::Error;
use address::{Address, Protocol};
use async_std::sync::Arc;
use async_trait::async_trait;
use blocks::TipsetKeys;
use blocks::{BlockHeader, ElectionProof, Ticket, Tipset};
use chain::HeadChange;
use cid::Cid;
use crypto::VRFProof;
use message::ChainMessage;
use message::Message;
use message::{SignedMessage, UnsignedMessage};
use num_bigint::BigInt;
use std::collections::HashMap;
use std::convert::TryFrom;
use tokio::sync::broadcast;
use vm::ActorState;

/// Struct used for creating a provider when writing tests involving message pool
pub struct TestApi {
    bmsgs: HashMap<Cid, Vec<SignedMessage>>,
    state_sequence: HashMap<Address, u64>,
    balances: HashMap<Address, BigInt>,
    tipsets: Vec<Tipset>,
    publisher: Publisher<HeadChange>,
}

impl Default for TestApi {
    /// Create a new TestApi
    fn default() -> Self {
        let (publisher, _) = broadcast::channel(1);
        TestApi {
            bmsgs: HashMap::new(),
            state_sequence: HashMap::new(),
            balances: HashMap::new(),
            tipsets: Vec::new(),
            publisher,
        }
    }
}

impl TestApi {
    /// Set the state sequence for an Address for TestApi
    pub fn set_state_sequence(&mut self, addr: &Address, sequence: u64) {
        self.state_sequence.insert(*addr, sequence);
    }

    /// Set the state balance for an Address for TestApi
    pub fn set_state_balance_raw(&mut self, addr: &Address, bal: BigInt) {
        self.balances.insert(*addr, bal);
    }

    /// Set the block messages for TestApi
    pub fn set_block_messages(&mut self, h: &BlockHeader, msgs: Vec<SignedMessage>) {
        self.bmsgs.insert(*h.cid(), msgs);
        self.tipsets.push(Tipset::new(vec![h.clone()]).unwrap())
    }

    /// Set the heaviest tipset for TestApi
    pub async fn set_heaviest_tipset(&mut self, ts: Arc<Tipset>) {
        self.publisher.send(HeadChange::Apply(ts)).unwrap();
    }

    pub fn next_block(&mut self) -> BlockHeader {
        let new_block = mock_block_with_parents(
            self.tipsets
                .last()
                .unwrap_or(&Tipset::new(vec![mock_block(1, 1)]).unwrap()),
            1,
            1,
        );
        new_block
    }
}

#[async_trait]
impl Provider for TestApi {
    async fn subscribe_head_changes(&mut self) -> Subscriber<HeadChange> {
        self.publisher.subscribe()
    }

    async fn get_heaviest_tipset(&mut self) -> Option<Arc<Tipset>> {
        Tipset::new(vec![create_header(1)]).ok().map(Arc::new)
    }

    fn put_message(&self, _msg: &ChainMessage) -> Result<Cid, Error> {
        Ok(Cid::default())
    }

    fn get_actor_after(&self, addr: &Address, ts: &Tipset) -> Result<ActorState, Error> {
        let mut msgs: Vec<SignedMessage> = Vec::new();
        for b in ts.blocks() {
            if let Some(ms) = self.bmsgs.get(b.cid()) {
                for m in ms {
                    if m.from() == addr {
                        msgs.push(m.clone());
                    }
                }
            }
        }
        let balance = match self.balances.get(addr) {
            Some(b) => b.clone(),
            None => (10_000_000_000_u64).into(),
        };

        msgs.sort_by_key(|m| m.sequence());
        let mut sequence: u64 = self.state_sequence.get(addr).copied().unwrap_or_default();
        for m in msgs {
            if m.sequence() != sequence {
                break;
            }
            sequence += 1;
        }
        let actor = ActorState::new(Cid::default(), Cid::default(), balance, sequence);
        Ok(actor)
    }

    fn messages_for_block(
        &self,
        h: &BlockHeader,
    ) -> Result<(Vec<UnsignedMessage>, Vec<SignedMessage>), Error> {
        let v: Vec<UnsignedMessage> = Vec::new();
        let thing = self.bmsgs.get(h.cid());

        match thing {
            Some(s) => Ok((v, s.clone())),
            None => {
                let temp: Vec<SignedMessage> = Vec::new();
                Ok((v, temp))
            }
        }
    }

    async fn state_account_key<V>(
        &self,
        addr: &Address,
        _ts: &Arc<Tipset>,
    ) -> Result<Address, Error> {
        match addr.protocol() {
            Protocol::BLS | Protocol::Secp256k1 => Ok(*addr),
            _ => Err(Error::Other("given address was not a key addr".to_string())),
        }
    }

    fn messages_for_tipset(&self, h: &Tipset) -> Result<Vec<ChainMessage>, Error> {
        let (us, s) = self.messages_for_block(&h.blocks()[0]).unwrap();
        let mut msgs = Vec::new();

        for msg in us {
            msgs.push(ChainMessage::Unsigned(msg));
        }
        for smsg in s {
            msgs.push(ChainMessage::Signed(smsg));
        }
        Ok(msgs)
    }

    async fn load_tipset(&self, tsk: &TipsetKeys) -> Result<Arc<Tipset>, Error> {
        for ts in &self.tipsets {
            if tsk.cids == ts.cids() {
                return Ok(ts.clone().into());
            }
        }
        Err(Error::InvalidToAddr)
    }

    fn chain_compute_base_fee(&self, _ts: &Tipset) -> Result<BigInt, Error> {
        Ok(100.into())
    }
}

pub fn create_header(weight: u64) -> BlockHeader {
    BlockHeader::builder()
        .weight(BigInt::from(weight))
        .miner_address(Address::new_id(0))
        .build()
        .unwrap()
}

pub fn mock_block(weight: u64, ticket_sequence: u64) -> BlockHeader {
    let addr = Address::new_id(1234561);
    let c = Cid::try_from("bafyreicmaj5hhoy5mgqvamfhgexxyergw7hdeshizghodwkjg6qmpoco7i").unwrap();

    let fmt_str = format!("===={}=====", ticket_sequence);
    let ticket = Ticket::new(VRFProof::new(fmt_str.clone().into_bytes()));
    let election_proof = ElectionProof {
        win_count: 0,
        vrfproof: VRFProof::new(fmt_str.into_bytes()),
    };
    let weight_inc = BigInt::from(weight);
    BlockHeader::builder()
        .miner_address(addr)
        .election_proof(Some(election_proof))
        .ticket(Some(ticket))
        .message_receipts(c)
        .messages(c)
        .state_root(c)
        .weight(weight_inc)
        .build()
        .unwrap()
}

pub fn mock_block_with_epoch(epoch: i64, weight: u64, ticket_sequence: u64) -> BlockHeader {
    let addr = Address::new_id(1234561);
    let c = Cid::try_from("bafyreicmaj5hhoy5mgqvamfhgexxyergw7hdeshizghodwkjg6qmpoco7i").unwrap();

    let fmt_str = format!("===={}=====", ticket_sequence);
    let ticket = Ticket::new(VRFProof::new(fmt_str.clone().into_bytes()));
    let election_proof = ElectionProof {
        win_count: 0,
        vrfproof: VRFProof::new(fmt_str.into_bytes()),
    };
    let weight_inc = BigInt::from(weight);
    BlockHeader::builder()
        .miner_address(addr)
        .election_proof(Some(election_proof))
        .ticket(Some(ticket))
        .message_receipts(c)
        .messages(c)
        .state_root(c)
        .weight(weight_inc)
        .epoch(epoch)
        .build()
        .unwrap()
}
pub fn mock_block_with_parents(parents: &Tipset, weight: u64, ticket_sequence: u64) -> BlockHeader {
    let addr = Address::new_id(1234561);
    let c = Cid::try_from("bafyreicmaj5hhoy5mgqvamfhgexxyergw7hdeshizghodwkjg6qmpoco7i").unwrap();

    let height = parents.epoch() + 1;

    let mut weight_inc = BigInt::from(weight);
    weight_inc = parents.blocks()[0].weight() + weight_inc;
    let fmt_str = format!("===={}=====", ticket_sequence);
    let ticket = Ticket::new(VRFProof::new(fmt_str.clone().into_bytes()));
    let election_proof = ElectionProof {
        win_count: 0,
        vrfproof: VRFProof::new(fmt_str.into_bytes()),
    };
    BlockHeader::builder()
        .miner_address(addr)
        .election_proof(Some(election_proof))
        .ticket(Some(ticket))
        .parents(parents.key().clone())
        .message_receipts(c)
        .messages(c)
        .state_root(*parents.blocks()[0].state_root())
        .weight(weight_inc)
        .epoch(height)
        .build()
        .unwrap()
}
