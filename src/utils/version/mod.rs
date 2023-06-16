// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use git_version::git_version;
use once_cell::sync::Lazy;

/// Current git commit hash of the Forest repository.
pub const GIT_HASH: &str =
    git_version!(args = ["--always", "--exclude", "*"], fallback = "unknown");

/// Current version of the Forest repository with git hash embedded
/// E.g., `0.8.0+git.e69baf3e4`
pub static FOREST_VERSION_STRING: Lazy<String> =
    Lazy::new(|| format!("{}+git.{}", env!("CARGO_PKG_VERSION"), GIT_HASH));
