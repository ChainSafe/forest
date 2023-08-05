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
    local: HashMap<RangeInclusive<ChainEpoch>, PathBuf>,
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
        println!("Get range: {:?}", range);
        let required_snapshots = self.known_snapshots.iter().filter(|snapshot| {
            range.contains(snapshot.epoch_range.start())
                || range.contains(snapshot.epoch_range.end())
            || snapshot.epoch_range.contains(range.start())
            || snapshot.epoch_range.contains(range.end())
        });
        for required_snapshot in required_snapshots {
            if self.local.get(&required_snapshot.epoch_range).is_none() {
                println!("Downloading snapshot: {}", required_snapshot.url);
                let base_name = format!("snapshot_{}_to_{}.car.zst", required_snapshot.epoch_range.start(), required_snapshot.epoch_range.end());
                let compressed_name = format!("snapshot_{}_to_{}.forest.car.zst", required_snapshot.epoch_range.start(), required_snapshot.epoch_range.end());
                let tmp_plain_file = TempPath::from_path(&base_name);
                let tmp_forest_file = PathBuf::from(&compressed_name);
                required_snapshot.download(&tmp_plain_file)?;
                super::forest::compress(&tmp_plain_file, &tmp_forest_file)?;
                self.local
                    .insert(required_snapshot.epoch_range.clone(), tmp_forest_file);
            }
        }
        self.drop_before(*range.start())?;
        Ok(())
    }

    pub fn drop_before(&mut self, epoch: ChainEpoch) -> Result<()> {
        for (range,v) in self.local.iter() {
            if range.end() < &epoch {
                std::fs::remove_file(v)?;
            }
        }
        self.local.retain(|range, _| range.end() >= &epoch);
        Ok(())
    }
}
