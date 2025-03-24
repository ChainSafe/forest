// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::Path;
use walkdir::WalkDir;

/// Hard links every file in the directory recursively, into the target directory.
pub fn hard_link_dir(from: impl AsRef<Path>, to: impl AsRef<Path>) -> std::io::Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();
    if to.is_dir() {
        return Err(std::io::Error::other(
            "destination directory already exists",
        ));
    }
    for entry in WalkDir::new(from).into_iter().flatten() {
        if let Ok(relative_path) = entry.path().strip_prefix(from) {
            let to_path = to.join(relative_path);
            if entry.path().is_dir() {
                std::fs::create_dir_all(&to_path)?;
            } else if entry.path().is_file() {
                std::fs::hard_link(entry.path(), &to_path)?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hard_link_dir() {
        let from_dir = tempfile::tempdir().unwrap();
        std::fs::write(from_dir.path().join("1.txt"), "1").unwrap();
        std::fs::create_dir_all(from_dir.path().join("2")).unwrap();
        std::fs::write(from_dir.path().join("2").join("2.txt"), "2").unwrap();
        std::fs::create_dir_all(from_dir.path().join("2").join("3")).unwrap();
        std::fs::write(from_dir.path().join("2").join("3").join("3.txt"), "3").unwrap();

        let to_dir = tempfile::tempdir().unwrap();
        std::fs::remove_dir(to_dir.path()).unwrap();

        hard_link_dir(from_dir.path(), to_dir.path()).unwrap();

        assert_eq!(std::fs::read(to_dir.path().join("1.txt")).unwrap(), b"1");
        assert_eq!(
            std::fs::read(to_dir.path().join("2").join("2.txt")).unwrap(),
            b"2"
        );
        assert_eq!(
            std::fs::read(to_dir.path().join("2").join("3").join("3.txt")).unwrap(),
            b"3"
        );

        std::fs::write(from_dir.path().join("1.txt"), "11").unwrap();
        assert_eq!(std::fs::read(to_dir.path().join("1.txt")).unwrap(), b"11");
    }
}
