// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod system;
mod verifier;

#[macro_export(local_inner_macros)]
macro_rules! define_manifests {
    ($manifest_old:ty, $manifest_new:ty) => {
        type ManifestOld = $manifest_old;
        type ManifestNew = $manifest_new;
    };
}
