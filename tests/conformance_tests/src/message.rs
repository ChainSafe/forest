// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
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
    selector: &Option<Selector>,
) -> Result<(ApplyRet, Cid), Box<dyn StdError>> {
    let mut vm = VM::<_, _, _>::new(
        pre_root,
        bs,
        epoch,
        TestSyscalls,
        &TestRand,
        TokenAmount::from(BASE_FEE),
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
