// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::str::FromStr;

use crate::lotus_json::lotus_json_with_self;

use super::fvm_shared_latest::version::NetworkVersion as NetworkVersion_latest;
pub use fvm_shared2::version::NetworkVersion as NetworkVersion_v2;
use fvm_shared3::version::NetworkVersion as NetworkVersion_v3;
use fvm_shared4::version::NetworkVersion as NetworkVersion_v4;
use pastey::paste;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Specifies the network version
///
/// # Examples
/// ```
/// # use forest::doctest_private::NetworkVersion;
/// let v0 = NetworkVersion::V0;
///
/// // dereference to convert to FVM4
/// assert_eq!(fvm_shared4::version::NetworkVersion::V0, *v0);
///
/// // use `.into()` when FVM3 has to be specified.
/// assert_eq!(fvm_shared3::version::NetworkVersion::V0, v0.into());
///
/// // use `.into()` when FVM2 has to be specified.
/// assert_eq!(fvm_shared2::version::NetworkVersion::V0, v0.into());
/// ```
#[derive(
    Debug,
    Eq,
    PartialEq,
    Clone,
    Copy,
    Ord,
    PartialOrd,
    Serialize,
    Deserialize,
    JsonSchema,
    derive_more::Deref,
    derive_more::DerefMut,
    derive_more::From,
    derive_more::Into,
    derive_more::Display,
)]
#[from(u32, NetworkVersion_v4)]
#[into(u32, NetworkVersion_v4)]
#[repr(transparent)]
#[serde(transparent)]
pub struct NetworkVersion(#[schemars(with = "u32")] pub NetworkVersion_latest);

lotus_json_with_self!(NetworkVersion);

/// Defines public constants V0, V1, ... for [`NetworkVersion`].
/// Each constant is mapped to the corresponding [`NetworkVersion_latest`] variant.
macro_rules! define_network_versions {
    ($($version:literal),+ $(,)?) => {
        impl NetworkVersion {
            $(
                paste! {
                    pub const [<V $version>]: Self = Self(NetworkVersion_latest::[<V $version>]);
                }
            )+
        }
    }
}

define_network_versions!(
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28,
);

impl From<NetworkVersion_v2> for NetworkVersion {
    fn from(value: NetworkVersion_v2) -> Self {
        NetworkVersion((value as u32).into())
    }
}

impl From<NetworkVersion_v3> for NetworkVersion {
    fn from(value: NetworkVersion_v3) -> Self {
        NetworkVersion(u32::from(value).into())
    }
}

impl From<NetworkVersion> for NetworkVersion_v2 {
    fn from(other: NetworkVersion) -> NetworkVersion_v2 {
        u32::from(other.0).try_into().expect("Infallible")
    }
}

impl From<NetworkVersion> for NetworkVersion_v3 {
    fn from(other: NetworkVersion) -> Self {
        u32::from(other.0).into()
    }
}

impl FromStr for NetworkVersion {
    type Err = <u32 as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let v: u32 = s.parse()?;
        Ok(v.into())
    }
}

#[cfg(test)]
impl quickcheck::Arbitrary for NetworkVersion {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let value = u32::arbitrary(g);
        NetworkVersion(NetworkVersion_latest::new(value))
    }
}
