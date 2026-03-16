// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! State diff computation for `trace_call` and related RPC methods.
//!
//! Compares pre- and post-execution actor states to produce per-account diffs
//! covering balance, nonce, code, and storage.

use super::super::types::{EthAddress, EthHash};
use super::super::utils::ActorStateEthExt as _;
use super::types::{AccountDiff, ChangedType, Delta, StateDiff};
use crate::rpc::eth::EthBigInt;
use crate::shim::actors::{EVMActorStateLoad as _, evm, is_evm_actor};
use crate::shim::state_tree::{ActorState, StateTree};
use ahash::{HashMap, HashSet};
use fil_actor_evm_state::evm_shared::v17::uints::U256;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_kamt::{AsHashedKey, Config as KamtConfig, HashedKey, Kamt};
use std::borrow::Cow;
use std::collections::BTreeMap;
use tracing::debug;

/// KAMT configuration matching the EVM actor in builtin-actors.
// Code is taken from: https://github.com/filecoin-project/builtin-actors/blob/v17.0.0/actors/evm/src/interpreter/system.rs#L47
fn evm_kamt_config() -> KamtConfig {
    KamtConfig {
        bit_width: 5,       // 32 children per node (2^5)
        min_data_depth: 0,  // Data can be stored at root level
        max_array_width: 1, // Max 1 key-value pair per bucket
    }
}

/// Hash algorithm for EVM storage KAMT.
// Code taken from: https://github.com/filecoin-project/builtin-actors/blob/v17.0.0/actors/evm/src/interpreter/system.rs#L49.
struct EvmStateHashAlgorithm;

impl AsHashedKey<U256, 32> for EvmStateHashAlgorithm {
    fn as_hashed_key(key: &U256) -> Cow<'_, HashedKey<32>> {
        Cow::Owned(key.to_big_endian())
    }
}

/// Type alias for EVM storage KAMT with configuration.
type EvmStorageKamt<BS> = Kamt<BS, U256, U256, EvmStateHashAlgorithm>;

fn u256_to_eth_hash(value: &U256) -> EthHash {
    EthHash(ethereum_types::H256(value.to_big_endian()))
}

const ZERO_HASH: EthHash = EthHash(ethereum_types::H256([0u8; 32]));

/// Build state diff by comparing pre and post-execution states for touched addresses.
pub(crate) fn build_state_diff<S: Blockstore, T: Blockstore>(
    store: &S,
    pre_state: &StateTree<T>,
    post_state: &StateTree<T>,
    touched_addresses: &HashSet<EthAddress>,
) -> anyhow::Result<StateDiff> {
    let mut state_diff = StateDiff::new();

    for eth_addr in touched_addresses {
        let fil_addr = eth_addr.to_filecoin_address()?;

        // Get actor state before and after
        let pre_actor = pre_state
            .get_actor(&fil_addr)
            .map_err(|e| anyhow::anyhow!("failed to get actor state: {e}"))?;

        let post_actor = post_state
            .get_actor(&fil_addr)
            .map_err(|e| anyhow::anyhow!("failed to get actor state: {e}"))?;

        let account_diff = build_account_diff(store, pre_actor.as_ref(), post_actor.as_ref())?;

        // Only include it if there were actual changes
        state_diff.insert_if_changed(*eth_addr, account_diff);
    }

    Ok(state_diff)
}

/// Build account diff by comparing pre and post actor states.
fn build_account_diff<DB: Blockstore>(
    store: &DB,
    pre_actor: Option<&ActorState>,
    post_actor: Option<&ActorState>,
) -> anyhow::Result<AccountDiff> {
    let mut diff = AccountDiff::default();

    // Compare balance
    let pre_balance = pre_actor.map(|a| EthBigInt(a.balance.atto().clone()));
    let post_balance = post_actor.map(|a| EthBigInt(a.balance.atto().clone()));
    diff.balance = Delta::from_comparison(pre_balance, post_balance);

    // Compare nonce
    let pre_nonce = pre_actor.map(|a| a.eth_nonce(store)).transpose()?;
    let post_nonce = post_actor.map(|a| a.eth_nonce(store)).transpose()?;
    diff.nonce = Delta::from_comparison(pre_nonce, post_nonce);

    // Compare code (bytecode for EVM actors)
    let pre_code = pre_actor
        .map(|a| a.eth_bytecode(store))
        .transpose()?
        .flatten();
    let post_code = post_actor
        .map(|a| a.eth_bytecode(store))
        .transpose()?
        .flatten();
    diff.code = Delta::from_comparison(pre_code, post_code);

    // Compare storage slots for EVM actors
    diff.storage = diff_evm_storage_for_actors(store, pre_actor, post_actor)?;

    Ok(diff)
}

