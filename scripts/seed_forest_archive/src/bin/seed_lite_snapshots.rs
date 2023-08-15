use anyhow::{Context, Result};
use rand::prelude::Rng;
use std::ops::RangeInclusive;
use which::which;

use seed_forest_archive::historical::HistoricalSnapshot;
use seed_forest_archive::store::Store;
use seed_forest_archive::{forest, ChainEpoch, EPOCH_STEP};

use seed_forest_archive::archive::has_lite_snapshot;

fn main() -> Result<()> {
    which("forest").context("Failed to find the 'forest' binary.\nSee installation instructions: https://github.com/ChainSafe/forest")?;
    which("aws").context("Failed to find the 'aws' binary.")?;

    let snapshots = HistoricalSnapshot::new()?;
    let highest_epoch = snapshots
        .iter()
        .map(HistoricalSnapshot::highest_epoch)
        .max()
        .unwrap_or(0);
    let max_round = highest_epoch / EPOCH_STEP;
    println!("Highest epoch: {highest_epoch}");
    let mut rng = rand::thread_rng();
    let store = Store::new(snapshots.clone());
    loop {
        let round = rng.gen::<ChainEpoch>() % max_round;
        println!("Round {round}");
        let epoch = round * EPOCH_STEP;
        let initial_range = RangeInclusive::new(epoch.saturating_sub(900), epoch);

        if !has_lite_snapshot(epoch)? {
            let mut downloads = vec![];
            let mut paths = vec![];
            for snapshot in store.in_range(&initial_range) {
                println!("Downloading: {}", snapshot.path());
                paths.push(snapshot.path().to_owned());
                downloads.push((snapshot.path().to_owned(), snapshot.encode()?));
            }
            for (path, download) in downloads {
                let output = download.wait_with_output()?;
                if !output.status.success() {
                    eprintln!("Failed to download snapshot. Error message:");
                    eprintln!(
                        "{}",
                        std::str::from_utf8(&output.stderr).unwrap_or_default()
                    );
                    std::process::exit(1);
                }
                println!("Download complete: {path}");
            }

            let output = forest::export(epoch, paths.clone())?.wait_with_output()?;
            if !output.status.success() {
                eprintln!("Failed to export/upload snapshot. Error message:");
                eprintln!(
                    "{}",
                    std::str::from_utf8(&output.stderr).unwrap_or_default()
                );
                std::process::exit(1);
            }
            println!("Upload complete: {:?}", paths);
            for file in paths.drain(..) {
                std::fs::remove_file(file)?;
            }
        } else {
            println!("Lite snapshot already uploaded - skipping");
            continue;
        }
    }
    // Ok(())
}
