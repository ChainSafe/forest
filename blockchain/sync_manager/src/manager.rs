use super::bucket::SyncBucketSet;
use blocks::Tipset;

#[derive(Default)]
pub struct SyncManager<'a> {
    sync_queue: SyncBucketSet<'a>,
}

impl<'a> SyncManager<'a> {
    pub fn schedule_tipset(&mut self, tipset: &'a Tipset) {
        // TODO implement interactions for syncing state when SyncManager built out
        self.sync_queue.insert(tipset);
    }
}
