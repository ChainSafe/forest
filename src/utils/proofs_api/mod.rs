// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod parameters;
mod paramfetch;

pub use parameters::set_proofs_parameter_cache_dir_env;
pub use paramfetch::{ensure_params_downloaded, get_params_default, SectorSizeOpt};

/// Check if the given environment variable is set to truthy value.
fn is_env_truthy(env: &str) -> bool {
    match std::env::var(env) {
        Ok(var) => matches!(var.to_lowercase().as_str(), "1" | "true"),
        _ => false,
    }
}
