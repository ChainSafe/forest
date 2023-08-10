use anyhow::{Context, Result};
use rand::prelude::Rng;
use std::ops::RangeInclusive;
use which::which;

use seed_forest_archive::historical::HistoricalSnapshot;
use seed_forest_archive::store::Store;
use seed_forest_archive::{forest, ChainEpoch, DIFF_STEP, EPOCH_STEP};

use seed_forest_archive::archive::{
    has_diff_snapshot, has_lite_snapshot, upload_diff_snapshot, upload_lite_snapshot,
};

fn main() -> Result<()> {
    which("forest").context("Failed to find the 'forest' binary.\nSee installation instructions: https://github.com/ChainSafe/forest")?;

    let mut threads = vec![];

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
        let round = 3;
        println!("Round {round}");
        let epoch = round * EPOCH_STEP;
        let initial_range = RangeInclusive::new(epoch.saturating_sub(2000), epoch);

        if !has_lite_snapshot(epoch)? {
            store.get_range(&initial_range)?;
            let lite_snapshot = forest::export(epoch, store.files())?;
            threads.push(std::thread::spawn(move || {
                upload_lite_snapshot(&lite_snapshot)?;
                std::fs::remove_file(&lite_snapshot)?;
                anyhow::Ok(())
            }));
        } else {
            println!("Lite snapshot already uploaded - skipping");
        }

        for n in 0..EPOCH_STEP / DIFF_STEP {
            let diff_epoch = epoch + DIFF_STEP * n;
            let diff_range =
                RangeInclusive::new(diff_epoch.saturating_sub(2000), diff_epoch + DIFF_STEP);

            if !has_diff_snapshot(diff_epoch, DIFF_STEP)? {
                store.get_range(&diff_range)?;
                let diff_snapshot = forest::export_diff(diff_epoch, DIFF_STEP, store.files())?;
                threads.push(std::thread::spawn(move || {
                    upload_diff_snapshot(&diff_snapshot)?;
                    std::fs::remove_file(&diff_snapshot)?;
                    anyhow::Ok(())
                }));
            } else {
                println!("Diff snapshot already uploaded - skipping");
            }
        }
        break;
    }
    for thread in threads {
        thread.join().unwrap()?;
    }
    // for snapshot in snapshots {
    //     println!("{:?}", snapshot);
    // }

    // Pick incomplete range (x*30000 - (x+1)*30000)
    // Get snapshots to emit lite snapshot.
    // Get snapshtos to emit diff snapshots.
    // Repeat

    // forest_snapshot_calibnet_2023-05-12_height_30000.forest.car.zst
    // forest_diff_calibnet_2023-05-12_height_30000+3000.forest.car.zst
    // Snapshot at epoch 0
    //  1 diff 00000+3000
    //  2 diff 03000+3000
    //  3 diff 06000+3000
    //  4 diff 09000+3000
    //  5 diff 12000+3000
    //  6 diff 15000+3000
    //  7 diff 18000+3000
    //  8 diff 21000+3000
    //  9 diff 24000+3000
    // 10 diff 27000+3000
    // Snapshot at epoch 30000
    // roughly 2880 in a day.
    // 00000-29999
    // 30000-59999

    Ok(())
}
