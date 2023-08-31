use anyhow::{Context, Result};
use rand::prelude::Rng;
use std::ops::RangeInclusive;
use which::which;

use seed_forest_archive::historical::HistoricalSnapshot;
use seed_forest_archive::store::Store;
use seed_forest_archive::{forest, ChainEpoch, DIFF_STEP, EPOCH_STEP};

use seed_forest_archive::archive::{has_diff_snapshot, has_lite_snapshot, upload_diff_snapshot};

fn main() -> Result<()> {
    which("forest-cli").context("Failed to find the 'forest-cli' binary.\nSee installation instructions: https://github.com/ChainSafe/forest")?;
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
    let mut store = Store::new(snapshots.clone());
    loop {
        let round = rng.gen::<ChainEpoch>() % max_round;
        println!("Round {round}");
        let epoch = round * EPOCH_STEP;

        if !has_lite_snapshot(epoch)? {
            println!("Lite snapshot missing - skipping");
            continue;
        }

        for n in 0..EPOCH_STEP / DIFF_STEP {
            let diff_epoch = epoch + DIFF_STEP * n;
            let diff_range =
                RangeInclusive::new(diff_epoch.saturating_sub(900), diff_epoch + DIFF_STEP);

            if !has_diff_snapshot(diff_epoch, DIFF_STEP)? {
                store.get_range(&diff_range)?;
                let depth = diff_epoch - epoch + 900;
                let diff_snapshot =
                    forest::export_diff(diff_epoch, DIFF_STEP, depth, store.files())?;
                upload_diff_snapshot(&diff_snapshot)?;
                std::fs::remove_file(&diff_snapshot)?;
            } else {
                println!("Diff snapshot already uploaded - skipping");
            }
        }
    }
    // Ok(())
}
