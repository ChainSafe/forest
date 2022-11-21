// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_actors_runtime_v9::runtime::Runtime;
use fil_actors_runtime_v9::{ActorError, SYSTEM_ACTOR_ADDR};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::tuple::*;
use fvm_ipld_encoding::RawBytes;
use fvm_shared::econ::TokenAmount;

use fvm_shared::METHOD_CONSTRUCTOR;
use num_derive::FromPrimitive;
use num_traits::Zero;

pub use self::state::{Entry, State};

mod state;
pub mod testing;

// * Updated to specs-actors commit: 845089a6d2580e46055c24415a6c32ee688e5186 (v3.0.0)

/// Cron actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    EpochTick = 2,
}

/// Constructor parameters for Cron actor, contains entries
/// of actors and methods to call on each epoch
#[derive(Default, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct ConstructorParams {
    /// Entries is a set of actors (and corresponding methods) to call during EpochTick.
    pub entries: Vec<Entry>,
}

/// Cron actor
pub struct Actor;
impl Actor {
    /// Constructor for Cron actor
    fn constructor<BS, RT>(rt: &mut RT, params: ConstructorParams) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&SYSTEM_ACTOR_ADDR))?;
        rt.create(&State {
            entries: params.entries,
        })?;
        Ok(())
    }
    /// Executes built-in periodic actions, run at every Epoch.
    /// epoch_tick(r) is called after all other messages in the epoch have been applied.
    /// This can be seen as an implicit last message.
    fn epoch_tick<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&SYSTEM_ACTOR_ADDR))?;

        let st: State = rt.state()?;
        for entry in st.entries {
            // Intentionally ignore any error when calling cron methods
            let res = rt.send(
                &entry.receiver,
                entry.method_num,
                RawBytes::default(),
                TokenAmount::zero(),
            );
            if let Err(e) = res {
                log::error!(
                    "cron failed to send entry to {}, send error code {}",
                    entry.receiver,
                    e
                );
            }
        }
        Ok(())
    }
}