/// Compute storage diff between pre and post actor states.
///
/// Uses different Delta types based on the scenario:
/// - Account created (None → EVM): storage slots are `Delta::Added`
/// - Account deleted (EVM → None): storage slots are `Delta::Removed`
/// - Account modified (EVM → EVM): storage slots are `Delta::Changed`
/// - Actor type changed (EVM ↔ non-EVM): treated as deletion + creation
fn diff_evm_storage_for_actors<DB: Blockstore>(
    store: &DB,
    pre_actor: Option<&ActorState>,
    post_actor: Option<&ActorState>,
) -> anyhow::Result<BTreeMap<EthHash, Delta<EthHash>>> {
    let pre_is_evm = pre_actor.is_some_and(|a| is_evm_actor(&a.code));
    let post_is_evm = post_actor.is_some_and(|a| is_evm_actor(&a.code));

    // Extract storage entries from EVM actors (empty map for non-EVM or missing actors)
    let pre_entries = extract_evm_storage_entries(store, pre_actor);
    let post_entries = extract_evm_storage_entries(store, post_actor);

    // If both are empty, no storage diff
    if pre_entries.is_empty() && post_entries.is_empty() {
        return Ok(BTreeMap::new());
    }

    let mut diff = BTreeMap::new();

    match (pre_is_evm, post_is_evm) {
        (false, true) => {
            for (key_bytes, value) in &post_entries {
                let key_hash = EthHash(ethereum_types::H256(*key_bytes));
                diff.insert(key_hash, Delta::Added(u256_to_eth_hash(value)));
            }
        }
        (true, false) => {
            for (key_bytes, value) in &pre_entries {
                let key_hash = EthHash(ethereum_types::H256(*key_bytes));
                diff.insert(key_hash, Delta::Removed(u256_to_eth_hash(value)));
            }
        }
        (true, true) => {
            for (key_bytes, pre_value) in &pre_entries {
                let key_hash = EthHash(ethereum_types::H256(*key_bytes));
                let pre_hash = u256_to_eth_hash(pre_value);

                match post_entries.get(key_bytes) {
                    Some(post_value) if pre_value != post_value => {
                        // Value changed
                        diff.insert(
                            key_hash,
                            Delta::Changed(ChangedType {
                                from: pre_hash,
                                to: u256_to_eth_hash(post_value),
                            }),
                        );
                    }
                    Some(_) => {
                        // Value unchanged, skip
                    }
                    None => {
                        // Slot cleared (value → zero)
                        diff.insert(
                            key_hash,
                            Delta::Changed(ChangedType {
                                from: pre_hash,
                                to: ZERO_HASH,
                            }),
                        );
                    }
                }
            }

            // Check for newly written entries (zero → value)
            for (key_bytes, post_value) in &post_entries {
                if !pre_entries.contains_key(key_bytes) {
                    let key_hash = EthHash(ethereum_types::H256(*key_bytes));
                    diff.insert(
                        key_hash,
                        Delta::Changed(ChangedType {
                            from: ZERO_HASH,
                            to: u256_to_eth_hash(post_value),
                        }),
                    );
                }
            }
        }
        // Neither EVM: no storage diff
        (false, false) => {}
    }

    Ok(diff)
}

