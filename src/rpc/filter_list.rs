// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use itertools::Itertools as _;
use std::path::Path;

/// A filter list that allows or rejects RPC methods based on their name.
#[derive(Default, Clone)]
pub struct FilterList {
    allow: Vec<String>,
    reject: Vec<String>,
}

impl FilterList {
    pub fn new_from_file(file: &Path) -> anyhow::Result<Self> {
        let (allow, reject) = Self::create_allow_reject_list(file)?;
        Ok(Self { allow, reject })
    }

    /// Authorize (or not) an RPC method based on its name.
    /// If the allow list is empty, all methods are authorized, unless they are rejected.
    pub fn authorize(&self, entry: impl AsRef<str>) -> bool {
        let entry = entry.as_ref();
        (self.allow.is_empty() || self.allow.iter().any(|a| entry.contains(a)))
            && !self.reject.iter().any(|r| entry.contains(r))
    }

    pub fn allow(mut self, entry: String) -> Self {
        self.allow.push(entry);
        self
    }

    #[allow(dead_code)]
    pub fn reject(mut self, entry: String) -> Self {
        self.reject.push(entry);
        self
    }

    /// Create a list of allowed and rejected RPC methods from a file.
    fn create_allow_reject_list(file: &Path) -> anyhow::Result<(Vec<String>, Vec<String>)> {
        let filter_file = std::fs::read_to_string(file)?;
        let (reject, allow): (Vec<_>, Vec<_>) = filter_file
            .lines()
            .map(|line| line.trim().to_owned())
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .partition(|line| line.starts_with('!'));

        let reject = reject
            .into_iter()
            .map(|entry| entry.trim_start_matches('!').to_owned())
            .collect_vec();

        Ok((allow, reject))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_filter_list_creation() {
        // Create a temporary file and write some test data to it
        let mut filter_file = tempfile::Builder::new().tempfile().unwrap();
        let list = FilterList::new_from_file(filter_file.path()).unwrap();
        assert!(list.allow.is_empty());
        assert!(list.reject.is_empty());

        write!(
            filter_file,
            r#"# This is a comment
            !cthulhu
            azathoth
            !nyarlathotep
            "#
        )
        .unwrap();

        let list = FilterList::new_from_file(filter_file.path()).unwrap();
        assert_eq!(list.allow, vec!["azathoth".to_string()]);
        assert_eq!(
            list.reject,
            vec!["cthulhu".to_string(), "nyarlathotep".to_string()]
        );

        let list = list
            .allow("shub-niggurath".to_string())
            .reject("yog-sothoth".to_string());
        assert_eq!(
            list.allow,
            vec!["azathoth".to_string(), "shub-niggurath".to_string()]
        );
    }

    #[test]
    fn test_filter_list_authorize() {
        let list = FilterList::default();
        // if allow is empty, all entries are authorized
        assert!(list.authorize("Filecoin.ChainGetBlock"));
        assert!(list.authorize("Filecoin.StateNetworkName"));

        // all entries are authorized, except the rejected ones
        let list = list.reject("Network".to_string());
        assert!(list.authorize("Filecoin.ChainGetBlock"));

        // case-sensitive
        assert!(list.authorize("Filecoin.StatenetworkName"));
        assert!(!list.authorize("Filecoin.StateNetworkName"));

        // if allow is not empty, only the allowed entries are authorized
        let list = FilterList::default().allow("Chain".to_string());
        assert!(list.authorize("Filecoin.ChainGetBlock"));
        assert!(!list.authorize("Filecoin.StateNetworkName"));

        // unless they are rejected
        let list = list.reject("GetBlock".to_string());
        assert!(!list.authorize("Filecoin.ChainGetBlock"));
        assert!(list.authorize("Filecoin.ChainGetMessage"));

        // reject takes precedence over allow
        let list = FilterList::default()
            .allow("Chain".to_string())
            .reject("Chain".to_string());
        assert!(!list.authorize("Filecoin.ChainGetBlock"));
    }

    #[test]
    fn test_filter_list_real_use_case() {
        let mut filter_file = tempfile::Builder::new().tempfile().unwrap();
        write!(
            filter_file,
            r#"# Allow all F3 methods, except for F3.GetParent
            F3.
            !F3.GetParent
            "#
        )
        .unwrap();

        let list = FilterList::new_from_file(filter_file.path()).unwrap();
        assert!(!list.authorize("F3.GetParent"));
        assert!(list.authorize("F3.GetHead"));
        assert!(list.authorize("F3.GetPowerTable"));
        assert!(!list.authorize("Filecoin.ChainExport"));
    }
}
