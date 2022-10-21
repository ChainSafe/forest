// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::cli::{cli_error_and_die, handle_rpc_err, snapshot_fetch::snapshot_fetch};
use anyhow::bail;
use forest_blocks::tipset_keys_json::TipsetKeysJson;
use forest_rpc_client::chain_ops::*;
use regex::Regex;
use std::{collections::HashMap, ffi::OsStr, fs, path::PathBuf};
use strfmt::strfmt;
use structopt::StructOpt;
use time::{format_description::well_known::Iso8601, Date, OffsetDateTime};

pub(crate) const OUTPUT_PATH_DEFAULT_FORMAT: &str =
    "forest_snapshot_{chain}_{year}-{month}-{day}_height_{height}.car";

#[derive(Debug, StructOpt)]
pub enum SnapshotCommands {
    /// Export a snapshot of the chain to `<output_path>`
    Export {
        /// Tipset to start the export from, default is the chain head
        #[structopt(short, long)]
        tipset: Option<i64>,
        /// Specify the number of recent state roots to include in the export.
        #[structopt(short, long, default_value = "2000")]
        recent_stateroots: i64,
        /// Include old messages
        #[structopt(short, long)]
        include_old_messages: bool,
        /// Snapshot output path. Default to `forest_snapshot_{chain}_{year}-{month}-{day}_height_{height}.car`
        /// Date is in ISO 8601 date format.
        /// Arguments:
        ///  - chain - chain name e.g. `mainnet`
        ///  - year
        ///  - month
        ///  - day
        ///  - height - the epoch
        #[structopt(short, default_value = OUTPUT_PATH_DEFAULT_FORMAT, verbatim_doc_comment)]
        output_path: PathBuf,
        /// Skip creating the checksum file.
        #[structopt(long)]
        skip_checksum: bool,
    },

    /// Fetches the most recent snapshot from a trusted, pre-defined location.
    Fetch {
        /// Directory to which the snapshot should be downloaded. If not provided, it will be saved
        /// in default Forest data location.
        #[structopt(short, long)]
        snapshot_dir: Option<PathBuf>,
    },

    /// Shows default snapshot dir
    Dir,

    /// List local snapshots
    List {
        /// Directory to which the snapshots are downloaded. If not provided, it will be the
        /// default Forest data location.
        #[structopt(short, long)]
        snapshot_dir: Option<PathBuf>,
    },

    /// Remove local snapshot
    Remove {
        /// Snapshot filename to remove
        filename: PathBuf,

        /// Directory to which the snapshots are downloaded. If not provided, it will be the
        /// default Forest data location.
        #[structopt(short, long)]
        snapshot_dir: Option<PathBuf>,

        /// Answer yes to all forest-cli yes/no questions without prompting
        #[structopt(long)]
        force: bool,
    },

    /// Prune local snapshot, keeps the latest only.
    /// Note that file names that do not match forest_snapshot_{chain}_{year}-{month}-{day}_height_{height}.car
    /// pattern will be ignored
    Prune {
        /// Directory to which the snapshots are downloaded. If not provided, it will be the
        /// default Forest data location.
        #[structopt(short, long)]
        snapshot_dir: Option<PathBuf>,

        /// Answer yes to all forest-cli yes/no questions without prompting
        #[structopt(long)]
        force: bool,
    },

    /// Clean all local snapshots, use with care.
    Clean {
        /// Directory to which the snapshots are downloaded. If not provided, it will be the
        /// default Forest data location.
        #[structopt(short, long)]
        snapshot_dir: Option<PathBuf>,

        /// Answer yes to all forest-cli yes/no questions without prompting
        #[structopt(long)]
        force: bool,
    },
}