/// Extract all storage entries from an EVM actor's KAMT.
/// Returns empty map if actor is None, not an EVM actor, or state cannot be loaded.
fn extract_evm_storage_entries<DB: Blockstore>(
    store: &DB,
    actor: Option<&ActorState>,
) -> HashMap<[u8; 32], U256> {
    let actor = match actor {
        Some(a) if is_evm_actor(&a.code) => a,
        _ => return HashMap::default(),
    };

    let evm_state = match evm::State::load(store, actor.code, actor.state) {
        Ok(state) => state,
        Err(e) => {
            debug!("failed to load EVM state for storage extraction: {e}");
            return HashMap::default();
        }
    };

    let storage_cid = evm_state.contract_state();
    let config = evm_kamt_config();

    let kamt: EvmStorageKamt<&DB> = match Kamt::load_with_config(&storage_cid, store, config) {
        Ok(k) => k,
        Err(e) => {
            debug!("failed to load storage KAMT: {e}");
            return HashMap::default();
        }
    };

    let mut entries = HashMap::default();
    if let Err(e) = kamt.for_each(|key, value| {
        entries.insert(key.to_big_endian(), *value);
        Ok(())
    }) {
        debug!("failed to iterate storage KAMT: {e}");
        return HashMap::default();
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::MemoryDB;
    use crate::networks::ACTOR_BUNDLES_METADATA;
    use crate::rpc::eth::EthUint64;
    use crate::rpc::eth::types::EthBytes;
    use crate::shim::address::Address as FilecoinAddress;
    use crate::shim::econ::TokenAmount;
    use crate::shim::machine::BuiltinActor;
    use crate::shim::state_tree::StateTreeVersion;
    use crate::utils::db::CborStoreExt as _;
    use ahash::HashSetExt as _;
    use cid::Cid;
    use num::BigInt;
    use std::sync::Arc;

    fn create_test_actor(balance_atto: u64, sequence: u64) -> ActorState {
        ActorState::new(
            Cid::default(), // Non-EVM actor code CID
            Cid::default(), // State CID (not used for non-EVM)
            TokenAmount::from_atto(balance_atto),
            sequence,
            None, // No delegated address
        )
    }

    fn get_evm_actor_code_cid() -> Option<Cid> {
        for bundle in ACTOR_BUNDLES_METADATA.values() {
            if bundle.actor_major_version().ok() == Some(17)
                && let Ok(cid) = bundle.manifest.get(BuiltinActor::EVM)
            {
                return Some(cid);
            }
        }
        None
    }

    fn create_evm_actor_with_bytecode(
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

    fn create_masked_id_eth_address(actor_id: u64) -> EthAddress {
        EthAddress::from_actor_id(actor_id)
    }

    struct TestStateTrees {
        store: Arc<MemoryDB>,
        pre_state: StateTree<MemoryDB>,
        post_state: StateTree<MemoryDB>,
    }

    impl TestStateTrees {
        fn new() -> anyhow::Result<Self> {
            let store = Arc::new(MemoryDB::default());
            // Use V4 which creates FvmV2 state trees that allow direct set_actor
            let pre_state = StateTree::new(store.clone(), StateTreeVersion::V5)?;
            let post_state = StateTree::new(store.clone(), StateTreeVersion::V5)?;
            Ok(Self {
                store,
                pre_state,
                post_state,
            })
        }

        /// Create state trees with different actors in pre and post.
        fn with_changed_actor(
            actor_id: u64,
            pre_actor: ActorState,
            post_actor: ActorState,
        ) -> anyhow::Result<Self> {
            let store = Arc::new(MemoryDB::default());
            let mut pre_state = StateTree::new(store.clone(), StateTreeVersion::V5)?;
            let mut post_state = StateTree::new(store.clone(), StateTreeVersion::V5)?;
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
        fn with_created_actor(actor_id: u64, post_actor: ActorState) -> anyhow::Result<Self> {
            let store = Arc::new(MemoryDB::default());
            let pre_state = StateTree::new(store.clone(), StateTreeVersion::V5)?;
            let mut post_state = StateTree::new(store.clone(), StateTreeVersion::V5)?;
            let addr = FilecoinAddress::new_id(actor_id);
            post_state.set_actor(&addr, post_actor)?;
            Ok(Self {
                store,
                pre_state,
                post_state,
            })
        }

        /// Create state trees with actor only in pre (deletion scenario).
        fn with_deleted_actor(actor_id: u64, pre_actor: ActorState) -> anyhow::Result<Self> {
            let store = Arc::new(MemoryDB::default());
            let mut pre_state = StateTree::new(store.clone(), StateTreeVersion::V5)?;
            let post_state = StateTree::new(store.clone(), StateTreeVersion::V5)?;
            let addr = FilecoinAddress::new_id(actor_id);
            pre_state.set_actor(&addr, pre_actor)?;
            Ok(Self {
                store,
                pre_state,
                post_state,
            })
        }

        /// Build state diff for given touched addresses.
        fn build_diff(&self, touched_addresses: &HashSet<EthAddress>) -> anyhow::Result<StateDiff> {
            build_state_diff(
                self.store.as_ref(),
                &self.pre_state,
                &self.post_state,
                touched_addresses,
            )
        }
    }

    #[test]
    fn test_build_state_diff_empty_touched_addresses() {
        let trees = TestStateTrees::new().unwrap();
        let touched_addresses = HashSet::new();

        let state_diff = trees.build_diff(&touched_addresses).unwrap();

        // No addresses touched = empty state diff
        assert!(state_diff.0.is_empty());
    }

    #[test]
    fn test_build_state_diff_nonexistent_address() {
        let trees = TestStateTrees::new().unwrap();
        let mut touched_addresses = HashSet::new();
        touched_addresses.insert(create_masked_id_eth_address(9999));

        let state_diff = trees.build_diff(&touched_addresses).unwrap();

        // Address doesn't exist in either state, so no diff (both None = unchanged)
        assert!(state_diff.0.is_empty());
    }

    #[test]
    fn test_build_state_diff_balance_increase() {
        let actor_id = 1001u64;
        let pre_actor = create_test_actor(1000, 5);
        let post_actor = create_test_actor(2000, 5);
        let trees = TestStateTrees::with_changed_actor(actor_id, pre_actor, post_actor).unwrap();

        let mut touched_addresses = HashSet::new();
        touched_addresses.insert(create_masked_id_eth_address(actor_id));

        let state_diff = trees.build_diff(&touched_addresses).unwrap();

        assert_eq!(state_diff.0.len(), 1);
        let eth_addr = create_masked_id_eth_address(actor_id);
        let diff = state_diff.0.get(&eth_addr).unwrap();
        match &diff.balance {
            Delta::Changed(change) => {
                assert_eq!(change.from.0, BigInt::from(1000));
                assert_eq!(change.to.0, BigInt::from(2000));
            }
            _ => panic!("Expected Delta::Changed for balance"),
        }
        assert!(diff.nonce.is_unchanged());
    }

    #[test]
    fn test_build_state_diff_balance_decrease() {
        let actor_id = 1002u64;
        let pre_actor = create_test_actor(5000, 10);
        let post_actor = create_test_actor(3000, 10);
        let trees = TestStateTrees::with_changed_actor(actor_id, pre_actor, post_actor).unwrap();

        let mut touched_addresses = HashSet::new();
        touched_addresses.insert(create_masked_id_eth_address(actor_id));

        let state_diff = trees.build_diff(&touched_addresses).unwrap();

        let eth_addr = create_masked_id_eth_address(actor_id);
        let diff = state_diff.0.get(&eth_addr).unwrap();
        match &diff.balance {
            Delta::Changed(change) => {
                assert_eq!(change.from.0, BigInt::from(5000));
                assert_eq!(change.to.0, BigInt::from(3000));
            }
            _ => panic!("Expected Delta::Changed for balance"),
        }
        assert!(diff.nonce.is_unchanged());
    }

    #[test]
    fn test_build_state_diff_nonce_increment() {
        let actor_id = 1003u64;
        let pre_actor = create_test_actor(1000, 5);
        let post_actor = create_test_actor(1000, 6);
        let trees = TestStateTrees::with_changed_actor(actor_id, pre_actor, post_actor).unwrap();

        let mut touched_addresses = HashSet::new();
        touched_addresses.insert(create_masked_id_eth_address(actor_id));

        let state_diff = trees.build_diff(&touched_addresses).unwrap();

        let eth_addr = create_masked_id_eth_address(actor_id);
        let diff = state_diff.0.get(&eth_addr).unwrap();
        assert!(diff.balance.is_unchanged());
        match &diff.nonce {
            Delta::Changed(change) => {
                assert_eq!(change.from.0, 5);
                assert_eq!(change.to.0, 6);
            }
            _ => panic!("Expected Delta::Changed for nonce"),
        }
    }

    #[test]
    fn test_build_state_diff_both_balance_and_nonce_change() {
        let actor_id = 1004u64;
        let pre_actor = create_test_actor(10000, 100);
        let post_actor = create_test_actor(9000, 101);
        let trees = TestStateTrees::with_changed_actor(actor_id, pre_actor, post_actor).unwrap();

        let mut touched_addresses = HashSet::new();
        touched_addresses.insert(create_masked_id_eth_address(actor_id));

        let state_diff = trees.build_diff(&touched_addresses).unwrap();

        let eth_addr = create_masked_id_eth_address(actor_id);
        let diff = state_diff.0.get(&eth_addr).unwrap();
        match &diff.balance {
            Delta::Changed(change) => {
                assert_eq!(change.from.0, BigInt::from(10000));
                assert_eq!(change.to.0, BigInt::from(9000));
            }
            _ => panic!("Expected Delta::Changed for balance"),
        }
        match &diff.nonce {
            Delta::Changed(change) => {
                assert_eq!(change.from.0, 100);
                assert_eq!(change.to.0, 101);
            }
            _ => panic!("Expected Delta::Changed for nonce"),
        }
    }

    #[test]
    fn test_build_state_diff_account_creation() {
        let actor_id = 1005u64;
        let post_actor = create_test_actor(5000, 0);
        let trees = TestStateTrees::with_created_actor(actor_id, post_actor).unwrap();

        let mut touched_addresses = HashSet::new();
        touched_addresses.insert(create_masked_id_eth_address(actor_id));

        let state_diff = trees.build_diff(&touched_addresses).unwrap();

        let eth_addr = create_masked_id_eth_address(actor_id);
        let diff = state_diff.0.get(&eth_addr).unwrap();
        match &diff.balance {
            Delta::Added(balance) => {
                assert_eq!(balance.0, BigInt::from(5000));
            }
            _ => panic!("Expected Delta::Added for balance"),
        }
        match &diff.nonce {
            Delta::Added(nonce) => {
                assert_eq!(nonce.0, 0);
            }
            _ => panic!("Expected Delta::Added for nonce"),
        }
    }

    #[test]
    fn test_build_state_diff_account_deletion() {
        let actor_id = 1006u64;
        let pre_actor = create_test_actor(3000, 10);
        let trees = TestStateTrees::with_deleted_actor(actor_id, pre_actor).unwrap();

        let mut touched_addresses = HashSet::new();
        touched_addresses.insert(create_masked_id_eth_address(actor_id));

        let state_diff = trees.build_diff(&touched_addresses).unwrap();

        let eth_addr = create_masked_id_eth_address(actor_id);
        let diff = state_diff.0.get(&eth_addr).unwrap();
        match &diff.balance {
            Delta::Removed(balance) => {
                assert_eq!(balance.0, BigInt::from(3000));
            }
            _ => panic!("Expected Delta::Removed for balance"),
        }
        match &diff.nonce {
            Delta::Removed(nonce) => {
                assert_eq!(nonce.0, 10);
            }
            _ => panic!("Expected Delta::Removed for nonce"),
        }
    }

    #[test]
    fn test_build_state_diff_multiple_addresses() {
        let store = Arc::new(MemoryDB::default());
        let mut pre_state = StateTree::new(store.clone(), StateTreeVersion::V5).unwrap();
        let mut post_state = StateTree::new(store.clone(), StateTreeVersion::V5).unwrap();

        // Actor 1: balance increase
        let addr1 = FilecoinAddress::new_id(2001);
        pre_state
            .set_actor(&addr1, create_test_actor(1000, 0))
            .unwrap();
        post_state
            .set_actor(&addr1, create_test_actor(2000, 0))
            .unwrap();

        // Actor 2: nonce increase
        let addr2 = FilecoinAddress::new_id(2002);
        pre_state
            .set_actor(&addr2, create_test_actor(500, 5))
            .unwrap();
        post_state
            .set_actor(&addr2, create_test_actor(500, 6))
            .unwrap();

        // Actor 3: no change (should not appear in diff)
        let addr3 = FilecoinAddress::new_id(2003);
        pre_state
            .set_actor(&addr3, create_test_actor(100, 1))
            .unwrap();
        post_state
            .set_actor(&addr3, create_test_actor(100, 1))
            .unwrap();

        let mut touched_addresses = HashSet::new();
        touched_addresses.insert(create_masked_id_eth_address(2001));
        touched_addresses.insert(create_masked_id_eth_address(2002));
        touched_addresses.insert(create_masked_id_eth_address(2003));

        let state_diff =
            build_state_diff(store.as_ref(), &pre_state, &post_state, &touched_addresses).unwrap();

        assert_eq!(state_diff.0.len(), 2);
        assert!(
            state_diff
                .0
                .contains_key(&create_masked_id_eth_address(2001))
        );
        assert!(
            state_diff
                .0
                .contains_key(&create_masked_id_eth_address(2002))
        );
        assert!(
            !state_diff
                .0
                .contains_key(&create_masked_id_eth_address(2003))
        );
    }

    #[test]
    fn test_build_state_diff_evm_actor_scenarios() {
        struct TestCase {
            name: &'static str,
            pre: Option<(u64, u64, Option<&'static [u8]>)>, // balance, nonce, bytecode
            post: Option<(u64, u64, Option<&'static [u8]>)>,
            expected_balance: Delta<EthBigInt>,
            expected_nonce: Delta<EthUint64>,
            expected_code: Delta<EthBytes>,
        }

        let bytecode1: &[u8] = &[0x60, 0x80, 0x60, 0x40, 0x52];
        let bytecode2: &[u8] = &[0x60, 0x80, 0x60, 0x40, 0x52, 0x00];

        let cases = vec![
            TestCase {
                name: "No change",
                pre: Some((1000, 5, Some(bytecode1))),
                post: Some((1000, 5, Some(bytecode1))),
                expected_balance: Delta::Unchanged,
                expected_nonce: Delta::Unchanged,
                expected_code: Delta::Unchanged,
            },
            TestCase {
                name: "Balance increase",
                pre: Some((1000, 5, Some(bytecode1))),
                post: Some((2000, 5, Some(bytecode1))),
                expected_balance: Delta::Changed(ChangedType {
                    from: EthBigInt(BigInt::from(1000)),
                    to: EthBigInt(BigInt::from(2000)),
                }),
                expected_nonce: Delta::Unchanged,
                expected_code: Delta::Unchanged,
            },
            TestCase {
                name: "Nonce increment",
                pre: Some((1000, 5, Some(bytecode1))),
                post: Some((1000, 6, Some(bytecode1))),
                expected_balance: Delta::Unchanged,
                expected_nonce: Delta::Changed(ChangedType {
                    from: EthUint64(5),
                    to: EthUint64(6),
                }),
                expected_code: Delta::Unchanged,
            },
            TestCase {
                name: "Bytecode change",
                pre: Some((1000, 5, Some(bytecode1))),
                post: Some((1000, 5, Some(bytecode2))),
                expected_balance: Delta::Unchanged,
                expected_nonce: Delta::Unchanged,
                expected_code: Delta::Changed(ChangedType {
                    from: EthBytes(bytecode1.to_vec()),
                    to: EthBytes(bytecode2.to_vec()),
                }),
            },
            TestCase {
                name: "Balance and Nonce change",
                pre: Some((1000, 5, Some(bytecode1))),
                post: Some((2000, 6, Some(bytecode1))),
                expected_balance: Delta::Changed(ChangedType {
                    from: EthBigInt(BigInt::from(1000)),
                    to: EthBigInt(BigInt::from(2000)),
                }),
                expected_nonce: Delta::Changed(ChangedType {
                    from: EthUint64(5),
                    to: EthUint64(6),
                }),
                expected_code: Delta::Unchanged,
            },
            TestCase {
                name: "Creation",
                pre: None,
                post: Some((5000, 0, Some(bytecode1))),
                expected_balance: Delta::Added(EthBigInt(BigInt::from(5000))),
                expected_nonce: Delta::Added(EthUint64(0)),
                expected_code: Delta::Added(EthBytes(bytecode1.to_vec())),
            },
            TestCase {
                name: "Deletion",
                pre: Some((3000, 10, Some(bytecode1))),
                post: None,
                expected_balance: Delta::Removed(EthBigInt(BigInt::from(3000))),
                expected_nonce: Delta::Removed(EthUint64(10)),
                expected_code: Delta::Removed(EthBytes(bytecode1.to_vec())),
            },
        ];

        for case in cases {
            let store = Arc::new(MemoryDB::default());
            let actor_id = 10000u64; // arbitrary ID

            let pre_actor = case.pre.and_then(|(bal, nonce, code)| {
                create_evm_actor_with_bytecode(&store, bal, 0, nonce, code)
            });
            let post_actor = case.post.and_then(|(bal, nonce, code)| {
                create_evm_actor_with_bytecode(&store, bal, 0, nonce, code)
            });

            let mut pre_state = StateTree::new(store.clone(), StateTreeVersion::V5).unwrap();
            let mut post_state = StateTree::new(store.clone(), StateTreeVersion::V5).unwrap();
            let addr = FilecoinAddress::new_id(actor_id);

            if let Some(actor) = pre_actor {
                pre_state.set_actor(&addr, actor).unwrap();
            }
            if let Some(actor) = post_actor {
                post_state.set_actor(&addr, actor).unwrap();
            }

            let mut touched_addresses = HashSet::new();
            touched_addresses.insert(create_masked_id_eth_address(actor_id));

            let state_diff =
                build_state_diff(store.as_ref(), &pre_state, &post_state, &touched_addresses)
                    .unwrap();

            if case.expected_balance == Delta::Unchanged
                && case.expected_nonce == Delta::Unchanged
                && case.expected_code == Delta::Unchanged
            {
                assert!(
                    state_diff.0.is_empty(),
                    "Test case '{}' failed: expected empty diff",
                    case.name
                );
            } else {
                let eth_addr = create_masked_id_eth_address(actor_id);
                let diff = state_diff.0.get(&eth_addr).unwrap_or_else(|| {
                    panic!("Test case '{}' failed: missing diff entry", case.name)
                });

                assert_eq!(
                    diff.balance, case.expected_balance,
                    "Test case '{}' failed: balance mismatch",
                    case.name
                );
                assert_eq!(
                    diff.nonce, case.expected_nonce,
                    "Test case '{}' failed: nonce mismatch",
                    case.name
                );
                assert_eq!(
                    diff.code, case.expected_code,
                    "Test case '{}' failed: code mismatch",
                    case.name
                );
            }
        }
    }

    #[test]
    fn test_build_state_diff_non_evm_actor_no_code() {
        // Non-EVM actors should have no code in their diff
        let actor_id = 4005u64;
        let pre_actor = create_test_actor(1000, 5);
        let post_actor = create_test_actor(2000, 6);
        let trees = TestStateTrees::with_changed_actor(actor_id, pre_actor, post_actor).unwrap();

        let mut touched_addresses = HashSet::new();
        touched_addresses.insert(create_masked_id_eth_address(actor_id));

        let state_diff = trees.build_diff(&touched_addresses).unwrap();

        let eth_addr = create_masked_id_eth_address(actor_id);
        let diff = state_diff.0.get(&eth_addr).unwrap();

        // Balance and nonce should change
        assert!(!diff.balance.is_unchanged());
        assert!(!diff.nonce.is_unchanged());

        // Code should be unchanged (None -> None for non-EVM actors)
        assert!(diff.code.is_unchanged());
    }

    #[test]
    fn test_actor_nonce_non_evm() {
        let store = MemoryDB::default();
        let actor = create_test_actor(1000, 42);
        let nonce = actor.eth_nonce(&store).unwrap();
        assert_eq!(nonce.0, 42);
    }

    #[test]
    fn test_actor_nonce_evm() {
        let store = Arc::new(MemoryDB::default());
        let actor = create_evm_actor_with_bytecode(&store, 1000, 0, 7, Some(&[0x60]))
            .expect("failed to create EVM actor fixture");
        let nonce = actor.eth_nonce(store.as_ref()).unwrap();
        // EVM actors use the EVM nonce field, not the actor sequence
        assert_eq!(nonce.0, 7);
    }

    #[test]
    fn test_actor_bytecode_non_evm() {
        let store = MemoryDB::default();
        let actor = create_test_actor(1000, 0);
        assert!(actor.eth_bytecode(&store).unwrap().is_none());
    }

    #[test]
    fn test_actor_bytecode_evm() {
        let store = Arc::new(MemoryDB::default());
        let bytecode = &[0x60, 0x80, 0x60, 0x40, 0x52];
        let actor = create_evm_actor_with_bytecode(&store, 1000, 0, 1, Some(bytecode))
            .expect("failed to create EVM actor fixture");
        let result = actor.eth_bytecode(store.as_ref()).unwrap();
        assert_eq!(result, Some(EthBytes(bytecode.to_vec())));
    }

    #[test]
    fn test_actor_bytecode_evm_no_bytecode() {
        let store = Arc::new(MemoryDB::default());
        let actor = create_evm_actor_with_bytecode(&store, 1000, 0, 1, None)
            .expect("failed to create EVM actor fixture");
        // No bytecode stored => None (Cid::default() won't resolve to raw data)
        let result = actor.eth_bytecode(store.as_ref()).unwrap();
        assert!(result.is_none());
    }
}
