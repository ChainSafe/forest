// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::RawBytes;
use fvm_shared::MethodNum;

use crate::{ActorError, Runtime};

/// Interface for invoking methods on an Actor
pub trait ActorCode {
    /// Invokes method with runtime on the actor's code. Method number will match one
    /// defined by the Actor, and parameters will be serialized and used in execution
    fn invoke_method<BS, RT>(
        rt: &mut RT,
        method: MethodNum,
        params: &RawBytes,
    ) -> Result<RawBytes, ActorError>
    where
        // TODO: remove the clone requirement on the blockstore when we fix "replica update" to not
        // hold onto state between transactions.
        // https://github.com/filecoin-project/builtin-actors/issues/133
        BS: Blockstore + Clone,
        RT: Runtime<BS>;
}
