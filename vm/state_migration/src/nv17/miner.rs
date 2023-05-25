// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV17` upgrade for the miner
//! actor.

use std::sync::Arc;

use cid::{multihash::Code::Blake2b256, Cid};
use fil_actor_miner_state::{v8::State as MinerStateOld, v9::State as MinerStateNew};
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;

use crate::common::{
    ActorMigration, ActorMigrationInput, ActorMigrationOutput, TypeMigration, TypeMigrator,
};

pub struct MinerMigrator {
    out_code: Cid,
    // proposals: fil_actors_shared::v8::Array<fil_actor_market_state::v8::DealProposal, BS>,
    empty_precommit_map_cid_v9: Cid,
    empty_deadline_v8_cid: Cid,
    empty_deadline_v9_cid: Cid,
    empty_deadlines_v9_cid: Cid,
}

pub(crate) fn miner_migrator<BS: Blockstore + Clone + Send + Sync>(
    out_code: Cid,
    store: &BS,
    market_proposals: fil_actors_shared::v8::Array<fil_actor_market_state::v8::DealProposal, BS>,
) -> anyhow::Result<Arc<dyn ActorMigration<BS> + Send + Sync>> {
    let mut empty_precommit_map = fil_actors_shared::v9::make_empty_map::<_, Cid>(
        store,
        fil_actors_shared::v9::builtin::HAMT_BIT_WIDTH,
    );
    let empty_precommit_map_cid_v9 = empty_precommit_map.flush()?;

    let empty_deadline_v8: fil_actor_miner_state::v8::Deadline =
        fil_actor_miner_state::v8::Deadline::new(store)?;
    let empty_deadline_v8_cid = store.put_cbor(&empty_deadline_v8, Blake2b256)?;

    let empty_deadline_v9 = fil_actor_miner_state::v9::Deadline::new(store)?;
    let empty_deadline_v9_cid = store.put_cbor(&empty_deadline_v9, Blake2b256)?;

    // FIXME: pass policy from chain config
    let policy = fil_actors_shared::v9::runtime::Policy::calibnet();
    let empty_deadlines_v9 =
        fil_actor_miner_state::v9::Deadlines::new(&policy, empty_deadline_v9_cid);
    let empty_deadlines_v9_cid = store.put_cbor(&empty_deadlines_v9, Blake2b256)?;

    Ok(Arc::new(MinerMigrator {
        out_code,
        empty_precommit_map_cid_v9,
        empty_deadline_v8_cid,
        empty_deadline_v9_cid,
        empty_deadlines_v9_cid,
    }))
}

impl<BS: Blockstore + Clone + Send + Sync> ActorMigration<BS> for MinerMigrator {
    fn migrate_state(
        &self,
        store: BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<ActorMigrationOutput> {
        let in_state: MinerStateOld = store
            .get_obj(&input.head)?
            .ok_or_else(|| anyhow::anyhow!("Init actor: could not read v9 state"))?;

        let out_state: MinerStateNew = TypeMigrator::migrate_type(in_state, &store)?;

        let new_head = store.put_obj(&out_state, Blake2b256)?;

        Ok(ActorMigrationOutput {
            new_code_cid: self.out_code,
            new_head,
        })
    }
}
