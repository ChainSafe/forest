// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod fvm;
#[cfg(feature = "instrumented_kernel")]
mod instrumented_kernel;
#[cfg(feature = "instrumented_kernel")]
mod metrics;
mod vm;

pub use self::vm::*;

use ::fvm::state_tree::StateTree as FvmStateTree;
use forest_actor_interface::account;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::address::{Address, Protocol};

/// returns the public key type of address (`BLS`/`SECP256K1`) of an account actor
/// identified by `addr`.
pub fn resolve_to_key_addr<BS, S>(
    st: &FvmStateTree<S>,
    store: &BS,
    addr: &Address,
) -> Result<Address, anyhow::Error>
where
    BS: Blockstore,
    S: Blockstore,
{
    if addr.protocol() == Protocol::BLS || addr.protocol() == Protocol::Secp256k1 {
        return Ok(*addr);
    }

    let act = st
        .get_actor(addr)?
        .ok_or_else(|| anyhow::anyhow!("Failed to retrieve actor: {}", addr))?;

    let acc_st = account::State::load(store, &act)?;

    Ok(acc_st.pubkey_address())
}
