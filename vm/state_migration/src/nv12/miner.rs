
use ipld_blockstore::BlockStore;
use crate::MigrationErr;
use crate::ActorMigrationInput;
use crate::ActorMigrationResult;
use cid::{Cid, Code::Blake2b256};
pub(crate) struct MinerMigrator;

impl MinerMigrator {
    fn migrate_state<BS: BlockStore>(&self, store: BS, input: ActorMigrationInput) -> Result<ActorMigrationResult, MigrationErr>  {
        let in_state: actorv2::miner::State = store.get(&input.head).unwrap().unwrap();

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
            new_code_cid: self.migrated_code_cid(),
            new_head
        })
    }

    fn migrated_code_cid(&self) -> Cid {
        *actorv3::MINER_ACTOR_CODE_ID
    }
}