// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_ipld_blockstore::Blockstore;
use fvm_shared::address::{Address, Protocol};
use fvm_shared::METHOD_CONSTRUCTOR;
use num_derive::FromPrimitive;

use fil_actors_runtime_v8::builtin::singletons::SYSTEM_ACTOR_ADDR;
use fil_actors_runtime_v8::runtime::Runtime;
use fil_actors_runtime_v8::{actor_error, ActorError};

pub use self::state::State;

mod state;
pub mod testing;

/// Account actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    PubkeyAddress = 2,
}

/// Account Actor
pub struct Actor;
impl Actor {
    /// Constructor for Account actor
    pub fn constructor<BS, RT>(rt: &mut RT, address: Address) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&SYSTEM_ACTOR_ADDR))?;
        match address.protocol() {
            Protocol::Secp256k1 | Protocol::BLS => {}
            protocol => {
                return Err(actor_error!(illegal_argument;
                    "address must use BLS or SECP protocol, got {}", protocol));
            }
        }
        rt.create(&State { address })?;
        Ok(())
    }

    // Fetches the pubkey-type address from this actor.
    pub fn pubkey_address<BS, RT>(rt: &mut RT) -> Result<Address, ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;
        let st: State = rt.state()?;
        Ok(st.address)
    }
}
