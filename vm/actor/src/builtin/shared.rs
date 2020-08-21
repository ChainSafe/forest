// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::miner::Method;
use address::Address;
use encoding::tuple::*;
use ipld_blockstore::BlockStore;
use num_traits::Zero;
use runtime::Runtime;
use vm::{ActorError, Serialized, TokenAmount};

pub(crate) fn request_miner_control_addrs<BS, RT>(
    rt: &mut RT,
    miner_addr: Address,
) -> Result<(Address, Address, Vec<Address>), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let ret = rt.send(
        miner_addr,
        Method::ControlAddresses as u64,
        Serialized::default(),
        TokenAmount::zero(),
    )?;
    let addrs: MinerAddrs = ret.deserialize()?;

    Ok((addrs.owner, addrs.worker, addrs.control_addrs))
}

#[derive(Serialize_tuple, Deserialize_tuple)]
struct MinerAddrs {
    owner: Address,
    worker: Address,
    control_addrs: Vec<Address>,
}

// ResolveToIDAddr resolves the given address to it's ID address form.
// If an ID address for the given address dosen't exist yet, it tries to create one by sending a zero balance to the given address.
// TODO ResolveToIDAddr
