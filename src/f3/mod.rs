// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(all(f3sidecar, not(feature = "no-f3-sidecar")))]
mod go_ffi;
#[cfg(all(f3sidecar, not(feature = "no-f3-sidecar")))]
use go_ffi::*;

pub fn run_f3_sidecar_if_enabled(
    _rpc_endpoint: String,
    _f3_rpc_endpoint: String,
    _finality: i64,
    _db: String,
    _manifest_server: String,
) {
    if is_sidecar_ffi_enabled() {
        #[cfg(all(f3sidecar, not(feature = "no-f3-sidecar")))]
        {
            GoF3NodeImpl::run(
                _rpc_endpoint,
                _f3_rpc_endpoint,
                _finality,
                _db,
                _manifest_server,
            );
        }
    }
}

// Use opt-in mode for now. Consider switching to opt-out mode once F3 is shipped.
fn is_sidecar_ffi_enabled() -> bool {
    // Opt-out building the F3 sidecar staticlib
    match std::env::var("FOREST_F3_SIDECAR_FFI_ENABLED") {
        Ok(value) => {
            let enabled = matches!(value.to_lowercase().as_str(), "1" | "true");
            cfg_if::cfg_if! {
                if #[cfg(all(f3sidecar, not(feature = "no-f3-sidecar")))] {
                    enabled
                }
                else {
                    if enabled {
                        tracing::error!("Failed to enable F3 sidecar, the forerst binary is not compiled with f3-sidecar Go lib");
                    }
                    false
                }
            }
        }
        _ => false,
    }
}