impl SnapshotCommands {
    pub async fn run(&self, config: Config) {
        match self {
            Self::Export {
                tipset,
                recent_stateroots,
                output_path,
                include_old_messages,
                skip_checksum,
            } => {
                let chain_head = match chain_head().await {
                    Ok(head) => head.0,
                    Err(_) => cli_error_and_die("Could not get network head", 1),
                };

                let epoch = tipset.unwrap_or(chain_head.epoch());

                let now = OffsetDateTime::now_utc();

                let month_string = format!("{:02}", now.month() as u8);
                let year = now.year();
                let day_string = format!("{:02}", now.day() as u8);
                let chain_name = chain_get_name().await.map_err(handle_rpc_err).unwrap();

                let vars = HashMap::from([
                    ("year".to_string(), year.to_string()),
                    ("month".to_string(), month_string),
                    ("day".to_string(), day_string),
                    ("chain".to_string(), chain_name),
                    ("height".to_string(), epoch.to_string()),
                ]);
                let output_path = match strfmt(&output_path.display().to_string(), &vars) {
                    Ok(path) => path.into(),
                    Err(e) => {
                        cli_error_and_die(format!("Unparsable string error: {}", e), 1);
                    }
                };

                let params = (
                    epoch,
                    *recent_stateroots,
                    *include_old_messages,
                    output_path,
                    TipsetKeysJson(chain_head.key().clone()),
                    *skip_checksum,
                );

                // infallible unwrap
                let out = chain_export(params).await.map_err(handle_rpc_err).unwrap();

                println!("Export completed. Snapshot located at {}", out.display());
            }
            Self::Fetch { snapshot_dir } => {
                let snapshot_dir = snapshot_dir
                    .clone()
                    .unwrap_or_else(|| default_snapshot_dir(&config));
                match snapshot_fetch(&snapshot_dir, config).await {
                    Ok(out) => println!("Snapshot successfully downloaded at {}", out.display()),
                    Err(e) => cli_error_and_die(format!("Failed fetching the snapshot: {e}"), 1),
                }
            }
            Self::Dir => {
                let dir = default_snapshot_dir(&config);
                println!("Default snapshot dir: {}", dir.display());
            }
            Self::List { snapshot_dir } => {
                list(&config, snapshot_dir).unwrap();
            }
            Self::Remove {
                filename,
                snapshot_dir,
                force,
            } => {
                remove(&config, filename, snapshot_dir, *force);
            }
            Self::Prune {
                snapshot_dir,
                force,
            } => {
                prune(&config, snapshot_dir, *force);
            }
            Self::Clean {
                snapshot_dir,
                force,
            } => {
                clean(&config, snapshot_dir, *force).unwrap();
            }
        }
    }
}

fn list(config: &Config, snapshot_dir: &Option<PathBuf>) -> anyhow::Result<()> {
    let snapshot_dir = snapshot_dir
        .clone()
        .unwrap_or_else(|| default_snapshot_dir(config));
    println!("Snapshot dir: {}", snapshot_dir.display());
    println!("\nLocal snapshots:");
    fs::read_dir(snapshot_dir)?
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| p.extension().unwrap_or_default() == "car")
        .for_each(|p| println!("{}", p.display()));

    Ok(())
}

fn remove(config: &Config, filename: &PathBuf, snapshot_dir: &Option<PathBuf>, force: bool) {
    let snapshot_dir = snapshot_dir
        .clone()
        .unwrap_or_else(|| default_snapshot_dir(config));
    let snapshot_path = snapshot_dir.join(filename);
    if snapshot_path.exists()
        && snapshot_path.is_file()
        && snapshot_path.extension() == Some(OsStr::new("car"))
    {
        println!("Deleting {}", snapshot_path.display());
        if !force && !prompt_confirm() {
            println!("Aborted.");
            return;
        }

        delete_snapshot(&snapshot_path);
    } else {
        println!(
                "{} is not a valid snapshot file path, to list all snapshots, run forest-cli snapshot list",
                snapshot_path.display());
    }
}

