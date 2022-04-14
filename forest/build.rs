// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::env;
//use git_version::git_version;
use serde_derive::{Deserialize, Serialize};
use std::fs;
use std::io::{Error, ErrorKind};
use std::path::PathBuf;
use std::process::Command;
use toml;

#[cfg(not(feature = "release"))]
const RELEASE_TRACK: &str = "unstable";

#[cfg(feature = "release")]
const RELEASE_TRACK: &str = "alpha";

const NETWORK: &str = if cfg!(feature = "devnet") {
    "devnet"
} else if cfg!(feature = "interopnet") {
    "interopnet"
} else if cfg!(feature = "calibnet") {
    "calibnet"
} else {
    "mainnet"
};

fn main() {
    println!("cargo:rustc-env=CURRENT_COMMIT={}", current_commit());
    // expose environment variable FOREST_VERSON at build time
    println!("cargo:rustc-env=FOREST_VERSION={}", version());
}

fn current_commit() -> String {
    try_git_version()
        .or_else(try_git_toml)
        .map_err(|e| {
            Error::new(
                ErrorKind::NotFound,
                format!("Current Commmit: git command failed with Error: '{}'", e),
            )
        })
        .unwrap()
    //.expect("Git references must be available on a build system")
}

// returns version string at build time, e.g., `v0.1.0/unstable/mainnet/7af2f5bf`
fn version() -> String {
    let git_hash = match env::var("CURRENT_COMMIT") {
        Ok(cmt) => cmt,
        Err(_) => current_commit(),
    };
    format!(
        "v{}/{}/{}/{}",
        env!("CARGO_PKG_VERSION"),
        RELEASE_TRACK,
        NETWORK,
        git_hash,
    )
}

//Commmand fails when Git is not installed on Build Host
fn try_git_version() -> Result<String, Error> {
    //let git_cmd = git_version!()?;
    let git_cmd = Command::new("git")
        .args(&["rev-parse", "--short", "HEAD"])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::NotFound,
                format!("Git Commmand: command failed with Error: '{:?}'", e),
            )
        }).unwrap();
    println!("git cmd res: '{:?}'", git_cmd);
    //Ok(git_cmd)
    String::from_utf8(git_cmd.stdout).map_err(|e| Error::new(ErrorKind::Other, format!("{}", e)))
}

#[derive(Deserialize, Serialize)]
struct ForestVersion {
    commit: CurrentCommit,
}

#[derive(Deserialize, Serialize)]
struct CurrentCommit {
    hash: String,
    short: String,
}

fn try_git_toml(e: Error) -> Result<String, Error> {
    println!("Try recover from Error: '{}'", e);
    let build_dir = std::env::current_dir()
        .map_err(|e| {
            Error::new(
                ErrorKind::NotFound,
                format!("Build Directory: find directory failed with Error: '{}'", e),
            )
        })
        .unwrap();
    println!("Build Directory: '{}'", build_dir.display());

    if let Some(d) = build_dir.parent() {
        let project_dir = PathBuf::from(d);

        println!("Project Directory: '{}'", project_dir.display());

        let mut commit_file = PathBuf::from(&project_dir);

        commit_file.push("config_forest_commit.toml");

        if commit_file.exists() {
            let commit_toml = fs::read_to_string(commit_file.as_path())
                .map_err(|e| {
                    Error::new(
                        ErrorKind::NotFound,
                        format!(
                            "file: '{:?}': read file failed with Error: '{}'",
                            commit_file.as_path(),
                            e
                        ),
                    )
                })
                .unwrap();
            let commit_value: ForestVersion = toml::from_str(&commit_toml)?;

            Ok(commit_value.commit.short)
        } else {  //Serialized Commit File does not exist
            Err(Error::new(
                ErrorKind::NotFound,
                format!(
                    "Project Directory '{}' - Commit File: '{:?}': file does not exist!",
                    project_dir.display(),
                    commit_file.file_name()
                ),
            ))
        }
    } else {  //Parent Directory cannot be found
        Err(Error::new(
            ErrorKind::NotFound,
            format!(
                "Project Directory: build directory '{}' does not have a parent",
                build_dir.display()
            ),
        ))
    }
}
