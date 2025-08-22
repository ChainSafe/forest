// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::utils::multihash::prelude::*;
use cid::Cid;
use fil_actors_shared::v11::runtime::builtins::Type;
use paste::paste;
use std::sync::LazyLock;

macro_rules! impl_actor_cids_type_actor {
    ($($actor_type:ident, $actor:ident),*) => {
        $(
paste! {
static [<$actor:upper _ACTOR_CIDS>]: LazyLock<Vec<(u64, Cid)>> = LazyLock::new(|| {
    let mut actors: Vec<_> = crate::networks::ACTOR_BUNDLES_METADATA
        .values()
        .filter_map(|bundle| {
            if let Ok(cid) = bundle.manifest.get(Type::$actor_type) {
                Some((bundle.actor_major_version().ok()?, cid))
            } else {
                None
            }
        })
        .collect();

    // we need to add manually init actors for V0.
    if Type::$actor_type == Type::Init {
        let init = Cid::new_v1(fvm_ipld_encoding::IPLD_RAW, MultihashCode::Identity.digest(b"fil/1/init"));
        actors.push((0, init));
    }
    actors

});

#[allow(unused)]
/// Checks if the provided actor code CID is valid for the given actor (any version).
pub fn [<is_ $actor:lower _actor>](actor_code_cid: &Cid) -> bool {
    [<$actor:upper _ACTOR_CIDS>]
        .iter()
        .any(|(_, cid)| cid == actor_code_cid)
}

#[allow(unused)]
/// Checks if the provided actor code CID and version are valid for the given actor.
pub fn [<is_ $actor:lower _cid_version>](actor_code_cid: &Cid, version: u64) -> bool {
    [<$actor:upper _ACTOR_CIDS>]
        .iter()
        .any(|(v, cid)| *v == version && cid == actor_code_cid)
}
}
        )*
    };
}

macro_rules! impl_actor_cids {
    ($($actor:ident),*) => {
        $(
            impl_actor_cids_type_actor!($actor, $actor);
        )*
    };
}

impl_actor_cids!(
    System,
    Init,
    Cron,
    Account,
    Power,
    Miner,
    Market,
    PaymentChannel,
    Multisig,
    Reward,
    DataCap,
    Placeholder,
    EVM,
    EAM,
    EthAccount
);

// A special snowflake which has a slightly different type and package name.
impl_actor_cids_type_actor!(VerifiedRegistry, Verifreg);
