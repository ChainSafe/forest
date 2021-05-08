
use address::Address;
use cid::Cid;
use ipld_blockstore::BlockStore;
use clock::ChainEpoch;
use std::collections::HashMap;
use crate::{ActorMigration, CachedMigrator, MigrationJob};
use crate::Config;
use crate::MigrationCache;
use crate::MigrationErr;
use crate::NilMigrator;
use std::collections::HashSet;
use state_tree::StateTree;
use fil_types::StateTreeVersion;
use std::rc::Rc;
use async_std::task;
use futures::StreamExt;

mod util;
mod miner;

fn actor_head_key(addr: Address, head: Cid) -> String {
	format!("{}-h-{}", addr, head)
}

fn nil_migrator_v3<BS: BlockStore>(cid: Cid) -> Rc<dyn ActorMigration<BS>> {
    Rc::new(NilMigrator(cid))
}

fn migrate_state_tree<BS: BlockStore + Clone>(store: BS,
    actors_root_in: Cid,
    prior_epoch: ChainEpoch,
    cfg: Config,
    cache: Rc<dyn MigrationCache>) -> Result<Cid, MigrationErr> {

    let mut jobs_future = futures::stream::FuturesOrdered::new();

    if cfg.max_workers <= 0 {
        todo!();
    }

    // Maps prior version code CIDs to migration functions.
    let mut migrations: HashMap<Cid, Rc<dyn ActorMigration<BS>>> = HashMap::new();
    migrations.insert(*actorv2::ACCOUNT_ACTOR_CODE_ID, nil_migrator_v3(*actorv3::ACCOUNT_ACTOR_CODE_ID));
    migrations.insert(*actorv2::CRON_ACTOR_CODE_ID, nil_migrator_v3(*actorv3::CRON_ACTOR_CODE_ID));
    migrations.insert(*actorv2::INIT_ACTOR_CODE_ID, nil_migrator_v3(*actorv3::INIT_ACTOR_CODE_ID));
    migrations.insert(*actorv2::INIT_ACTOR_CODE_ID, nil_migrator_v3(*actorv3::INIT_ACTOR_CODE_ID));
    migrations.insert(*actorv2::MULTISIG_ACTOR_CODE_ID, nil_migrator_v3(*actorv3::MULTISIG_ACTOR_CODE_ID));
    migrations.insert(*actorv2::PAYCH_ACTOR_CODE_ID, nil_migrator_v3(*actorv3::PAYCH_ACTOR_CODE_ID));
    migrations.insert(*actorv2::REWARD_ACTOR_CODE_ID, nil_migrator_v3(*actorv3::REWARD_ACTOR_CODE_ID));
    migrations.insert(*actorv2::MARKET_ACTOR_CODE_ID,  nil_migrator_v3(*actorv3::MARKET_ACTOR_CODE_ID));
    // TODO: not using cache migrator as of now
    migrations.insert(*actorv2::MINER_ACTOR_CODE_ID, Rc::new(miner::MinerMigrator));
    migrations.insert(*actorv2::POWER_ACTOR_CODE_ID, nil_migrator_v3(*actorv3::POWER_ACTOR_CODE_ID));
    migrations.insert(*actorv2::SYSTEM_ACTOR_CODE_ID, nil_migrator_v3(*actorv3::SYSTEM_ACTOR_CODE_ID));
    migrations.insert(*actorv2::VERIFREG_ACTOR_CODE_ID, nil_migrator_v3(*actorv3::VERIFREG_ACTOR_CODE_ID));

    // Set of prior version code CIDs for actors to defer during iteration, for explicit migration afterwards.
	let deferred_code_ids = HashSet::<Cid>::new(); // None in this migration

    if migrations.len()+deferred_code_ids.len() != 11 {
        panic!("Incomplete migration specification with {} code CIDs", migrations.len());
	}

    let actors_in = StateTree::new_from_root(&store, &actors_root_in).unwrap();
    let actors_out = StateTree::new(&store, StateTreeVersion::V2);
    
    let a = actors_in.for_each(|a,s| {
        let store_clone = store.clone();
        if deferred_code_ids.contains(&s.code) {
            return Ok(());
        }

        let next_input = MigrationJob {
            address: a,
            actor_state: s.clone(),
            cache: cache.clone(),
            actor_migration: migrations[&s.code].clone()
        };

        jobs_future.push(async move {next_input.run(store_clone, prior_epoch)});

        Ok(())
    }).expect("failed to create jobs for each actor state");

    let mut actors_out = actors_out.expect("failed accesing actors_out");

    task::block_on(async {
        while let Some(job_result) = jobs_future.next().await {
            let result = job_result.unwrap();
            actors_out.set_actor(&result.address, result.actor_state).expect("failed updating resulting actor state");
        }
    });

    let root_cid = actors_out.flush().map_err(|_| MigrationErr::FlushFailed);

    root_cid
}
