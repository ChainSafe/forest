// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::Runtime;
use ipld_blockstore::BlockStore;
use vm::{ActorError, MethodNum, Serialized};

/// Interface for invoking methods on an Actor
pub trait ActorCode {
    /// Invokes method with runtime on the actor's code. Method number will match one
    /// defined by the Actor, and parameters will be serialized and used in execution
    fn invoke_method<BS, RT>(
        rt: &mut RT,
        method: MethodNum,
        params: &Serialized,
    ) -> Result<Serialized, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>;
}
