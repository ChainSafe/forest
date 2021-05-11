
use ipld_blockstore::BlockStore;
use crate::MigrationErr;
use crate::ActorMigrationInput;
use crate::ActorMigrationResult;
use cid::{Cid, Code::Blake2b256};
use crate::ActorMigration;
use std::rc::Rc;
pub(crate) struct MinerMigrator;

impl<'db, BS: BlockStore> ActorMigration<'db, BS> for MinerMigrator {
    fn migrate_state(&self, store: &'db BS, input: ActorMigrationInput) -> Result<ActorMigrationResult, MigrationErr>  {
        // TODO: error handling
        let v2_state: Option<actorv2::miner::State> = store.get(&input.head).map_err(|e| MigrationErr::Other)?;
        let in_state: actorv2::miner::State = v2_state.ok_or(MigrationErr::Other)?;

        let out_state = actorv3::miner::State {
            info: in_state.info,
            pre_commit_deposits: in_state.pre_commit_deposits,
            locked_funds: in_state.locked_funds,
            vesting_funds: in_state.vesting_funds,
            fee_debt: in_state.fee_debt,
            initial_pledge: in_state.initial_pledge,
            pre_committed_sectors: in_state.pre_committed_sectors,
            pre_committed_sectors_expiry: in_state.pre_committed_sectors_expiry,
            allocated_sectors: in_state.allocated_sectors,
            sectors: in_state.sectors,
            proving_period_start: in_state.proving_period_start,
            current_deadline: in_state.current_deadline as usize,
            deadlines: in_state.deadlines,
            early_terminations: in_state.early_terminations,
            deadline_cron_active: true
        };

        let new_head = store.put(&out_state, Blake2b256).map_err(|e| MigrationErr::Other)?; // FIXME: is Blake2b256 correct here?

        Ok(ActorMigrationResult {
            new_code_cid: *actorv3::MINER_ACTOR_CODE_ID,
            new_head
        })
    }

    // don't really need it
    fn migrated_code_cid(&self) -> Cid {
        *actorv3::MINER_ACTOR_CODE_ID
    }
}