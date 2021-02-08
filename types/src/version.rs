// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use encoding::repr::Serialize_repr;

/// Specifies the network version
#[derive(Debug, PartialEq, Clone, Copy, PartialOrd, Serialize_repr)]
#[repr(u32)]
pub enum NetworkVersion {
    /// genesis (specs-actors v0.9.3)
    V0,
    /// breeze (specs-actors v0.9.7)
    V1,
    /// smoke (specs-actors v0.9.8)
    V2,
    /// ignition (specs-actors v0.9.11)
    V3,
    /// actors v2 (specs-actors v2.0.x)
    V4,
    /// tape (increases max prove commit size by 10x)
    V5,
    /// kumquat (specs-actors v2.2.0)
    V6,
    /// calico (specs-actors v2.3.2)
    V7,
    /// persian (post-2.3.2 behaviour transition)
    V8,
    /// orange
    V9,
    /// actors v3 (specs-actors v3.0.x)
    V10,
    /// reserved
    V11,
}
