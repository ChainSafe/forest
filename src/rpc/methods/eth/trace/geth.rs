// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Geth-style trace construction from Filecoin execution traces.
//!
//! Builds nested [`GethCallFrame`] trees and [`PreStateFrame`] snapshots
//! from FVM [`ExecutionTrace`] data.

use super::super::EthBigInt;
use super::super::types::{EthAddress, EthHash};
use super::super::utils::ActorStateEthExt;
use super::Environment;
use super::state_diff::{diff_entry_keys, extract_evm_storage_entries};
use super::types::{
    CallTracerConfig, DiffMode, GethCallFrame, GethCallType, PreStateFrame, PreStateMode,
    TraceAction,
};
use super::utils::{ZERO_HASH, trace_to_address, u256_to_eth_hash};
use crate::eth::EAMMethod;
use crate::rpc::state::ExecutionTrace;
use crate::shim::actors::is_evm_actor;
use crate::shim::address::Address;
use crate::shim::state_tree::{ActorState, StateTree};
use ahash::{HashMap, HashSet};
use fil_actor_evm_state::evm_shared::v17::uints::U256;
use fvm_ipld_blockstore::Blockstore;
use num::FromPrimitive as _;
use std::collections::BTreeMap;

/// Error string used in Geth-format traces.
pub const GETH_TRACE_REVERT_ERROR: &str = "execution reverted";

/// Builds a Geth-style nested call frame tree from a Filecoin execution trace.
///
/// Reuses [`build_trace`] for classification and data extraction, then converts
/// the Parity-style [`EthTrace`] into a nested [`GethCallFrame`].
pub fn build_geth_call_frame(
    env: &mut Environment,
    trace: ExecutionTrace,
    tracer_cfg: &CallTracerConfig,
) -> anyhow::Result<Option<GethCallFrame>> {
    build_geth_frame_recursive(env, trace, tracer_cfg, true)
}

fn build_geth_frame_recursive(
    env: &mut Environment,
    trace: ExecutionTrace,
    tracer_cfg: &CallTracerConfig,
    is_root: bool,
) -> anyhow::Result<Option<GethCallFrame>> {
    let msg_to = trace.msg.to;
    let msg_method = trace.msg.method;

    // Reuse build_trace for all classification logic (EVM call, create, delegatecall, etc.).
    // Pass an empty address for root (skips the insufficient-funds early-return) and a
    // non-empty placeholder for subcalls (enables it).
    let address: &[i64] = if is_root { &[] } else { &[0] };
    let (eth_trace, recurse_into) = super::parity::build_trace(env, address, trace)?;

    let eth_trace = match eth_trace {
        Some(t) => t,
        None => return Ok(None),
    };

    let call_type = match &eth_trace.action {
        TraceAction::Call(action) => GethCallType::from_parity_call_type(&action.call_type),
        TraceAction::Create(_) => {
            if msg_to == Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR
                && matches!(EAMMethod::from_u64(msg_method), Some(EAMMethod::Create2))
            {
                GethCallType::Create2
            } else {
                GethCallType::Create
            }
        }
    };

    let mut frame = eth_trace.into_geth_frame(call_type)?;
    if !tracer_cfg.only_top_call.unwrap_or_default()
        && let Some(recurse_trace) = recurse_into
        && let Some(invoked_actor) = &recurse_trace.invoked_actor
    {
        let mut sub_env = Environment {
            caller: trace_to_address(invoked_actor),
            is_evm: is_evm_actor(&invoked_actor.state.code),
            ..Environment::default()
        };

        let mut subcalls = Vec::new();
        for subcall in recurse_trace.subcalls {
            if let Some(f) = build_geth_frame_recursive(&mut sub_env, subcall, tracer_cfg, false)? {
                subcalls.push(f);
            }
        }

        if !subcalls.is_empty() {
            frame.calls = Some(subcalls);
        }
    }

    Ok(Some(frame))
}

/// Build an [`AccountState`] snapshot from an actor.
/// Returns `None` when the actor does not exist.
///
/// When `storage_filter` is provided, only storage keys in the filter set are
/// included. This limits output to slots that actually changed between pre and
/// post state.
fn build_account_snapshot_from_entries<DB: Blockstore>(
    store: &DB,
    actor: Option<&ActorState>,
    config: &super::types::PreStateConfig,
    entries: &HashMap<[u8; 32], U256>,
    storage_filter: Option<&HashSet<[u8; 32]>>,
) -> Option<super::types::AccountState> {
    let actor = actor?;

    let nonce = actor.eth_nonce(store).ok();
    let code = if config.is_code_disabled() {
        None
    } else {
        actor.eth_bytecode(store).ok().flatten()
    };
    let storage = if config.is_storage_disabled() {
        BTreeMap::new()
    } else {
        entries
            .iter()
            .filter(|(k, _)| storage_filter.is_none_or(|f| f.contains(*k)))
            .map(|(k, v)| (EthHash(ethereum_types::H256(*k)), u256_to_eth_hash(v)))
            .collect()
    };

    Some(super::types::AccountState {
        balance: Some(EthBigInt(actor.balance.atto().clone())),
        code,
        nonce,
        storage,
    })
}

