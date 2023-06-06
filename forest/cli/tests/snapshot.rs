// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//! Basic UI tests for the snapshot subcommand
//! We only cover the `testable` aspects, so no fetching snapshots (and no validating snapshots)
//!
//! Where applicable, take care to write utility functions so that they can
//! accept both &[TempDir] and &[std::path::PathBuf] (so they can be debugged if needed)
//!
//! See `forest_cli_check.sh` for those tests

use std::{
    fmt::Display,
    path::{Component, Path},
};

use assert_cmd::{assert::Assert, Command};
use itertools::Itertools;
use predicates::boolean::PredicateBooleanExt as _;
use tap::Pipe as _;
use tempfile::TempDir;

/// Just check that the dir command produces some output
#[test]
fn dir() {
    forest_cli()
        .args(["snapshot", "default-location"])
        .pipe_ref_mut(run_and_log("snapshot dir"))
        .success()
        .stdout(predicates::str::is_empty().not());
}

/// Ask forest to intern some snapshots, and check that the count of snapshots increments accordingly
/// Then, delete the snapshot dir, and check the count is zero again
#[test]
fn intern_list_and_manually_clean() {
    let snapshot_dir = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();

    let forest_snapshot_name = "forest_snapshot_calibnet_2023-06-01_height_609819.car.zst";
    let filops_snapshot_name = "610080_2023_06_01T14_13_00Z.car.zst";
    tmp.touch_all([forest_snapshot_name, filops_snapshot_name]);

    assert_eq!(snapshot_count(&snapshot_dir), 0);

    snapshot_intern(&snapshot_dir, &tmp.path().join(filops_snapshot_name), None);
    assert_eq!(snapshot_count(&snapshot_dir), 1);

    snapshot_intern(&snapshot_dir, &tmp.path().join(forest_snapshot_name), None);
    assert_eq!(snapshot_count(&snapshot_dir), 2);

    snapshot_dir.rm_rf();
    assert_eq!(snapshot_count(&snapshot_dir), 0);
}

/// Ask forest to intern 2 mainnet snapshots, and prune one of them away
/// Then, add a calibnet snapshot - it should remain after pruning
/// Then, add a second calibnet snapshot - it should be removed after pruning
#[test]
fn prune() {
    let snapshot_dir = TempDir::new().unwrap().into_path();
    let tmp = TempDir::new().unwrap().into_path();

    let tall_mainnet_snapshot = "777777_2023_06_01T14_13_00Z.car.zst";
    let short_mainnet_snapshot = "222222_2023_06_01T14_13_00Z.car.zst";

    let tall_calibnet_snapshot = "666666_2023_06_01T14_13_00Z.car.zst";
    let short_calibnet_snapshot = "111111_2023_06_01T14_13_00Z.car.zst";

    tmp.touch_all([
        tall_mainnet_snapshot,
        short_mainnet_snapshot,
        tall_calibnet_snapshot,
        short_calibnet_snapshot,
    ]);

    println!("Add and prune the mainnet snapshots");
    assert_eq!(snapshot_count(&snapshot_dir), 0);
    for snapshot in [tall_mainnet_snapshot, short_mainnet_snapshot] {
        snapshot_intern(&snapshot_dir, &tmp.join(snapshot), "mainnet")
    }
    assert_eq!(snapshot_count(&snapshot_dir), 2);

    snapshot_prune(&snapshot_dir);
    assert_eq!(snapshot_count(&snapshot_dir), 1);

    println!("Add a calibnet snapshot, which survives pruning");
    snapshot_intern(&snapshot_dir, &tmp.join(tall_calibnet_snapshot), "calibnet");
    assert_eq!(snapshot_count(&snapshot_dir), 2);
    snapshot_prune(&snapshot_dir);
    assert_eq!(snapshot_count(&snapshot_dir), 2);

    println!("Add a second calibnet snapshot, which is pruned away");
    snapshot_intern(
        &snapshot_dir,
        &tmp.join(short_calibnet_snapshot),
        "calibnet",
    );
    assert_eq!(snapshot_count(&snapshot_dir), 3);
    snapshot_prune(&snapshot_dir);
    assert_eq!(snapshot_count(&snapshot_dir), 2);
}

fn forest_cli() -> Command {
    Command::cargo_bin("forest-cli").expect("couldn't find test binary")
}

