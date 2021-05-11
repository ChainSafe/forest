use vm::ActorState;
use ipld_blockstore::BlockStore;
use cid::Cid;
use vm::TokenAmount;
use clock::ChainEpoch;
use address::Address;
use std::rc::Rc;

pub mod nv12;

#[derive(thiserror::Error, Debug, Clone)]
pub enum MigrationErr {
    #[error("Cache read failed")]
    MigrationCacheRead,
    #[error("Cache write failed")]
    MigrationCacheWrite,
    #[error("State migration: {0}")]
    MigrationJobErr(String),
    #[error("Flush failed post migration")]
    FlushFailed,
    #[error("Migration failed")]
    Other
}
// // Config parameterizes a state tree migration
// struct Config {
//     // Number of migration worker goroutines to run.
// 	// More workers enables higher CPU utilization doing migration computations (including state encoding)
//     max_workers: usize,
// 	// Capacity of the queue of jobs available to workers (zero for unbuffered).
// 	// A queue length of hundreds to thousands improves throughput at the cost of memory.
//     job_queue_size: usize,
// 	// Capacity of the queue receiving migration results from workers, for persisting (zero for unbuffered).
// 	// A queue length of tens to hundreds improves throughput at the cost of memory.
//     res_queue_size: usize,
// 	// Time between progress logs to emit.
// 	// Zero (the default) results in no progress logs.
//     progress_log_period: std::time::Duration
// }

pub(crate) struct ActorMigrationInput  {
	/// actor's address
    address: Address,
    /// actor's balance
	balance: TokenAmount, 
    /// actor's state head CID
	head: Cid,
    // epoch of last state transition prior to migration
	prior_epoch: ChainEpoch,
    // /// cache of existing cid -> cid migrations for this actor
	// cache: Rc<dyn MigrationCache>  
}

pub(crate) struct ActorMigrationResult {
	new_code_cid: Cid,
	new_head: Cid
}

pub(crate) trait ActorMigration<'db, BS: BlockStore> {
    fn migrate_state(
        &self,
        store: &'db BS,
        input: ActorMigrationInput,
    ) -> Result<ActorMigrationResult, MigrationErr>;
    fn migrated_code_cid(&self) -> Cid;
}

struct MigrationJob<'db, BS: BlockStore> {
    address: Address,
    actor_state: ActorState,
    // cache: Rc<dyn MigrationCache>,
    actor_migration: Rc<dyn ActorMigration<'db, BS>>,
}

impl<'db, BS: BlockStore> MigrationJob<'db, BS> {
    fn run(&self, store: &'db BS, prior_epoch: ChainEpoch) -> Result<MigrationJobResult, ()> {
        let result = self.actor_migration.migrate_state(store,  ActorMigrationInput{
            address:    self.address,
            balance:    self.actor_state.balance.clone(),
            head:       self.actor_state.state,
            prior_epoch: prior_epoch,
            // cache:      self.cache.clone(),
        }).map_err(|e| 
            MigrationErr::MigrationJobErr(format!("state migration failed for {} actor, addr {}:{}", self.actor_state.code, self.address, e.to_string()
        ))).unwrap();

        let migration_job_result = MigrationJobResult {
            address: self.address,
            actor_state: ActorState::new(result.new_code_cid, result.new_head, self.actor_state.balance.clone(), self.actor_state.sequence)
        };

        Ok(migration_job_result)
    }
}

#[derive(Debug)]
struct MigrationJobResult {
    address: Address,
    actor_state: ActorState
}

// Migrator which preserves the head CID and provides a fixed result code CID.
pub(crate) struct NilMigrator(Cid);

impl<'db, BS: BlockStore> ActorMigration<'db, BS> for NilMigrator {
    fn migrate_state(&self, store: &'db BS, input: ActorMigrationInput) -> Result<ActorMigrationResult, MigrationErr> {
        Ok(ActorMigrationResult {
            new_code_cid: self.0,
            new_head: input.head
        })
    }
    fn migrated_code_cid(&self) -> Cid {
        self.0
    }
} 

trait MigrationCache {
    fn write(&self, key: &str, new_cid: Cid) -> Result<(), MigrationErr>;
    fn read(&self, key: &str) -> Result<(bool, Cid), MigrationErr>;
    // fn load(key: &str, func: FN) -> Result<Cid, MigrationErr>
    // where FN: FnOnce() -> Result<Cid, MigrationErr> {

    // }
}

// // Migrator that uses cached transformation if it exists
// struct CachedMigrator<'db, BS>  {
// 	cache: Rc<dyn MigrationCache>,
// 	actor_migration: Box<dyn ActorMigration<'db, BS>>,
// }

// impl<BS: BlockStore> CachedMigrator<BS> {
//     fn from(cache: Rc<dyn MigrationCache>, m: Box<dyn ActorMigration<BS>>) -> Self {
//         CachedMigrator {
//             actor_migration: m,
//             cache: cache
//         }
//     }
// }
