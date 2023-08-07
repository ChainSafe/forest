use anyhow::{Context, Result};
use seed_forest_archive::archive::{has_historical_snapshot, upload_historical_snapshot};
use seed_forest_archive::historical::HistoricalSnapshot;
use std::path::PathBuf;
use which::which;

fn main() -> Result<()> {
    which("gsutil").context("Failed to find the 'gsutil' binary.\nSee installation instructions: https://cloud.google.com/storage/docs/gsutil_install")?;
    let mut snapshots = HistoricalSnapshot::new()?;
    snapshots.sort_by_key(|snapshot| *snapshot.epoch_range.start());
    for snapshot in snapshots.into_iter() {
        println!("Snapshot: {}", snapshot.url);
        if !has_historical_snapshot(&snapshot)? {
            let path = PathBuf::from(snapshot.path());
            snapshot.download(&path)?;
            upload_historical_snapshot(&path)?;
            std::fs::remove_file(&path)?;
        } else {
            println!("Skipped")
        }
    }
    Ok(())
}