/// Run the command, logging its output and exit status to stdout, and returning an assert handle
/// This is useful for sequences of commands
fn run_and_log(message: impl Display) -> impl FnMut(&mut Command) -> Assert {
    move |command| {
        println!(">>>>{message}>>>>");
        let assert = command.assert();
        let output = assert.get_output();
        println!("===exit===");
        println!("{:?}", output.status);
        println!("===stdout===");
        match std::str::from_utf8(&output.stdout) {
            Ok(s) => println!("{s}"),
            Err(_) => println!("<binary>"),
        }
        println!("===stderr===");
        match std::str::from_utf8(&output.stderr) {
            Ok(s) => println!("{s}"),
            Err(_) => println!("<binary>"),
        }
        println!("<<<<{message}<<<<");
        assert
    }
}

/// Note the snapshot list output is not currently stable, this just helps us count snapshots
fn snapshot_count(snapshot_dir: &impl AsRef<Path>) -> usize {
    // NOTE log messages come to stdout(!), so parsing that is unreliable. See #2946
    forest_cli()
        .args(["snapshot", "list"])
        .arg("--snapshot-dir")
        .arg(snapshot_dir.as_ref())
        .pipe_ref_mut(run_and_log("snapshot count"))
        .success()
        .pipe_ref(Assert::get_output)
        .pipe(|it| std::str::from_utf8(&it.stdout))
        .expect("non-utf8 output")
        .lines()
        .filter(|s| *s == "snapshot:")
        .count()
}

fn snapshot_intern<'a>(
    snapshot_dir: &impl AsRef<Path>,
    snapshot: &Path,
    chain: impl Into<Option<&'a str>>,
) {
    forest_cli()
        .pipe_ref_mut(|cmd| match chain.into() {
            Some(chain) => cmd.arg(format!("--chain={chain}")),
            None => cmd,
        })
        .args(["snapshot", "intern"])
        .arg("--snapshot-dir")
        .arg(snapshot_dir.as_ref())
        .arg(snapshot)
        .pipe_ref_mut(run_and_log("snapshot intern"))
        .success();
}

fn snapshot_prune(snapshot_dir: &impl AsRef<Path>) {
    forest_cli()
        .args(["snapshot", "prune"])
        .arg("--snapshot-dir")
        .arg(snapshot_dir.as_ref())
        .pipe_ref_mut(run_and_log("snapshot prune"))
        .success();
}

fn assert_one_normal_component(s: &str) -> &Path {
    if let Ok(Component::Normal(_)) = Path::new(s).components().exactly_one() {
        return Path::new(s);
    }
    panic!("{s} is not one normal path component")
}

/// `Self` should be a directory containing files and folders (no symlinks etc)
pub trait DirectoryExt: AsRef<Path> + Sized {
    /// Create files in `self`
    fn touch_all<'a>(&self, filenames: impl IntoIterator<Item = &'a str>) -> &Self {
        let mut sel = self;
        for filename in filenames {
            sel = self.touch(filename);
        }
        sel
    }
    /// Create a file underneath `self`
    fn touch(&self, filename: &str) -> &Self {
        self.tee(filename, [])
    }
    /// Populate a file underneath `self`
    fn tee(&self, filename: &str, contents: impl AsRef<[u8]>) -> &Self {
        let path = self.as_ref().join(assert_one_normal_component(filename));
        std::fs::write(&path, contents)
            .unwrap_or_else(|e| panic!("couldn't write file {}: {e}", path.display()));
        self
    }
    /// Create an empty directory underneath `self`
    fn mkdir(&self, dirname: &str) -> &Self {
        self.mkdir_and(dirname, |_| {})
    }
    /// Create a directory underneath `self`, and pass its path to given closure
    fn mkdir_and<AnyT>(&self, dirname: &str, mut f: impl FnMut(&Path) -> AnyT) -> &Self {
        let path = self.as_ref().join(assert_one_normal_component(dirname));
        std::fs::create_dir_all(&path)
            .unwrap_or_else(|e| panic!("couldn't create directory {}: {e}", path.display()));
        f(&path);
        self
    }
    /// Remove all children of `self`
    fn rm_rf(&self) -> &Self {
        for (path, metadata) in std::fs::read_dir(self.as_ref())
            .and_then(|read_dir| {
                read_dir
                    .map(|maybe_entry| {
                        maybe_entry.and_then(|entry| {
                            entry.metadata().map(|metadata| (entry.path(), metadata))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_else(|e| panic!("couldnt list {}: {e}", self.as_ref().display()))
        {
            if metadata.is_file() {
                std::fs::remove_file(&path)
            } else if metadata.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                panic!("unsupported child in directory")
            }
            .unwrap_or_else(|e| panic!("error removing {}: {e}", path.display()))
        }
        self
    }
}

impl<T> DirectoryExt for T where T: AsRef<Path> + Sized {}