/// Build a [`PreStateFrame`] for the `prestateTracer`.
///
/// In default mode, returns the pre-execution state of every touched account.
/// Only storage slots that changed between pre and post state are included.
///
/// In diff mode, returns separate `pre` and `post` snapshots with unchanged
/// fields and accounts stripped.
pub fn build_prestate_frame<S: Blockstore, T: Blockstore>(
    store: &S,
    pre_state: &StateTree<T>,
    post_state: &StateTree<T>,
    touched_addresses: &HashSet<EthAddress>,
    config: &super::types::PreStateConfig,
) -> anyhow::Result<PreStateFrame> {
    if config.is_diff_mode() {
        let mut pre_map = BTreeMap::new();
        let mut post_map = BTreeMap::new();
        let mut deleted_addrs = HashSet::default();

        for eth_addr in touched_addresses {
            let fil_addr = eth_addr.to_filecoin_address()?;
            let pre_actor = pre_state.get_actor(&fil_addr)?;
            let post_actor = post_state.get_actor(&fil_addr)?;

            if pre_actor.is_some() && post_actor.is_none() {
                deleted_addrs.insert(*eth_addr);
            }

            let pre_entries = extract_evm_storage_entries(store, pre_actor.as_ref());
            let post_entries = extract_evm_storage_entries(store, post_actor.as_ref());
            let changed_keys = diff_entry_keys(&pre_entries, &post_entries);

            let pre_snap = build_account_snapshot_from_entries(
                store,
                pre_actor.as_ref(),
                config,
                &pre_entries,
                Some(&changed_keys),
            );
            let post_snap = build_account_snapshot_from_entries(
                store,
                post_actor.as_ref(),
                config,
                &post_entries,
                Some(&changed_keys),
            );

            // Created accounts (pre=None) only appear in post.
            if let Some(ref snap) = pre_snap {
                pre_map.insert(*eth_addr, snap.clone());
            }

            // Deleted accounts (post=None) only appear in pre.
            // For modified accounts, strip unchanged fields from the post snapshot.
            if let Some(mut snap) = post_snap {
                // Strip zero-valued storage entries from post.
                snap.storage.retain(|_, v| *v != ZERO_HASH);
                if let Some(ref pre) = pre_snap {
                    snap.retain_changed(pre);
                }
                if !snap.is_empty() {
                    post_map.insert(*eth_addr, snap);
                }
            }
        }

        // Remove fully unchanged accounts: keep only those with changes
        // (in post_map) or that were deleted.
        pre_map.retain(|addr, _| post_map.contains_key(addr) || deleted_addrs.contains(addr));

        Ok(PreStateFrame::Diff(DiffMode {
            pre: pre_map,
            post: post_map,
        }))
    } else {
        let mut result = BTreeMap::new();

        for eth_addr in touched_addresses {
            let fil_addr = eth_addr.to_filecoin_address()?;
            let pre_actor = pre_state.get_actor(&fil_addr)?;
            let post_actor = post_state.get_actor(&fil_addr)?;

            // Extract storage once per actor and derive both changed keys and
            // the snapshot from the cached entries.
            let pre_entries = extract_evm_storage_entries(store, pre_actor.as_ref());
            let post_entries = extract_evm_storage_entries(store, post_actor.as_ref());
            let changed_keys = diff_entry_keys(&pre_entries, &post_entries);

            if let Some(snap) = build_account_snapshot_from_entries(
                store,
                pre_actor.as_ref(),
                config,
                &pre_entries,
                Some(&changed_keys),
            ) {
                result.insert(*eth_addr, snap);
            }
        }

        Ok(PreStateFrame::Default(PreStateMode(result)))
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_helpers::*;
    use super::super::types::{PreStateConfig, PreStateFrame};
    use super::*;
    use crate::rpc::eth::{EthBigInt, EthUint64};
    use ahash::HashSetExt as _;
    use num::BigInt;

    #[test]
    fn test_build_prestate_frame_default_mode_empty() {
        let trees = TestStateTrees::new().unwrap();
        let config = PreStateConfig::default();
        let touched = HashSet::new();

        let frame = build_prestate_frame(
            trees.store.as_ref(),
            &trees.pre_state,
            &trees.post_state,
            &touched,
            &config,
        )
        .unwrap();

        match frame {
            PreStateFrame::Default(mode) => {
                assert!(mode.0.is_empty());
            }
            _ => panic!("Expected Default mode"),
        }
    }

    #[test]
    fn test_build_prestate_frame_default_mode_with_actor() {
        let actor_id = 5001u64;
        let pre_actor = create_test_actor(1000, 5);
        let post_actor = create_test_actor(2000, 6);
        let trees = TestStateTrees::with_changed_actor(actor_id, pre_actor, post_actor).unwrap();

        let config = PreStateConfig::default();
        let mut touched = HashSet::new();
        let actor_addr = create_masked_id_eth_address(actor_id);
        touched.insert(actor_addr);

        let frame = build_prestate_frame(
            trees.store.as_ref(),
            &trees.pre_state,
            &trees.post_state,
            &touched,
            &config,
        )
        .unwrap();

        match frame {
            PreStateFrame::Default(mode) => {
                assert_eq!(mode.0.len(), 1);
                let snap = mode.0.get(&actor_addr).unwrap();
                // Default mode returns pre-state
                assert_eq!(snap.balance, Some(EthBigInt(BigInt::from(1000))));
                assert_eq!(snap.nonce, Some(EthUint64(5)));
            }
            _ => panic!("Expected Default mode"),
        }
    }

    #[test]
    fn test_build_prestate_frame_diff_mode() {
        let actor_id = 5002u64;
        let pre_actor = create_test_actor(1000, 5);
        let post_actor = create_test_actor(2000, 6);
        let trees = TestStateTrees::with_changed_actor(actor_id, pre_actor, post_actor).unwrap();

        let config = PreStateConfig {
            diff_mode: Some(true),
            ..PreStateConfig::default()
        };
        let mut touched = HashSet::new();
        touched.insert(create_masked_id_eth_address(actor_id));

        let frame = build_prestate_frame(
            trees.store.as_ref(),
            &trees.pre_state,
            &trees.post_state,
            &touched,
            &config,
        )
        .unwrap();

        match frame {
            PreStateFrame::Diff(diff) => {
                let addr = create_masked_id_eth_address(actor_id);

                // Pre should contain the original state
                let pre_snap = diff.pre.get(&addr).unwrap();
                assert_eq!(pre_snap.balance, Some(EthBigInt(BigInt::from(1000))));
                assert_eq!(pre_snap.nonce, Some(EthUint64(5)));

                // Post should only contain changed fields
                let post_snap = diff.post.get(&addr).unwrap();
                assert_eq!(post_snap.balance, Some(EthBigInt(BigInt::from(2000))));
                assert_eq!(post_snap.nonce, Some(EthUint64(6)));
            }
            _ => panic!("Expected Diff mode"),
        }
    }

    #[test]
    fn test_build_prestate_frame_diff_mode_unchanged_actor_excluded() {
        let actor_id = 5003u64;
        let actor = create_test_actor(1000, 5);
        let trees =
            TestStateTrees::with_changed_actor(actor_id, actor.clone(), actor.clone()).unwrap();

        let config = PreStateConfig {
            diff_mode: Some(true),
            ..PreStateConfig::default()
        };
        let mut touched = HashSet::new();
        touched.insert(create_masked_id_eth_address(actor_id));

        let frame = build_prestate_frame(
            trees.store.as_ref(),
            &trees.pre_state,
            &trees.post_state,
            &touched,
            &config,
        )
        .unwrap();

        match frame {
            PreStateFrame::Diff(diff) => {
                // Unchanged actors should not appear in either pre or post
                assert!(diff.pre.is_empty());
                assert!(diff.post.is_empty());
            }
            _ => panic!("Expected Diff mode"),
        }
    }

    #[test]
    fn test_build_prestate_frame_diff_mode_created_actor() {
        let actor_id = 5004u64;
        let post_actor = create_test_actor(5000, 0);
        let trees = TestStateTrees::with_created_actor(actor_id, post_actor).unwrap();

        let config = PreStateConfig {
            diff_mode: Some(true),
            ..PreStateConfig::default()
        };
        let mut touched = HashSet::new();
        touched.insert(create_masked_id_eth_address(actor_id));

        let frame = build_prestate_frame(
            trees.store.as_ref(),
            &trees.pre_state,
            &trees.post_state,
            &touched,
            &config,
        )
        .unwrap();

        match frame {
            PreStateFrame::Diff(diff) => {
                let addr = create_masked_id_eth_address(actor_id);
                // Created accounts should only appear in post
                assert!(!diff.pre.contains_key(&addr));
                assert!(diff.post.contains_key(&addr));
            }
            _ => panic!("Expected Diff mode"),
        }
    }

    #[test]
    fn test_build_prestate_frame_diff_mode_deleted_actor() {
        let actor_id = 5005u64;
        let pre_actor = create_test_actor(3000, 10);
        let trees = TestStateTrees::with_deleted_actor(actor_id, pre_actor).unwrap();

        let config = PreStateConfig {
            diff_mode: Some(true),
            ..PreStateConfig::default()
        };
        let mut touched = HashSet::new();
        touched.insert(create_masked_id_eth_address(actor_id));

        let frame = build_prestate_frame(
            trees.store.as_ref(),
            &trees.pre_state,
            &trees.post_state,
            &touched,
            &config,
        )
        .unwrap();

        match frame {
            PreStateFrame::Diff(diff) => {
                let addr = create_masked_id_eth_address(actor_id);
                // Deleted accounts should only appear in pre
                assert!(diff.pre.contains_key(&addr));
                assert!(!diff.post.contains_key(&addr));
            }
            _ => panic!("Expected Diff mode"),
        }
    }
}
