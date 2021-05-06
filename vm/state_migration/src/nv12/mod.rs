
use actorv2::ActorDowncast;
use address::Address;
use cid::Cid;
use ipld_blockstore::BlockStore;
use clock::ChainEpoch;
use std::collections::HashMap;
use crate::{ActorMigration, CachedMigrator};
use crate::Config;
use crate::MigrationCache;
use crate::MigrationErr;
use crate::NilMigrator;
use std::collections::HashSet;

mod util;
mod miner;

fn actor_head_key(addr: Address, head: Cid) -> String {
	format!("{}-h-{}", addr, head)
}

fn migrate_state_tree<BS: BlockStore>(store: &BS,
    actors_root_in: Cid,
    prior_epoch: ChainEpoch,
    cfg: Config,
    cache: Box<dyn MigrationCache>) -> Result<Cid, MigrationErr> {
    if cfg.max_workers <= 0 {
        todo!();
    }

    // Maps prior version code CIDs to migration functions.
    let mut migrations = HashMap::new();
    migrations.insert(*actorv2::ACCOUNT_ACTOR_CODE_ID, NilMigrator(*actorv3::ACCOUNT_ACTOR_CODE_ID));
    migrations.insert(*actorv2::CRON_ACTOR_CODE_ID, NilMigrator(*actorv3::CRON_ACTOR_CODE_ID));
    migrations.insert(*actorv2::INIT_ACTOR_CODE_ID, NilMigrator(*actorv3::INIT_ACTOR_CODE_ID));
    migrations.insert(*actorv2::INIT_ACTOR_CODE_ID, NilMigrator(*actorv3::INIT_ACTOR_CODE_ID));
    migrations.insert(*actorv2::MULTISIG_ACTOR_CODE_ID, NilMigrator(*actorv3::MULTISIG_ACTOR_CODE_ID));
    migrations.insert(*actorv2::PAYCH_ACTOR_CODE_ID, NilMigrator(*actorv3::PAYCH_ACTOR_CODE_ID));
    migrations.insert(*actorv2::REWARD_ACTOR_CODE_ID, NilMigrator(*actorv3::REWARD_ACTOR_CODE_ID));
    migrations.insert(*actorv2::MARKET_ACTOR_CODE_ID,  NilMigrator(*actorv3::MARKET_ACTOR_CODE_ID));
    // migrations.insert(*actorv2::MINER_ACTOR_CODE_ID, CachedMigrator { cache, actor_migration: Box::new(miner::MinerMigrator)});
    migrations.insert(*actorv2::POWER_ACTOR_CODE_ID, NilMigrator(*actorv3::POWER_ACTOR_CODE_ID));
    migrations.insert(*actorv2::SYSTEM_ACTOR_CODE_ID, NilMigrator(*actorv3::SYSTEM_ACTOR_CODE_ID));
    migrations.insert(*actorv2::VERIFREG_ACTOR_CODE_ID, NilMigrator(*actorv3::VERIFREG_ACTOR_CODE_ID));

    // Set of prior version code CIDs for actors to defer during iteration, for explicit migration afterwards.
	let deferred_code_ids = HashSet::<Cid>::new(); // None in this migration

    if migrations.len()+deferred_code_ids.len() != 11 {
        panic!("Incomplete migration specification with {} code CIDs", migrations.len());
	}

    let actors_in = actorv2::util::Multimap::from_root(store, &actors_root_in).unwrap();
    let actors_out = actorv2::util::Multimap::new(store);

    actors_in.for_all(|x,_| {
        // need a closure here which takes address and actor as param
        todo!()
    });

    todo!()
}
