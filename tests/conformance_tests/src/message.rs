// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use db::MemoryDB;
use interpreter::{CircSupplyCalc, LookbackStateGetter};
use state_tree::StateTree;
use vm::TokenAmount;
use std::sync::Arc;

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
    pub randomness: ReplayingRand,
    pub nv: fil_types::NetworkVersion,
}

struct MockCircSupply(TokenAmount);
impl CircSupplyCalc for MockCircSupply {
    fn get_supply<DB: BlockStore>(
        &self,
        _: ChainEpoch,
        _: &StateTree<DB>,
    ) -> Result<TokenAmount, Box<dyn StdError>> {
        Ok(self.0.clone())
    }
}

struct MockStateLB();
impl<DB> LookbackStateGetter<DB> for MockStateLB {
    fn state_lookback(&self, _: ChainEpoch) -> Result<StateTree<'_, DB>, Box<dyn StdError>> {
        Err("Lotus runner doesn't seem to initialize this?".into())
    }
}

pub fn execute_message(
    bs: Arc<MemoryDB>,
    selector: &Option<Selector>,
    params: ExecuteMessageParams,
) -> Result<(ApplyRet, Cid), Box<dyn StdError>> {
    let circ_supply = MockCircSupply(params.circ_supply);
    let lb = MockStateLB();

    let nv = params.nv;
    let mut vm = VM::<_, _, _, _>::new(
        params.pre_root,
        bs.as_ref(),
        bs.clone(),
        params.epoch,
        params.randomness,
        params.basefee,
        move |_| nv,
        circ_supply,
        lb,
    )?;

    if let Some(s) = &selector {
        if s.chaos_actor
            .as_ref()
            .map(|s| s == "true")
            .unwrap_or_default()
        {
            vm.register_actor(*CHAOS_ACTOR_CODE_ID);
        }
    }

    let ret = vm.apply_message(params.msg)?;

    let root = vm.flush()?;
    Ok((ret, root))
}
