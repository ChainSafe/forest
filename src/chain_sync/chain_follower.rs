use std::sync::Arc;

use ahash::HashSet;
use cid::Cid;
use parking_lot::Mutex;
use tokio::task::JoinSet;

use crate::{blocks::FullTipset, chain::ChainStore};

// We receive new full tipsets from the p2p swarm, and from miners that use Forest as their frontend.
pub async fn chain_follower<DB: Sync + Send + 'static>(
    cs: Arc<ChainStore<DB>>,
    tipset_receiver: flume::Receiver<Arc<FullTipset>>,
) -> anyhow::Result<()> {
    let state_machine = Arc::new(Mutex::new(SyncStateMachine::new(cs.clone())));
    let tasks: Arc<Mutex<HashSet<SyncTask>>> = Arc::new(Mutex::new(HashSet::default()));

    let (event_sender, event_receiver) = flume::bounded(20);

    let mut set = JoinSet::new();

    // spawn a task to tipsets to the state machine. These tipsets are received
    // from the p2p swarm and from directly-connected miners.
    set.spawn({
        let event_sender = event_sender.clone();
        async move {
            while let Ok(tipset) = tipset_receiver.recv_async().await {
                event_sender.send(SyncEvent::NewFullTipsets(vec![tipset]));
            }
            // tipset_receiver is closed, shutdown gracefully
        }
    });

    set.spawn(async move {
        while let Ok(event) = event_receiver.recv_async().await {
            let mut sm = state_machine.lock();
            sm.update(event);
            let mut tasks = tasks.lock();
            for task in sm.tasks() {
                // insert task into tasks. If task is already in tasks, skip. If it is not, spawn it.
                let new = tasks.insert(task.clone());
                if new {
                    tokio::spawn(task.execute(cs.clone(), event_sender.clone()));
                }
            }
        }
    });

    set.join_all().await;
    Ok(())
}

enum SyncEvent {
    NewFullTipsets(Vec<Arc<FullTipset>>),
    BadBlock(Cid),
    ValidatedTipset(Arc<FullTipset>),
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

    fn add_full_tipset(&mut self, tipset: Arc<FullTipset>) {
        todo!()
    }

    fn mark_bad_block(&mut self, cid: Cid) {
        todo!()
    }

    fn mark_validated_tipset(&mut self, tipset: Arc<FullTipset>) {
        todo!()
    }

    pub fn update(&mut self, event: SyncEvent) {
        match event {
            SyncEvent::NewFullTipsets(tipsets) => {
                for tipset in tipsets {
                    self.add_full_tipset(tipset);
                }
            }
            SyncEvent::BadBlock(cid) => self.mark_bad_block(cid),
            SyncEvent::ValidatedTipset(tipset) => self.mark_validated_tipset(tipset),
        }
    }

    pub fn tasks(&self) -> Vec<SyncTask> {
        todo!()
    }
}

#[derive(PartialEq, Eq, Hash, Clone)]
enum SyncTask {
    ValidateTipset(Arc<FullTipset>),
    FetchParents(Arc<FullTipset>),
}

impl SyncTask {
    async fn execute<DB: Sync + Send + 'static>(
        self,
        cs: Arc<ChainStore<DB>>,
        sender: flume::Sender<SyncEvent>,
    ) {
        todo!()
    }
}
