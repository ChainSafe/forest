// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::ops::{Deref, DerefMut};

pub use fvm_shared::version::NetworkVersion as NetworkVersion_v2;
pub use fvm_shared3::version::NetworkVersion as NetworkVersion_v3;
use serde::{Deserialize, Serialize};

/// Specifies the network version
///
/// # Examples
/// ```
/// # use forest_filecoin::doctest_private::NetworkVersion;
/// let v0 = NetworkVersion::V0;
///
/// // dereference to convert to FVM3
/// assert_eq!(fvm_shared3::version::NetworkVersion::V0, *v0);
///
/// // use `.into()` when FVM2 has to be specified.
/// assert_eq!(fvm_shared::version::NetworkVersion::V0, v0.into());
/// ```
#[derive(Debug, Eq, PartialEq, Clone, Copy, Ord, PartialOrd, Serialize, Deserialize)]
#[repr(transparent)]
#[serde(transparent)]
pub struct NetworkVersion(pub NetworkVersion_v3);

impl NetworkVersion {
    pub const V0: Self = Self(NetworkVersion_v3::new(0));
    pub const V1: Self = Self(NetworkVersion_v3::new(1));
    pub const V2: Self = Self(NetworkVersion_v3::new(2));
    pub const V3: Self = Self(NetworkVersion_v3::new(3));
    pub const V4: Self = Self(NetworkVersion_v3::new(4));
    pub const V5: Self = Self(NetworkVersion_v3::new(5));
    pub const V6: Self = Self(NetworkVersion_v3::new(6));
    pub const V7: Self = Self(NetworkVersion_v3::new(7));
    pub const V8: Self = Self(NetworkVersion_v3::new(8));
    pub const V9: Self = Self(NetworkVersion_v3::new(9));
    pub const V10: Self = Self(NetworkVersion_v3::new(10));
    pub const V11: Self = Self(NetworkVersion_v3::new(11));
    pub const V12: Self = Self(NetworkVersion_v3::new(12));
    pub const V13: Self = Self(NetworkVersion_v3::new(13));
    pub const V14: Self = Self(NetworkVersion_v3::new(14));
    pub const V15: Self = Self(NetworkVersion_v3::new(15));
    pub const V16: Self = Self(NetworkVersion_v3::new(16));
    pub const V17: Self = Self(NetworkVersion_v3::new(17));
    pub const V18: Self = Self(NetworkVersion_v3::new(18));
    pub const V19: Self = Self(NetworkVersion_v3::new(19));
    pub const V20: Self = Self(NetworkVersion_v3::new(20));
}

impl Deref for NetworkVersion {
    type Target = NetworkVersion_v3;
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
        NetworkVersion(NetworkVersion_v3::from(value as u32))
    }
}

impl From<NetworkVersion_v3> for NetworkVersion {
    fn from(value: NetworkVersion_v3) -> Self {
        NetworkVersion(value)
    }
}

impl From<NetworkVersion> for NetworkVersion_v3 {
    fn from(other: NetworkVersion) -> Self {
        other.0
    }
}

impl From<NetworkVersion> for NetworkVersion_v2 {
    fn from(other: NetworkVersion) -> NetworkVersion_v2 {
        NetworkVersion_v2::try_from(u32::from(other.0)).unwrap()
    }
}
