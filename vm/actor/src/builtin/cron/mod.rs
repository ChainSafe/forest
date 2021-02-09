// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;

pub use self::state::{Entry, State};
use crate::{check_empty_params, SYSTEM_ACTOR_ADDR};
use encoding::tuple::*;
use ipld_blockstore::BlockStore;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use vm::{
    actor_error, ActorError, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR,
};

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
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*SYSTEM_ACTOR_ADDR))?;
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
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*SYSTEM_ACTOR_ADDR))?;

        let st: State = rt.state()?;
        for entry in st.entries {
            // Intentionally ignore any error when calling cron methods
            let _ = rt.send(
                entry.receiver,
                entry.method_num,
                Serialized::default(),
                TokenAmount::from(0u8),
            );
        }
        Ok(())
    }
}

impl ActorCode for Actor {
    fn invoke_method<BS, RT>(
        rt: &mut RT,
        method: MethodNum,
        params: &Serialized,
    ) -> Result<Serialized, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        match FromPrimitive::from_u64(method) {
            Some(Method::Constructor) => {
                Self::constructor(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::EpochTick) => {
                check_empty_params(params)?;
                Self::epoch_tick(rt)?;
                Ok(Serialized::default())
            }
            None => Err(actor_error!(SysErrInvalidMethod; "Invalid method")),
        }
    }
}
