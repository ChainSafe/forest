// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use cid::Cid;
use paste::paste;

use super::version::*;
use fvm_ipld_blockstore::Blockstore;
use serde::de::DeserializeOwned;

fn get_obj<T>(store: &impl Blockstore, cid: &Cid) -> anyhow::Result<Option<T>>
where
    T: DeserializeOwned,
{
    match store.get(cid)? {
        Some(bz) => Ok(Some(fvm_ipld_encoding::from_slice(&bz)?)),
        None => Ok(None),
    }
}

macro_rules! actor_state_load_trait {
    ($($actor:ident),*) => {
        $(
paste! {
    pub trait [< $actor ActorStateLoad >] {
        fn load<BS: fvm_ipld_blockstore::Blockstore>(store: &BS, code: Cid, state: Cid) -> anyhow::Result<crate::shim::actors::[< $actor:lower >]::State>;
    }
}
        )*
        }
}

actor_state_load_trait!(
    System, Init, Cron, Account, Power, Miner, Market, Multisig, Reward, Verifreg, DataCap, EVM
);

// We need to provide both the version and the version identifier to the macro; it is a limitation
// of the `paste` crate.
macro_rules! actor_state_load_impl {
    ($actor:ident, $($version:literal, $version_ident:ident),*) => {
        paste! {
        impl [< $actor ActorStateLoad >] for crate::shim::actors::[< $actor:lower >]::State {
            fn load<BS: fvm_ipld_blockstore::Blockstore>(store: &BS, code: Cid, state: Cid) -> anyhow::Result<crate::shim::actors::[< $actor:lower >]::State> {
                use anyhow::Context as _;
                $(
                    if [< is_ $actor:lower _cid_version >](&code, $version) {
                        return get_obj(store, &state)?
                            .map(crate::shim::actors::[< $actor:lower >]::State::[< $version_ident >])
                            .context("Actor state doesn't exist in store");
                    }
                )*
                anyhow::bail!("Unknown actor code {}", code)
            }
        }
                }
    };
}

actor_state_load_impl!(
    Account, 8, V8, 9, V9, 10, V10, 11, V11, 12, V12, 13, V13, 14, V14, 15, V15, 16, V16
);
actor_state_load_impl!(
    Cron, 8, V8, 9, V9, 10, V10, 11, V11, 12, V12, 13, V13, 14, V14, 15, V15, 16, V16
);
actor_state_load_impl!(
    DataCap, 9, V9, 10, V10, 11, V11, 12, V12, 13, V13, 14, V14, 15, V15, 16, V16
);
actor_state_load_impl!(EVM, 10, V10, 11, V11, 12, V12, 13, V13, 14, V14, 15, V15, 16, V16);
actor_state_load_impl!(
    Init, 0, V0, 8, V8, 9, V9, 10, V10, 11, V11, 12, V12, 13, V13, 14, V14, 15, V15, 16, V16
);
actor_state_load_impl!(
    Market, 8, V8, 9, V9, 10, V10, 11, V11, 12, V12, 13, V13, 14, V14, 15, V15, 16, V16
);
actor_state_load_impl!(
    Miner, 8, V8, 9, V9, 10, V10, 11, V11, 12, V12, 13, V13, 14, V14, 15, V15, 16, V16
);
actor_state_load_impl!(
    Multisig, 8, V8, 9, V9, 10, V10, 11, V11, 12, V12, 13, V13, 14, V14, 15, V15, 16, V16
);
actor_state_load_impl!(
    Power, 8, V8, 9, V9, 10, V10, 11, V11, 12, V12, 13, V13, 14, V14, 15, V15, 16, V16
);
actor_state_load_impl!(
    System, 8, V8, 9, V9, 10, V10, 11, V11, 12, V12, 13, V13, 14, V14, 15, V15, 16, V16
);
actor_state_load_impl!(
    Verifreg, 8, V8, 9, V9, 10, V10, 11, V11, 12, V12, 13, V13, 14, V14, 15, V15, 16, V16
);
actor_state_load_impl!(
    Reward, 8, V8, 9, V9, 10, V10, 11, V11, 12, V12, 13, V13, 14, V14, 15, V15, 16, V16
);
