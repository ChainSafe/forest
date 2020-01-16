// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use vm::{
    ExitCode, InvocInput, InvocOutput, MethodNum, MethodParams, SysCode, TokenAmount,
    METHOD_CONSTRUCTOR, METHOD_CRON,
};

use address::Address;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{arg_end, ActorCode, Runtime};

/// CronActorState has no internal state
#[derive(Default)]
pub struct CronActorState;

#[derive(Clone)]
pub struct CronTableEntry {
    to_addr: Address,
    method_num: MethodNum,
}

#[derive(FromPrimitive)]
pub enum CronMethod {
    Constructor = METHOD_CONSTRUCTOR,
    Cron = METHOD_CRON,
}

impl CronMethod {
    /// from_method_num converts a method number into an CronMethod enum
    fn from_method_num(m: MethodNum) -> Option<CronMethod> {
        FromPrimitive::from_i32(m.into())
    }
}

#[derive(Clone)]
pub struct CronActorCode {
    /// Entries is a set of actors (and corresponding methods) to call during EpochTick.
    /// This can be done a bunch of ways. We do it this way here to make it easy to add
    /// a handler to Cron elsewhere in the spec code. How to do this is implementation
    /// specific.
    entries: Vec<CronTableEntry>,
}

impl CronActorCode {
    /// Constructor for Cron actor
    fn constructor<RT: Runtime>(rt: &RT) -> InvocOutput {
        // Intentionally left blank
        rt.success_return()
    }
    /// epoch_tick executes built-in periodic actions, run at every Epoch.
    /// epoch_tick(r) is called after all other messages in the epoch have been applied.
    /// This can be seen as an implicit last message.
    fn epoch_tick<RT: Runtime>(&self, rt: &RT) -> InvocOutput {
        // self.entries is basically a static registry for now, loaded
        // in the interpreter static registry.
        for entry in self.entries.clone() {
            let res = rt.send_catching_errors(InvocInput {
                to: entry.to_addr,
                method: entry.method_num,
                params: MethodParams::default(),
                value: TokenAmount::new(0),
            });
            if let Err(e) = res {
                return e.into();
            }
        }

        rt.success_return()
    }
}

impl ActorCode for CronActorCode {
    fn invoke_method<RT: Runtime>(
        &self,
        rt: &RT,
        method: MethodNum,
        params: &MethodParams,
    ) -> InvocOutput {
        match CronMethod::from_method_num(method) {
            Some(CronMethod::Constructor) => {
                // Assert no parameters passed
                arg_end(params, rt);
                Self::constructor(rt)
            }
            Some(CronMethod::Cron) => {
                // Assert no parameters passed
                arg_end(params, rt);
                self.epoch_tick(rt)
            }
            _ => {
                rt.abort(
                    ExitCode::SystemErrorCode(SysCode::InvalidMethod),
                    "Invalid method",
                );
                unreachable!();
            }
        }
    }
}
