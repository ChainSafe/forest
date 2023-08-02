use super::ChainEpoch;
use super::HistoricalSnapshot;
use anyhow::Result;
use std::collections::btree_map::Range;
use std::collections::HashMap;
use std::ops::Deref;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use tempfile::{NamedTempFile, TempPath};

pub struct Store {
    known_snapshots: Vec<HistoricalSnapshot>,
    local: HashMap<RangeInclusive<ChainEpoch>, TempPath>,
}

impl Store {
    pub fn new(snapshots: Vec<HistoricalSnapshot>) -> Self {
        Store {
            known_snapshots: snapshots,
            local: HashMap::new(),
        }
    }

    pub fn files(&self) -> Vec<&Path> {
        self.local.values().map(|tmp| tmp.deref()).collect()
    }

    pub fn get_range(&mut self, range: &RangeInclusive<ChainEpoch>) -> Result<()> {
        let required_snapshots = self.known_snapshots.iter().filter(|snapshot| {
            snapshot.epoch_range.contains(range.start())
                || snapshot.epoch_range.contains(range.end())
        });
        for required_snapshot in required_snapshots {
            if self.local.get(&required_snapshot.epoch_range).is_none() {
                println!("Downloading snapshot: {}", required_snapshot.url);
                let tmp_plain_file = NamedTempFile::new_in(".")?.into_temp_path();
                let tmp_forest_file = NamedTempFile::new_in(".")?.into_temp_path();
                required_snapshot.download(&tmp_plain_file)?;
                super::forest::compress(&tmp_plain_file, &tmp_forest_file)?;
                self.local
                    .insert(required_snapshot.epoch_range.clone(), tmp_forest_file);
            }
        }
        Ok(())
    }

    pub fn insert(&mut self, range: RangeInclusive<ChainEpoch>, path: PathBuf) {
        self.local.insert(range, TempPath::from_path(path));
    }

    pub fn drop_before(&mut self, epoch: ChainEpoch) {
        self.local.retain(|range, _| range.end() < &epoch)
    }
}
