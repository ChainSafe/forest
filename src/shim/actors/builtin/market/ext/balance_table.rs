// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::actors::market::BalanceTable;
use fvm_ipld_blockstore::Blockstore;

impl<BS: Blockstore> BalanceTableExt for BalanceTable<'_, BS> {
    fn for_each<F>(&self, mut f: F) -> anyhow::Result<()>
    where
        F: FnMut(&Address, &TokenAmount) -> anyhow::Result<()>,
    {
        match self {
            Self::V8(t) => {
                t.0.for_each(|key, escrow| f(&Address::from_bytes(&key.0)?, &escrow.into()))?
            }
            Self::V9(t) => {
                t.0.for_each(|key, escrow| f(&Address::from_bytes(&key.0)?, &escrow.into()))?
            }
            Self::V10(t) => {
                t.0.for_each(|key, escrow| f(&Address::from_bytes(&key.0)?, &escrow.into()))?
            }
            Self::V11(t) => {
                t.0.for_each(|key, escrow| f(&Address::from_bytes(&key.0)?, &escrow.into()))?
            }
            Self::V12(t) => t.0.for_each(|address, escrow| {
                f(&address.into(), &escrow.into())
                    .map_err(|e| fil_actors_shared::v12::ActorError::unspecified(e.to_string()))
            })?,
            Self::V13(t) => t.0.for_each(|address, escrow| {
                f(&address.into(), &escrow.into())
                    .map_err(|e| fil_actors_shared::v13::ActorError::unspecified(e.to_string()))
            })?,
            Self::V14(t) => t.0.for_each(|address, escrow| {
                f(&address.into(), &escrow.into())
                    .map_err(|e| fil_actors_shared::v14::ActorError::unspecified(e.to_string()))
            })?,
            Self::V15(t) => t.0.for_each(|address, escrow| {
                f(&address.into(), &escrow.into())
                    .map_err(|e| fil_actors_shared::v15::ActorError::unspecified(e.to_string()))
            })?,
            Self::V16(t) => t.0.for_each(|address, escrow| {
                f(&address.into(), &escrow.into())
                    .map_err(|e| fil_actors_shared::v16::ActorError::unspecified(e.to_string()))
            })?,
            Self::V17(t) => t.0.for_each(|address, escrow| {
                f(&address.into(), &escrow.into())
                    .map_err(|e| fil_actors_shared::v17::ActorError::unspecified(e.to_string()))
            })?,
        };
        Ok(())
    }
}
