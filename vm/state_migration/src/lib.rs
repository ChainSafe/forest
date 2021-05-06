use ipld_blockstore::BlockStore;
use cid::Cid;
use vm::TokenAmount;
use clock::ChainEpoch;
use address::Address;

mod nv12;

#[derive(thiserror::Error, Debug, Clone, Copy)]
pub(crate) enum MigrationErr {
    #[error("Cache read failed")]
    MigrationCacheRead,
    #[error("Cache write failed")]
    MigrationCacheWrite,
    #[error("State migration error")]
    StateMigrationErr,
    #[error("Migration failed")]
    Other
}
// Config parameterizes a state tree migration
struct Config {
    // Number of migration worker goroutines to run.
	// More workers enables higher CPU utilization doing migration computations (including state encoding)
    max_workers: usize,
	// Capacity of the queue of jobs available to workers (zero for unbuffered).
	// A queue length of hundreds to thousands improves throughput at the cost of memory.
    job_queue_size: usize,
	// Capacity of the queue receiving migration results from workers, for persisting (zero for unbuffered).
	// A queue length of tens to hundreds improves throughput at the cost of memory.
    res_queue_size: usize,
	// Time between progress logs to emit.
	// Zero (the default) results in no progress logs.
    progress_log_period: std::time::Duration
}

pub(crate) struct ActorMigrationInput  {
	/// actor's address
    address: Address,
    /// actor's balance
	balance: TokenAmount, 
    /// actor's state head CID
	head: Cid,
    // epoch of last state transition prior to migration
	prior_epoch: ChainEpoch,
    /// cache of existing cid -> cid migrations for this actor
	cache: Box<dyn MigrationCache>  
}

pub(crate) struct ActorMigrationResult {
	new_code_cid: Cid,
	new_head: Cid
}

pub(crate) trait ActorMigration<BS: BlockStore> {
    fn migrate_state(
        &self,
        store: BS,
        input: ActorMigrationInput,
    ) -> Result<ActorMigrationResult, MigrationErr>;
    fn migrated_code_cid(&self) -> Cid;
}

struct MigrationJob<BS: BlockStore> {
    address: Address,
    cache: Box<dyn MigrationCache>,
    actor_migration: dyn ActorMigration<BS>,
}

// Migrator which preserves the head CID and provides a fixed result code CID.
pub(crate) struct NilMigrator(Cid);

trait MigrationCache {
    fn write(&self, key: &str, new_cid: Cid) -> Result<(), MigrationErr>;
    fn read(&self, key: &str) -> Result<(bool, Cid), MigrationErr>;
    // fn load(key: &str, func: FN) -> Result<Cid, MigrationErr>
    // where FN: FnOnce() -> Result<Cid, MigrationErr> {

    // }
}

// Migrator that uses cached transformation if it exists
struct CachedMigrator<BS>  {
	cache: Box<dyn MigrationCache>,
	actor_migration: Box<dyn ActorMigration<BS>>,
}

impl<BS: BlockStore> CachedMigrator<BS> {
    fn from(cache: Box<dyn MigrationCache>, m: Box<dyn ActorMigration<BS>>) -> Self {
        CachedMigrator {
            actor_migration: m,
            cache: cache
        }
    }
}
