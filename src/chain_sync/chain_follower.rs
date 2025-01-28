use std::sync::Arc;

use parking_lot::Mutex;

use crate::{blocks::FullTipset, chain::ChainStore};

// struct ChainFollower<DB> {
//     state_machine: Arc<Mutex<SyncStateMachine<DB>>>,
//     tasks: Arc<Mutex<Vec<SyncTask>>>,
// }

pub async fn chain_follower<DB>(cs: Arc<ChainStore<DB>>) -> anyhow::Result<()> {
    todo!()
}

struct SyncStateMachine<DB> {
    // Head
    cs: Arc<ChainStore<DB>>,
    // Chains
    chains: Vec<Vec<Arc<FullTipset>>>,
}

impl<DB> SyncStateMachine<DB> {
    pub fn new(cs: Arc<ChainStore<DB>>) -> Self {
        Self { cs, chains: vec![] }
    }

    pub fn add_full_tipset(&mut self, tipset: Arc<FullTipset>) {
        todo!()
    }

    pub fn tasks(&self) -> Vec<SyncTask> {
        todo!()
    }
}

enum SyncTask {
    ValidateTipset(Arc<FullTipset>),
    FetchParents(Arc<FullTipset>),
}
