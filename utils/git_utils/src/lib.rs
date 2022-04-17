//use git_version::git_version;
use lazy_static::lazy_static;
use serde_derive::{Deserialize, Serialize};
use std::fs;
use std::io::{Error, ErrorKind};
use std::path::PathBuf;
use std::process::Command;
use toml;

lazy_static! {
    pub static ref CURRENT_COMMIT: String = current_commit();
}

#[derive(Deserialize, Serialize)]
struct ForestVersion {
    current_commit: GitCommit,
}

#[derive(Deserialize, Serialize)]
struct GitCommit {
    hash: String,
    short: String,
}

pub fn get_forest_current_commit() -> String {
    CURRENT_COMMIT.to_string()
}

pub fn current_commit() -> String {
    try_git_version()
        .or_else(try_git_toml)
        .map_err(|e| {
            Error::new(
                ErrorKind::NotFound,
                format!("Current Commmit: git command failed with Error: '{:?}'", e),
            )
        })
        .unwrap()
    //.expect("Git references must be available on a build system")
}

//Commmand fails when Git is not installed on Build Host
fn try_git_version() -> Result<String, Error> {
    //let git_cmd = git_version!()?;
    let git_cmd_rs = Command::new("git")
        .args(&["rev-parse", "--short", "HEAD"])
        .output()
        .map_err(|e| {
            //Command Launch failed
            Error::new(
                ErrorKind::NotFound,
                format!("Git Commmand: command failed with Error: '{:?}'", e),
            )
        })?;
    println!("Git Commmand: result: '{:?}'", git_cmd_rs);

    //Ok(git_cmd)
    if git_cmd_rs.status.success() {
        String::from_utf8(git_cmd_rs.stdout)
            .map_err(|e| Error::new(ErrorKind::Other, format!("{:?}", e)))
    } else {
        //Git Command failed
        //Building an Error Struct from the Exit Status and Error Message
        let mut error_message = String::from_utf8_lossy(&git_cmd_rs.stderr).into_owned();
        let error_code = match git_cmd_rs.status.code() {
            Some(i) => i,
            None => {
                //Missing Exit Status means an abnormal termination
                error_message.push_str("; Command was terminated abnormally.");
                -1
            }
        };
        Err(Error::new(
            ErrorKind::NotFound,
            format!(
                "Git Commmand: command failed with Error [{}]: '{}'",
                error_code, error_message
            ),
        ))
    }
}

fn try_git_toml(e: Error) -> Result<String, Error> {
    println!("Try recover from Error: '{}'", e);
    let build_dir = std::env::current_dir()
        .map_err(|e| {
            Error::new(
                ErrorKind::NotFound,
                format!(
                    "Build Directory: find directory failed with Error: '{:?}'",
                    e
                ),
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
                            "Commit File '{:?}': read file failed with Error: '{:?}'",
                            commit_file.as_path(),
                            e
                        ),
                    )
                })
                .unwrap();
            let commit_value: ForestVersion = toml::from_str(&commit_toml).map_err(|e| {
                Error::new(
                    ErrorKind::Other,
                    format!(
                        "Commit File '{:?}': parse file failed with Error: '{:?}'",
                        commit_file.file_name(),
                        e
                    ),
                )
            })?;

            Ok(commit_value.current_commit.short)
        } else {
            //Serialized Commit File does not exist
            Err(Error::new(
                ErrorKind::NotFound,
                format!(
                    "Project Directory '{}' - Commit File: '{:?}': file does not exist!",
                    project_dir.display(),
                    commit_file.file_name()
                ),
            ))
        }
    } else {
        //Parent Directory cannot be found
        Err(Error::new(
            ErrorKind::NotFound,
            format!(
                "Project Directory: build directory '{}' does not have a parent",
                build_dir.display()
            ),
        ))
    }
}
