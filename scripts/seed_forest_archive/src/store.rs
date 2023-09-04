use super::ChainEpoch;
use super::HistoricalSnapshot;
use anyhow::Result;
use std::collections::HashMap;
use std::ops::Deref;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

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

    pub fn in_range<'a>(
        &'a self,
        range: &'a RangeInclusive<ChainEpoch>,
    ) -> impl Iterator<Item = &'a HistoricalSnapshot> + 'a {
        self.known_snapshots.iter().filter(|snapshot| {
            range.contains(snapshot.epoch_range.start())
                || range.contains(snapshot.epoch_range.end())
                || snapshot.epoch_range.contains(range.start())
                || snapshot.epoch_range.contains(range.end())
        })
    }

    pub fn get_range(&mut self, range: &RangeInclusive<ChainEpoch>) -> Result<()> {
        self.drop_before(*range.start())?;
        println!("Get range: {:?}", range);
        let required_snapshots = self.known_snapshots.iter().filter(|snapshot| {
            range.contains(snapshot.epoch_range.start())
                || range.contains(snapshot.epoch_range.end())
                || snapshot.epoch_range.contains(range.start())
                || snapshot.epoch_range.contains(range.end())
        });
        for required_snapshot in required_snapshots {
            if self.local.get(&required_snapshot.epoch_range).is_none() {
                println!("Downloading snapshot: {}", required_snapshot.path());
                let compressed_name = format!(
                    "snapshot_{}_to_{}.forest.car.zst",
                    required_snapshot.epoch_range.start(),
                    required_snapshot.epoch_range.end()
                );
                let tmp_forest_file = PathBuf::from(&compressed_name);
                let tmp_download_path = PathBuf::from("tmp.car.zst");
                required_snapshot.download(&tmp_download_path)?;
                super::forest::compress(&tmp_download_path, &tmp_forest_file)?;
                std::fs::remove_file(&tmp_download_path)?;
                self.local
                    .insert(required_snapshot.epoch_range.clone(), tmp_forest_file);
            }
        }
        Ok(())
    }

    pub fn drop_before(&mut self, epoch: ChainEpoch) -> Result<()> {
        for (range, v) in self.local.iter() {
            if range.end() < &epoch {
                std::fs::remove_file(v)?;
            }
        }
        self.local.retain(|range, _| range.end() >= &epoch);
        Ok(())
    }
}