fn prune(config: &Config, snapshot_dir: &Option<PathBuf>, force: bool) {
    {
        let snapshot_dir = snapshot_dir
            .clone()
            .unwrap_or_else(|| default_snapshot_dir(config));
        println!("Snapshot dir: {}", snapshot_dir.display());
        let mut snapshots_with_valid_name = vec![];
        let mut snapshot_to_keep = None;
        let mut latest_date = Date::MIN;
        let mut latest_height = 0;
        let pattern = Regex::new(
            r"^forest_snapshot_([^_]+?)_(?P<date>\d{4}-\d{2}-\d{2})_height_(?P<height>\d+).car$",
        )
        .unwrap();
        if let Ok(dir) = fs::read_dir(snapshot_dir) {
            for path in dir
                .flatten()
                .map(|entry| entry.path())
                .filter(|p| p.is_file())
            {
                if let Some(Some(filename)) = path.file_name().map(|n| n.to_str()) {
                    if let Some(captures) = pattern.captures(filename) {
                        let date = captures.name("date").unwrap();
                        if let Ok(date) = time::Date::parse(date.as_str(), &Iso8601::DEFAULT) {
                            let height = captures
                                .name("height")
                                .unwrap()
                                .as_str()
                                .parse::<i64>()
                                .unwrap();
                            if date > latest_date {
                                latest_date = date;
                                latest_height = height;
                                snapshot_to_keep = Some(path.clone());
                            } else if date == latest_date && height > latest_height {
                                latest_height = height;
                                snapshot_to_keep = Some(path.clone());
                            }

                            snapshots_with_valid_name.push(path);
                        }
                    }
                }
            }
        }

        if snapshots_with_valid_name.len() < 2 {
            println!("No files to delete");
            return;
        }

        let mut snapshots_to_delete = snapshots_with_valid_name;
        let mut index_to_keep = 0;
        println!("Files to delete:");
        if let Some(snapshot_to_keep) = snapshot_to_keep {
            for (i, path) in snapshots_to_delete.iter().enumerate() {
                if &snapshot_to_keep != path {
                    println!("{}", path.as_path().display());
                } else {
                    index_to_keep = i;
                }
            }
        }
        snapshots_to_delete.remove(index_to_keep);

        if !force && !prompt_confirm() {
            println!("Aborted.");
            return;
        }

        for snapshot_path in snapshots_to_delete {
            delete_snapshot(&snapshot_path);
        }
    }
}

fn clean(config: &Config, snapshot_dir: &Option<PathBuf>, force: bool) -> anyhow::Result<()> {
    let snapshot_dir = snapshot_dir
        .clone()
        .unwrap_or_else(|| default_snapshot_dir(config));
    println!("Snapshot dir: {}", snapshot_dir.display());

    let read_dir = match fs::read_dir(snapshot_dir) {
        Ok(read_dir) => read_dir,
        // basically have the same behaviour as in `rm -f` which doesn't fail if the target
        // directory doesn't exist.
        Err(_) if force => {
            println!("Target directory not accessible. Skipping.");
            return Ok(());
        }
        Err(e) => bail!(e),
    };

    let snapshots_to_delete: Vec<_> = read_dir
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| p.extension().unwrap_or_default() == "car")
        .collect();

    if snapshots_to_delete.is_empty() {
        println!("No files to delete");
        return Ok(());
    }

    if !force && !prompt_confirm() {
        println!("Aborted.");
        return Ok(());
    }

    for snapshot_path in snapshots_to_delete {
        delete_snapshot(&snapshot_path);
    }

    Ok(())
}

fn default_snapshot_dir(config: &Config) -> PathBuf {
    config
        .client
        .data_dir
        .join("snapshots")
        .join(config.chain.name.clone())
}

fn delete_snapshot(snapshot_path: &PathBuf) {
    let checksum_path = snapshot_path.with_extension("sha256sum");
    for path in [snapshot_path, &checksum_path] {
        if path.exists() {
            if let Err(err) = fs::remove_file(path) {
                println!("Failed to delete {}\n{err}", path.display());
            } else {
                println!("Deleted {}", path.display());
            }
        }
    }
}

fn prompt_confirm() -> bool {
    println!("Do you want to continue? [y/n]");
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap();
    let line = line.trim().to_lowercase();
    line == "y" || line == "yes"
}
