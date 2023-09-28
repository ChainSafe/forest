// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::ops::{Deref, DerefMut};

use super::fvm_shared_latest::version::NetworkVersion as NetworkVersion_latest;
pub use fvm_shared2::version::NetworkVersion as NetworkVersion_v2;
use fvm_shared3::version::NetworkVersion as NetworkVersion_v3;
use fvm_shared4::version::NetworkVersion as NetworkVersion_v4;
use serde::{Deserialize, Serialize};

/// Specifies the network version
///
/// # Examples
/// ```
/// # use forest_filecoin::doctest_private::NetworkVersion;
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
#[derive(Debug, Eq, PartialEq, Clone, Copy, Ord, PartialOrd, Serialize, Deserialize)]
#[repr(transparent)]
#[serde(transparent)]
pub struct NetworkVersion(pub NetworkVersion_latest);

impl NetworkVersion {
    pub const V0: Self = Self(NetworkVersion_latest::new(0));
    pub const V1: Self = Self(NetworkVersion_latest::new(1));
    pub const V2: Self = Self(NetworkVersion_latest::new(2));
    pub const V3: Self = Self(NetworkVersion_latest::new(3));
    pub const V4: Self = Self(NetworkVersion_latest::new(4));
    pub const V5: Self = Self(NetworkVersion_latest::new(5));
    pub const V6: Self = Self(NetworkVersion_latest::new(6));
    pub const V7: Self = Self(NetworkVersion_latest::new(7));
    pub const V8: Self = Self(NetworkVersion_latest::new(8));
    pub const V9: Self = Self(NetworkVersion_latest::new(9));
    pub const V10: Self = Self(NetworkVersion_latest::new(10));
    pub const V11: Self = Self(NetworkVersion_latest::new(11));
    pub const V12: Self = Self(NetworkVersion_latest::new(12));
    pub const V13: Self = Self(NetworkVersion_latest::new(13));
    pub const V14: Self = Self(NetworkVersion_latest::new(14));
    pub const V15: Self = Self(NetworkVersion_latest::new(15));
    pub const V16: Self = Self(NetworkVersion_latest::new(16));
    pub const V17: Self = Self(NetworkVersion_latest::new(17));
    pub const V18: Self = Self(NetworkVersion_latest::new(18));
    pub const V19: Self = Self(NetworkVersion_latest::new(19));
    pub const V20: Self = Self(NetworkVersion_latest::new(20));
    pub const V21: Self = Self(NetworkVersion_latest::new(21));
}

impl Deref for NetworkVersion {
    type Target = NetworkVersion_latest;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for NetworkVersion {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

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

impl From<NetworkVersion_v4> for NetworkVersion {
    fn from(value: NetworkVersion_v4) -> Self {
        NetworkVersion(value)
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

impl From<NetworkVersion> for NetworkVersion_v4 {
    fn from(other: NetworkVersion) -> Self {
        other.0
    }
}
