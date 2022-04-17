//use git_version::git_version;
use lazy_static::lazy_static;
use serde_derive::{Deserialize, Serialize};
use std::fs;
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::Command;
use toml;

lazy_static! {
    pub static ref CURRENT_COMMIT: String = current_commit();
}

#[derive(Debug, Deserialize, Serialize)]
struct ForestVersion {
    current_commit: GitCommit,
}

#[derive(Debug, Deserialize, Serialize)]
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

    let commit_file = try_find_file(&Path::new("config_forest_commit.toml"))?;
    let commit_value = try_commit_from_path(&commit_file)?;

    Ok(commit_value.current_commit.short)
}

fn try_find_file(file: &Path) -> Result<PathBuf, Error> {
    let work_dir = std::env::current_dir()
        .map_err(|e| {
            Error::new(
                ErrorKind::NotFound,
                format!(
                    "Working Directory: find directory failed with Error: '{:?}'",
                    e
                ),
            )
        })
        .unwrap();
    println!("Working Directory: '{}'", work_dir.display());

    let mut search_dir: Option<&Path> = Some(Path::new(work_dir.as_path()));
    let mut find_file: Option<PathBuf> = None;

    while search_dir.is_some() && find_file.is_none() {
        if let Some(d) = search_dir {
            println!("Search Directory: '{}'", d.display());

            let mut search_file = PathBuf::from(d);

            search_file.push(file);

            if search_file.exists() {
                find_file = Some(search_file);
            } else {
                // Continue searching in Parent Directory
                search_dir = d.parent();
            }
        } //if let Some(d) = search_dir
    } //while search_dir.is_some() && find_file.is_none()

    if let Some(f) = find_file {
        Ok(f)
    } else {
        //Serialized Commit File does not exist
        Err(Error::new(
            ErrorKind::NotFound,
            format!(
                "Working Directory '{}' - Commit File: '{:?}': file does not exist in any parent directory!",
                work_dir.display(),
                file.file_name()
            ),
        ))
    } //if let Some(f) = find_file
}

fn try_commit_from_path(file: &Path) -> Result<ForestVersion, Error> {
    let commit_toml = fs::read_to_string(file)
        .map_err(|e| {
            Error::new(
                ErrorKind::NotFound,
                format!(
                    "Commit File '{:?}': read file failed with Error: '{:?}'",
                    file, e
                ),
            )
        })
        .unwrap();
    let commit_value: ForestVersion = toml::from_str(&commit_toml).map_err(|e| {
        Error::new(
            ErrorKind::Other,
            format!(
                "Commit File '{:?}': parse file failed with Error: '{:?}'",
                file.file_name(),
                e
            ),
        )
    })?;

    Ok(commit_value)
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;
    use std::path::Path;
    use toml;

    use super::*;

    #[test]
    fn test_find_file() {
        let test_file = Path::new("git_utils_test.txt");

        assert!(File::create(&test_file).is_ok());

        let find_result = try_find_file(&test_file);

        println!("Test Find File: '{:?}'", find_result);

        assert!(find_result.is_ok());

        let find_file = find_result.unwrap();

        assert_eq!(find_file.file_name(), test_file.file_name());

        assert!(fs::remove_file(&test_file).is_ok());
    }

    #[test]
    fn test_file_not_found() {
        let test_file = Path::new("no_file.txt");

        let find_result = try_find_file(&test_file);

        println!("Test Not-Found Error: '{:?}'", find_result);

        assert!(find_result.is_err());
    }

    #[test]
    fn test_deserialize_commit() {
        let test_file = Path::new("git_utils_test.toml");
        let test_commit = ForestVersion {
            current_commit: GitCommit {
                hash: String::from("git_commit_hash"),
                short: String::from("git_commit_short"),
            },
        };

        let mut file = File::create(&test_file).unwrap();
        let test_toml = toml::to_string(&test_commit).unwrap();

        assert!(file.write_all(test_toml.as_bytes()).is_ok());

        let commit_result = try_commit_from_path(&test_file);

        println!("Test Deserialize: '{:?}'", commit_result);

        assert!(commit_result.is_ok());

        assert_eq!(
            commit_result.unwrap().current_commit.short,
            "git_commit_short"
        );

        assert!(fs::remove_file(&test_file).is_ok());
    }

    #[test]
    fn test_git_toml() {
        let test_file = Path::new("config_forest_commit.toml");
        let test_commit = ForestVersion {
            current_commit: GitCommit {
                hash: String::from("git_commit_hash"),
                short: String::from("git_commit_short"),
            },
        };

        let mut file = File::create(&test_file).unwrap();
        let test_toml = toml::to_string(&test_commit).unwrap();

        assert!(file.write_all(test_toml.as_bytes()).is_ok());

        let commit_result = try_git_toml(Error::new(ErrorKind::Other, "Mock Git Error"));

        println!("Test Deserialize: '{:?}'", commit_result);

        assert!(commit_result.is_ok());

        assert_eq!(commit_result.unwrap(), "git_commit_short");

        assert!(fs::remove_file(&test_file).is_ok());
    }
}
