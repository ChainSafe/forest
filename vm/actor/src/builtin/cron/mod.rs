// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;

pub use self::state::{Entry, State};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vm::{ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR, METHOD_CRON};

/// Cron actor methods available
#[derive(FromPrimitive)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    Cron = METHOD_CRON,
}

impl Method {
    /// from_method_num converts a method number into an Method enum
    fn from_method_num(m: MethodNum) -> Option<Method> {
        FromPrimitive::from_u64(u64::from(m))
    }
}

/// Constructor parameters for Cron actor, contains entries
/// of actors and methods to call on each epoch
#[derive(Default)]
pub struct ConstructorParams {
    /// Entries is a set of actors (and corresponding methods) to call during EpochTick.
    pub entries: Vec<Entry>,
}

impl Serialize for ConstructorParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.entries.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ConstructorParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entries: Vec<Entry> = Deserialize::deserialize(deserializer)?;
        Ok(Self { entries })
    }
}

/// Cron actor
pub struct Actor;
impl Actor {
    /// Constructor for Cron actor
    fn constructor<RT: Runtime>(_rt: &RT, _params: ConstructorParams) {
        // Intentionally left blank
    }
    /// epoch_tick executes built-in periodic actions, run at every Epoch.
    /// epoch_tick(r) is called after all other messages in the epoch have been applied.
    /// This can be seen as an implicit last message.
    fn epoch_tick<RT: Runtime>(&self, _rt: &RT) {
        // self.entries is basically a static registry for now, loaded
        // in the interpreter static registry.
        // TODO update to new spec
        todo!()
        // for entry in &self.entries {
        //     let res = rt.send_catching_errors(InvocInput {
        //         to: entry.to_addr.clone(),
        //         method: entry.method_num,
        //         params: Serialized::default(),
        //         value: TokenAmount::new(0),
        //     });
        //     if let Err(e) = res {
        //         return e.into();
        //     }
        // }
    }
}

impl ActorCode for Actor {
    fn invoke_method<RT: Runtime>(&self, rt: &RT, method: MethodNum, _params: &Serialized) {
        match Method::from_method_num(method) {
            Some(Method::Constructor) => {
                // TODO unfinished spec
                Self::constructor(rt, ConstructorParams::default())
            }
            Some(Method::Cron) => {
                // TODO unfinished spec
                self.epoch_tick(rt)
            }
            _ => {
                rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method".to_owned());
                unreachable!();
            }
        }
    }
}
