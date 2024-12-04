// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::{address::Address, state_tree::ActorState};
use fvm_ipld_blockstore::Blockstore;

pub trait LoadActorStateFromBlockstore: Sized {
    const ACTOR: Option<Address> = None;

    fn load_from_blockstore(store: &impl Blockstore, actor: &ActorState) -> anyhow::Result<Self>;
}

mod load_actor_state_trait_impl {
    use super::*;
    use crate::shim::actors::state_load::*;

    macro_rules! impl_for {
        ($actor:ident $(, $addr:expr)?) => {
            impl LoadActorStateFromBlockstore for crate::shim::actors::$actor::State {
                $(const ACTOR: Option<Address> = Some($addr);)?
                fn load_from_blockstore(store: &impl Blockstore, actor: &ActorState) -> anyhow::Result<Self> {
                    Self::load(store, actor.code, actor.state)
                }
            }
        };
    }

    impl_for!(account);
    impl_for!(cron, Address::CRON_ACTOR);
    impl_for!(datacap, Address::DATACAP_TOKEN_ACTOR);
    impl_for!(evm);
    impl_for!(init, Address::INIT_ACTOR);
    impl_for!(market, Address::MARKET_ACTOR);
    impl_for!(miner);
    impl_for!(multisig);
    impl_for!(power, Address::POWER_ACTOR);
    impl_for!(reward, Address::REWARD_ACTOR);
    impl_for!(system, Address::SYSTEM_ACTOR);
    impl_for!(verifreg, Address::VERIFIED_REGISTRY_ACTOR);
}
