// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use db::MemoryDB;
use interpreter::{CircSupplyCalc, LookbackStateGetter};
use state_tree::StateTree;
use std::sync::Arc;
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
    pub randomness: ReplayingRand,
    pub nv: fil_types::NetworkVersion,
}

#[derive(Clone)]
struct MockCircSupply(TokenAmount);
impl CircSupplyCalc for MockCircSupply {
    fn get_supply<DB: BlockStore>(
        &self,
        _: ChainEpoch,
        _: &StateTree<DB>,
    ) -> Result<TokenAmount, Box<dyn StdError>> {
        Ok(self.0.clone())
    }
    fn get_fil_vested<DB: BlockStore>(
        &self,
        _height: ChainEpoch,
        _store: &DB,
    ) -> Result<TokenAmount, Box<dyn StdError>> {
        Ok(0.into())
    }
}

struct MockStateLB<'db, MemoryDB>(&'db MemoryDB);
impl<'db> LookbackStateGetter<'db, MemoryDB> for MockStateLB<'db, MemoryDB> {
    fn state_lookback(&self, _: ChainEpoch) -> Result<StateTree<'db, MemoryDB>, Box<dyn StdError>> {
        Err("Lotus runner doesn't seem to initialize this?".into())
    }
}

pub fn execute_message(
    bs: Arc<MemoryDB>,
    selector: &Option<Selector>,
    params: ExecuteMessageParams,
    engine: fvm::machine::Engine,
) -> Result<(ApplyRet, Cid), Box<dyn StdError>> {
    let circ_supply = MockCircSupply(params.circ_supply.clone());
    let lb = MockStateLB(bs.as_ref());

    let nv = params.nv;
    let mut vm = VM::<_, _, _, _>::new(
        *params.pre_root,
        bs.as_ref(),
        bs.clone(),
        params.epoch,
        &params.randomness,
        params.basefee,
        nv,
        circ_supply,
        Some(params.circ_supply),
        &lb,
        engine,
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
