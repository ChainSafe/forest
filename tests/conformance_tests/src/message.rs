// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use fil_types::get_network_version_default;
use vm::TokenAmount;

#[derive(Debug, Deserialize)]
pub struct MessageVector {
    #[serde(with = "base64_bytes")]
    pub bytes: Vec<u8>,
    #[serde(default)]
    pub epoch_offset: Option<ChainEpoch>,
}

pub struct ExecuteMessageParams<'a> {
    pub pre_root: &'a Cid,
    pub epoch: ChainEpoch,
    pub msg: &'a ChainMessage,
    pub circ_supply: TokenAmount,
    pub basefee: TokenAmount,
    pub randomness: ReplayingRand<'a>,
}

pub fn execute_message(
    bs: &db::MemoryDB,
    selector: &Option<Selector>,
    params: ExecuteMessageParams,
) -> Result<(ApplyRet, Cid), Box<dyn StdError>> {
    let circ_supply = params.circ_supply;
    let mut vm = VM::<_, _, _>::new(
        params.pre_root,
        bs,
        params.epoch,
        &params.randomness,
        params.basefee,
        get_network_version_default,
        Some(Box::new(move |_, _| Ok(circ_supply.clone()))),
    )?;

    if let Some(s) = &selector {
        if s.chaos_actor
            .as_ref()
            .map(|s| s == "true")
            .unwrap_or_default()
        {
            vm.register_actor(CHAOS_ACTOR_CODE_ID.clone());
        }
    }

    let ret = vm.apply_message(params.msg)?;

    let root = vm.flush()?;
    Ok((ret, root))
}
