// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::actorv0::{account, init, ACCOUNT_ACTOR_CODE_ID, INIT_ACTOR_ADDR, INIT_ACTOR_CODE_ID};
use address::Address;
use cid::Code::{Blake2b256, Identity};
use clock::ChainEpoch;
use crypto::DomainSeparationTag;
use db::MemoryDB;
use fil_types::{verifier::MockVerifier, NetworkVersion, StateTreeVersion};
use interpreter::{CircSupplyCalc, DefaultRuntime, LookbackStateGetter, Rand};
use ipld_blockstore::BlockStore;
use ipld_hamt::Hamt;
use message::UnsignedMessage;
use state_tree::StateTree;
use std::collections::HashSet;
use std::error::Error as StdError;
use vm::{ActorState, Serialized, TokenAmount};

struct MockCircSupply;
impl CircSupplyCalc for MockCircSupply {
    fn get_supply<DB: BlockStore>(
        &self,
        _: ChainEpoch,
        _: &StateTree<DB>,
    ) -> Result<TokenAmount, Box<dyn StdError>> {
        Ok(0.into())
    }
}

struct MockStateLB<'db, MemoryDB>(&'db MemoryDB);
impl<'db> LookbackStateGetter<'db, MemoryDB> for MockStateLB<'db, MemoryDB> {
    fn state_lookback(&self, _: ChainEpoch) -> Result<StateTree<'db, MemoryDB>, Box<dyn StdError>> {
        Err("Test shouldn't call lookback".into())
    }
}

struct MockRand;
impl Rand for MockRand {
    fn get_chain_randomness(
        &self,
        _: DomainSeparationTag,
        _: ChainEpoch,
        _: &[u8],
    ) -> Result<[u8; 32], Box<dyn StdError>> {
        Ok(*b"i_am_random_____i_am_random_____")
    }
    fn get_beacon_randomness(
        &self,
        _: DomainSeparationTag,
        _: ChainEpoch,
        _: &[u8],
    ) -> Result<[u8; 32], Box<dyn StdError>> {
        Ok(*b"i_am_random_____i_am_random_____")
    }
}

#[test]
fn transfer_test() {
    let store = MemoryDB::default();

    let mut state = StateTree::new(&store, StateTreeVersion::V0).unwrap();

    let e_cid = Hamt::<_, String>::new_with_bit_width(&store, 5)
        .flush()
        .unwrap();

    // Create and save init actor
    let init_state = init::State::new(e_cid.clone(), "test".to_owned());
    let state_cid = state
        .store()
        .put(&init_state, Blake2b256)
        .map_err(|e| e.to_string())
        .unwrap();

    let act_s = ActorState::new(
        INIT_ACTOR_CODE_ID.clone(),
        state_cid.clone(),
        Default::default(),
        1,
    );
    state.set_actor(&INIT_ACTOR_ADDR, act_s.clone()).unwrap();

    let actor_addr_1 = Address::new_id(100);
    let actor_addr_2 = Address::new_id(200);

    let actor_state_cid_1 = state
        .store()
        .put(
            &account::State {
                address: actor_addr_1.clone(),
            },
            Identity,
        )
        .map_err(|e| e.to_string())
        .unwrap();

    let actor_state_cid_2 = state
        .store()
        .put(
            &account::State {
                address: actor_addr_2.clone(),
            },
            Identity,
        )
        .map_err(|e| e.to_string())
        .unwrap();
    let actor_state_1 = ActorState::new(
        ACCOUNT_ACTOR_CODE_ID.clone(),
        actor_state_cid_1.clone(),
        10000u64.into(),
        0,
    );
    let actor_state_2 = ActorState::new(
        ACCOUNT_ACTOR_CODE_ID.clone(),
        actor_state_cid_2.clone(),
        1u64.into(),
        0,
    );

    let actor_addr_1 = state.register_new_address(&actor_addr_1).unwrap();
    let actor_addr_2 = state.register_new_address(&actor_addr_2).unwrap();
    state.set_actor(&actor_addr_1, actor_state_1).unwrap();
    state.set_actor(&actor_addr_2, actor_state_2).unwrap();

    let message = UnsignedMessage::builder()
        .to(actor_addr_1.clone())
        .from(actor_addr_2.clone())
        .method_num(2)
        .value(1u8.into())
        .gas_limit(10000000)
        .params(Serialized::default())
        .build()
        .unwrap();

    let registered = HashSet::new();

    let lookback = MockStateLB(&store);
    let mut runtime = DefaultRuntime::<_, _, _, _, MockVerifier>::new(
        NetworkVersion::V0,
        &mut state,
        &store,
        0,
        &message,
        0,
        actor_addr_2.clone(),
        0,
        0,
        0,
        &MockRand,
        &registered,
        &MockCircSupply,
        &lookback,
    )
    .unwrap();
    let _serialized = runtime.send(&message, None).unwrap();

    let actor_state_result_1 = state.get_actor(&actor_addr_1).unwrap().unwrap();
    let actor_state_result_2 = state.get_actor(&actor_addr_2).unwrap().unwrap();

    assert_eq!(actor_state_result_1.balance, 10001u64.into());
    assert_eq!(actor_state_result_2.balance, 0u64.into());
    assert_eq!(actor_state_result_1.sequence, 0);
    assert_eq!(actor_state_result_2.sequence, 0);
}
