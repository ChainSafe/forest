// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod transport;
pub use transport::*;

mod behaviour;
pub use behaviour::*;

cfg_if::cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        use wasm_bindgen::prelude::*;
        use wasm_bindgen_futures::*;

        mod js_exports;

        mod conn;
        use conn::*;

        mod js_ffi;
        use js_ffi::*;

        mod logger;
        use logger::*;

        mod utils;
        use utils::*;
    }
}
