// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::Runtime;
use vm::{InvocOutput, MethodNum, Serialized};

/// Interface for invoking methods on an Actor
pub trait ActorCode {
    /// Invokes method with runtime on the actor's code. Method number will match one
    /// defined by the Actor, and parameters will be serialized and used in execution
    fn invoke_method<RT: Runtime>(
        &self,
        rt: &RT,
        method: MethodNum,
        params: &Serialized,
    ) -> InvocOutput;
}
