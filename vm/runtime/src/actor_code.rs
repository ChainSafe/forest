// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use crate::Runtime;
use vm::{InvocOutput, MethodNum, Serialized};

pub trait ActorCode {
    /// Invokes method with runtime on the actor's code
    fn invoke_method<RT: Runtime>(
        &self,
        rt: &RT,
        method: MethodNum,
        params: &Serialized,
    ) -> InvocOutput;
}
