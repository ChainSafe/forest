use super::ChainEpoch;
use super::HistoricalSnapshot;
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use std::ops::RangeInclusive;
use tempfile::NamedTempFile;

pub struct Store {
    known_snapshots: Vec<HistoricalSnapshot>,
    local: HashMap<HistoricalSnapshot, NamedTempFile>,
}

impl Store {
    pub fn new(snapshots: Vec<HistoricalSnapshot>) -> Self {
        Store {
            known_snapshots: snapshots,
            local: HashMap::new(),
        }
    }
    
    pub fn files(&self) -> Vec<&Path> {
        self.local.values().map(|tmp| tmp.path()).collect()
    }

    pub fn get_range(&mut self, range: RangeInclusive<ChainEpoch>) -> Result<()> {
        let required_snapshots = self.known_snapshots.iter().filter(|snapshot| {
            snapshot.epoch_range.contains(range.start())
                || snapshot.epoch_range.contains(range.end())
        });
        for required_snapshot in required_snapshots {
            if self.local.get(required_snapshot).is_none() {
                println!("Downloading snapshot: {}", required_snapshot.url);
                let tmp_file = NamedTempFile::new_in(".")?;
                required_snapshot.download(tmp_file.path())?;
                self.local.insert(required_snapshot.clone(), tmp_file);
            }
        }
        Ok(())
    }

    pub fn drop_before(&mut self, epoch: ChainEpoch) {
        self.local
            .retain(|snapshot, _| snapshot.epoch_range.end() < &epoch)
    }
}
