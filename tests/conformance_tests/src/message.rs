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
    pub epoch: Option<ChainEpoch>,
}

pub fn execute_message(
    bs: &db::MemoryDB,
    msg: &ChainMessage,
    pre_root: &Cid,
    epoch: ChainEpoch,
    basefee: TokenAmount,
    selector: &Option<Selector>,
) -> Result<(ApplyRet, Cid), Box<dyn StdError>> {
    let mut vm = VM::<_, _, _, _>::new(
        pre_root,
        bs,
        epoch,
        TestSyscalls,
        &TestRand,
        basefee,
        get_network_version_default,
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

    let ret = vm.apply_message(msg)?;

    let root = vm.flush()?;
    Ok((ret, root))
}
