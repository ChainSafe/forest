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
) -> Result<(Address, Address), ActorError>
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

    Ok((addrs.owner, addrs.worker))
}

#[derive(Serialize_tuple, Deserialize_tuple)]
struct MinerAddrs {
    owner: Address,
    worker: Address,
}
