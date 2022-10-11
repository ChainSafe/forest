// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::cli::{cli_error_and_die, handle_rpc_err, snapshot_fetch::snapshot_fetch};
use console::Term;
use dialoguer::Confirm;
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
        #[structopt(short, long)]
        yes: bool,
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
        #[structopt(short, long)]
        yes: bool,
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
                println!("Default snapshot dir: {:?}", dir);
            }
            Self::List { snapshot_dir } => {
                let snapshot_dir = snapshot_dir
                    .clone()
                    .unwrap_or_else(|| default_snapshot_dir(&config));
                println!("Snapshot dir: {:?}", snapshot_dir);
                if let Ok(dir) = fs::read_dir(snapshot_dir) {
                    println!("\nLocal snapshots:");
                    for entry in dir.flatten() {
                        let path = entry.path();
                        if path.is_file() {
                            if let Some(filename) = entry.file_name().to_str() {
                                if filename.ends_with(".car") {
                                    println!("{filename}");
                                }
                            }
                        }
                    }
                }
            }
            Self::Remove {
                filename,
                snapshot_dir,
                yes,
            } => {
                let yes = *yes;
                let snapshot_dir = snapshot_dir
                    .clone()
                    .unwrap_or_else(|| default_snapshot_dir(&config));
                let mut snapshot_path = snapshot_dir;
                snapshot_path.push(filename);
                if snapshot_path.exists()
                    && snapshot_path.is_file()
                    && snapshot_path.extension() == Some(OsStr::new("car"))
                {
                    let term = Term::stdout();
                    term.write_line(&format!("Deleting {:?}", snapshot_path))
                        .unwrap();
                    if !yes
                        && !Confirm::new()
                            .with_prompt("Do you want to continue?")
                            .interact()
                            .unwrap()
                    {
                        term.write_line("Aborted.").unwrap();
                        return;
                    }

                    delete_snapshot(&snapshot_path);
                } else {
                    let term = Term::stdout();
                    term.write_line(&format!(
                        "{:?} is not a valid snapshot file path, to list all snapshots, run forest-cli snapshot list",
                        snapshot_path)
                    ).unwrap();
                }
            }
            Self::Prune { snapshot_dir, yes } => {
                let snapshot_dir = snapshot_dir
                    .clone()
                    .unwrap_or_else(|| default_snapshot_dir(&config));
                println!("Snapshot dir: {:?}", snapshot_dir);
                let mut snapshots_with_valid_name = vec![];
                let mut snapshot_to_keep = None;
                let mut latest_date = Date::MIN;
                let mut latest_height = 0;
                let pattern = Regex::new(
                    r"^forest_snapshot_([^_]+?)_(?P<date>\d{4}-\d{2}-\d{2})_height_(?P<height>\d+).car$",
                )
                .unwrap();
                if let Ok(dir) = fs::read_dir(snapshot_dir) {
                    println!("\nLocal snapshots:");
                    for entry in dir.flatten() {
                        let path = entry.path();
                        if path.is_file() {
                            if let Some(filename) = entry.file_name().to_str() {
                                let captures = pattern.captures(filename);
                                if let Some(captures) = captures {
                                    if let Some(date) = captures.name("date") {
                                        if let Ok(date) =
                                            time::Date::parse(date.as_str(), &Iso8601::DEFAULT)
                                        {
                                            if let Some(height) = captures.name("height") {
                                                if let Ok(height) = height.as_str().parse::<i64>() {
                                                    if date > latest_date {
                                                        latest_date = date;
                                                        latest_height = height;
                                                        snapshot_to_keep = Some(path.clone());
                                                    } else if date == latest_date
                                                        && height > latest_height
                                                    {
                                                        latest_height = height;
                                                        snapshot_to_keep = Some(path.clone());
                                                    }

                                                    snapshots_with_valid_name.push(path);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                let term = Term::stdout();
                if snapshots_with_valid_name.len() < 2 {
                    term.write_line("No files to delete").unwrap();
                    return;
                }

                let mut snapshots_to_delete = snapshots_with_valid_name;
                let mut index_to_keep = 0;
                term.write_line("Files to delete:").unwrap();
                if let Some(snapshot_to_keep) = snapshot_to_keep {
                    for (i, path) in snapshots_to_delete.iter().enumerate() {
                        if &snapshot_to_keep != path {
                            term.write_line(&format!("{:?}", path.as_path())).unwrap();
                        } else {
                            index_to_keep = i;
                        }
                    }
                }
                snapshots_to_delete.remove(index_to_keep);

                if !yes
                    && !Confirm::new()
                        .with_prompt("Do you want to continue?")
                        .interact()
                        .unwrap()
                {
                    term.write_line("Aborted.").unwrap();
                    return;
                }

                for snapshot_path in snapshots_to_delete {
                    delete_snapshot(&snapshot_path);
                }
            }
        }
    }
}

fn default_snapshot_dir(config: &Config) -> PathBuf {
    config
        .client
        .data_dir
        .join("snapshots")
        .join(config.chain.name.clone())
}

fn delete_snapshot(snapshot_path: &PathBuf) {
    let term = Term::stdout();
    let mut checksum_path = snapshot_path.clone();
    checksum_path.set_extension("sha256sum");
    for path in [snapshot_path, &checksum_path] {
        if path.exists() {
            if let Err(err) = fs::remove_file(path) {
                term.write_line(&format!("Failed to delete {:?}\n{err}", path))
                    .unwrap();
            } else {
                term.write_line(&format!("Deleted {:?}", path)).unwrap();
            }
        }
    }
}
