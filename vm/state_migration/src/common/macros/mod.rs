// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod system;
mod verifier;

/// Define type aliases for `Manifest` types before and after the state
/// migration, namely `ManifestOld` and `ManifestNew`
#[macro_export]
macro_rules! define_manifests {
    ($manifest_old:ty, $manifest_new:ty) => {
        type ManifestOld = $manifest_old;
        type ManifestNew = $manifest_new;
    };
}
