// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Shared test fixtures for the trace module.

use crate::db::MemoryDB;
use crate::networks::ACTOR_BUNDLES_METADATA;
use crate::rpc::eth::trace::state_diff::build_state_diff;
use crate::rpc::eth::trace::types::StateDiff;
use crate::rpc::eth::types::EthAddress;
use crate::shim::address::Address as FilecoinAddress;
use crate::shim::econ::TokenAmount;
use crate::shim::machine::BuiltinActor;
use crate::shim::state_tree::{ActorState, StateTree, StateTreeVersion};
use crate::utils::db::CborStoreExt as _;
use ahash::HashSet;
use cid::Cid;
use std::sync::Arc;

pub fn create_test_actor(balance_atto: u64, sequence: u64) -> ActorState {
    ActorState::new(
        Cid::default(), // Non-EVM actor code CID
        Cid::default(), // State CID (not used for non-EVM)
        TokenAmount::from_atto(balance_atto),
        sequence,
        None, // No delegated address
    )
}

pub fn get_evm_actor_code_cid() -> Option<Cid> {
    for bundle in ACTOR_BUNDLES_METADATA.values() {
        if bundle.actor_major_version().ok() == Some(17)
            && let Ok(cid) = bundle.manifest.get(BuiltinActor::EVM)
        {
            return Some(cid);
        }
    }
    None
}

pub fn create_evm_actor_with_bytecode(
    store: &MemoryDB,
    balance_atto: u64,
    actor_sequence: u64,
    evm_nonce: u64,
    bytecode: Option<&[u8]>,
) -> Option<ActorState> {
    use fvm_ipld_blockstore::Blockstore as _;

    let evm_code_cid = get_evm_actor_code_cid()?;

    // Store bytecode as raw bytes (not CBOR-encoded)
    let bytecode_cid = if let Some(code) = bytecode {
        use multihash_codetable::MultihashDigest;
        let mh = multihash_codetable::Code::Blake2b256.digest(code);
        let cid = Cid::new_v1(fvm_ipld_encoding::IPLD_RAW, mh);
        store.put_keyed(&cid, code).ok()?;
        cid
    } else {
        Cid::default()
    };

    let bytecode_hash = if let Some(code) = bytecode {
        use keccak_hash::keccak;
        let hash = keccak(code);
        fil_actor_evm_state::v17::BytecodeHash::from(hash.0)
    } else {
        fil_actor_evm_state::v17::BytecodeHash::EMPTY
    };

    let evm_state = fil_actor_evm_state::v17::State {
        bytecode: bytecode_cid,
        bytecode_hash,
        contract_state: Cid::default(),
        transient_data: None,
        nonce: evm_nonce,
        tombstone: None,
    };

    let state_cid = store.put_cbor_default(&evm_state).ok()?;

    Some(ActorState::new(
        evm_code_cid,
        state_cid,
        TokenAmount::from_atto(balance_atto),
        actor_sequence,
        None,
    ))
}

pub fn create_masked_id_eth_address(actor_id: u64) -> EthAddress {
    EthAddress::from_actor_id(actor_id)
}

pub struct TestStateTrees {
    pub store: Arc<MemoryDB>,
    pub pre_state: StateTree<Arc<MemoryDB>>,
    pub post_state: StateTree<Arc<MemoryDB>>,
}

impl TestStateTrees {
    pub fn new() -> anyhow::Result<Self> {
        let store = Arc::new(MemoryDB::default());
        let pre_state = StateTree::new(&store, StateTreeVersion::V5)?;
        let post_state = StateTree::new(&store, StateTreeVersion::V5)?;
        Ok(Self {
            store,
            pre_state,
            post_state,
        })
    }

    /// Create state trees with different actors in pre and post.
    pub fn with_changed_actor(
        actor_id: u64,
        pre_actor: ActorState,
        post_actor: ActorState,
    ) -> anyhow::Result<Self> {
        let store = Arc::new(MemoryDB::default());
        let mut pre_state = StateTree::new(&store, StateTreeVersion::V5)?;
        let mut post_state = StateTree::new(&store, StateTreeVersion::V5)?;
        let addr = FilecoinAddress::new_id(actor_id);
        pre_state.set_actor(&addr, pre_actor)?;
        post_state.set_actor(&addr, post_actor)?;
        Ok(Self {
            store,
            pre_state,
            post_state,
        })
    }

    /// Create state trees with actor only in post (creation scenario).
    pub fn with_created_actor(actor_id: u64, post_actor: ActorState) -> anyhow::Result<Self> {
        let store = Arc::new(MemoryDB::default());
        let pre_state = StateTree::new(&store, StateTreeVersion::V5)?;
        let mut post_state = StateTree::new(&store, StateTreeVersion::V5)?;
        let addr = FilecoinAddress::new_id(actor_id);
        post_state.set_actor(&addr, post_actor)?;
        Ok(Self {
            store,
            pre_state,
            post_state,
        })
    }

    /// Create state trees with actor only in pre (deletion scenario).
    pub fn with_deleted_actor(actor_id: u64, pre_actor: ActorState) -> anyhow::Result<Self> {
        let store = Arc::new(MemoryDB::default());
        let mut pre_state = StateTree::new(&store, StateTreeVersion::V5)?;
        let post_state = StateTree::new(&store, StateTreeVersion::V5)?;
        let addr = FilecoinAddress::new_id(actor_id);
        pre_state.set_actor(&addr, pre_actor)?;
        Ok(Self {
            store,
            pre_state,
            post_state,
        })
    }

    /// Build state diff for given touched addresses.
    pub fn build_diff(&self, touched_addresses: &HashSet<EthAddress>) -> anyhow::Result<StateDiff> {
        build_state_diff(
            self.store.as_ref(),
            &self.pre_state,
            &self.post_state,
            touched_addresses,
        )
    }
}
