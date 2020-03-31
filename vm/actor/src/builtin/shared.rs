// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use ipld_blockstore::BlockStore;
use runtime::Runtime;
use vm::ActorError;

pub(crate) fn request_miner_control_addrs<BS, RT>(
    _rt: &RT,
    _miner_addr: &Address,
) -> Result<(Address, Address), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    // TODO finish with miner actor
    todo!()
}
