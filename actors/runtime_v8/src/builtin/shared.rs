// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{actor_error, ActorContext, ActorError};
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::address::Address;
use fvm_shared::ActorID;
use fvm_shared::METHOD_SEND;

use crate::runtime::builtins::Type;
use crate::runtime::Runtime;

pub const HAMT_BIT_WIDTH: u32 = 5;

/// Types of built-in actors that can be treated as principles.
/// This distinction is legacy and should be removed prior to FVM support for
/// user-programmable actors.
pub const CALLER_TYPES_SIGNABLE: &[Type] = &[Type::Account, Type::Multisig];

/// ResolveToActorID resolves the given address to it's actor ID.
/// If an actor ID for the given address dosen't exist yet, it tries to create one by sending
/// a zero balance to the given address.
pub fn resolve_to_actor_id<BS, RT>(rt: &mut RT, address: &Address) -> Result<ActorID, ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    // if we are able to resolve it to an ID address, return the resolved address
    if let Some(id) = rt.resolve_address(address) {
        return Ok(id);
    }

    // send 0 balance to the account so an ID address for it is created and then try to resolve
    rt.send(address, METHOD_SEND, Default::default(), Default::default())
        .with_context(|| format!("failed to send zero balance to address {}", address))?;

    if let Some(id) = rt.resolve_address(address) {
        return Ok(id);
    }

    Err(actor_error!(illegal_argument, "failed to resolve or initialize address {}", address))
}
