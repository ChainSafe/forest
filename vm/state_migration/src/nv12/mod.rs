// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module implements network version 12 or actorv4 state migration
//! Please read https://filecoin.io/blog/posts/filecoin-network-v12/
//! to learn more about network version 12 migration.
//! This is more or less a direct port of the state migration
//! implemented in lotus' specs-actors library.

pub mod miner;

use cid::Cid;
use ipld_blockstore::BlockStore;
use clock::ChainEpoch;
use std::collections::HashMap;
use crate::{ActorMigration, MigrationJob};
use crate::MigrationErr;
use crate::nil_migrator_v4;
use miner::miner_migrator_v4;
use std::collections::HashSet;
use state_tree::StateTree;
use fil_types::StateTreeVersion;
use std::rc::Rc;
use async_std::task;
use futures::TryStreamExt;
use actor_interface::actorv3;
use actor_interface::actorv4;

pub fn migrate_state_tree<'db, BS: BlockStore>(store: &'db BS,
    actors_root_in: Cid,
    prior_epoch: ChainEpoch,
) -> Result<Cid, MigrationErr> {

    let mut jobs_future = futures::stream::FuturesOrdered::new();

    // Maps prior version code CIDs to migration functions.
    let mut migrations: HashMap<Cid, Rc<dyn ActorMigration<BS>>> = HashMap::new();
    migrations.insert(*actorv3::ACCOUNT_ACTOR_CODE_ID, nil_migrator_v4(*actorv4::ACCOUNT_ACTOR_CODE_ID));
    migrations.insert(*actorv3::CRON_ACTOR_CODE_ID, nil_migrator_v4(*actorv4::CRON_ACTOR_CODE_ID));
    migrations.insert(*actorv3::INIT_ACTOR_CODE_ID, nil_migrator_v4(*actorv4::INIT_ACTOR_CODE_ID));
    migrations.insert(*actorv3::MULTISIG_ACTOR_CODE_ID, nil_migrator_v4(*actorv4::MULTISIG_ACTOR_CODE_ID));
    migrations.insert(*actorv3::PAYCH_ACTOR_CODE_ID, nil_migrator_v4(*actorv4::PAYCH_ACTOR_CODE_ID));
    migrations.insert(*actorv3::REWARD_ACTOR_CODE_ID, nil_migrator_v4(*actorv4::REWARD_ACTOR_CODE_ID));
    migrations.insert(*actorv3::MARKET_ACTOR_CODE_ID,  nil_migrator_v4(*actorv4::MARKET_ACTOR_CODE_ID));
    migrations.insert(*actorv3::MINER_ACTOR_CODE_ID, miner_migrator_v4(*actorv4::MINER_ACTOR_CODE_ID));
    migrations.insert(*actorv3::POWER_ACTOR_CODE_ID, nil_migrator_v4(*actorv4::POWER_ACTOR_CODE_ID));
    migrations.insert(*actorv3::SYSTEM_ACTOR_CODE_ID, nil_migrator_v4(*actorv4::SYSTEM_ACTOR_CODE_ID));
    migrations.insert(*actorv3::VERIFREG_ACTOR_CODE_ID, nil_migrator_v4(*actorv4::VERIFREG_ACTOR_CODE_ID));
    // Set of prior version code CIDs for actors to defer during iteration, for explicit migration afterwards.
	let deferred_code_ids = HashSet::<Cid>::new(); // None in this migration

    if migrations.len()+deferred_code_ids.len() != 11 {
        panic!("Incomplete migration specification with {} code CIDs", migrations.len());
	}
    
    let actors_in = StateTree::new_from_root(store, &actors_root_in).unwrap();
    let actors_out = StateTree::new(store, StateTreeVersion::V3);
    
    actors_in.for_each(|addr,state| {
        if deferred_code_ids.contains(&state.code) {
            return Ok(());
        }

        let next_input = MigrationJob {
            address: addr,
            actor_state: state.clone(),
            actor_migration: migrations.get(&state.code).cloned().ok_or(MigrationErr::MigratorNotFound(state.code))?
        };

        jobs_future.push(async move {next_input.run(store, prior_epoch)});

        Ok(())
    }).map_err(MigrationErr::MigrationJobCreate)?;

    let mut actors_out = actors_out.expect("failed accesing actors_out");

    task::block_on(async {
        jobs_future.try_for_each_concurrent(None, |result| {
            match actors_out.set_actor(&result.address, result.actor_state) {
                Ok(a) => futures::future::ok(a),
                Err(e) => futures::future::err(MigrationErr::SetActorState(e))
            }
        }).await
        // while let Some(job_result) = jobs_future.next().await {
        //     let result = job_result.unwrap();
        //     actors_out.set_actor(&result.address, result.actor_state).expect("failed updating resulting actor state");
        // }
    })?;

    let root_cid = actors_out.flush().map_err(MigrationErr::FlushFailed);

    root_cid
}
