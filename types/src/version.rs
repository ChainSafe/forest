// Copyright 2019-2022 ChainSafe Systems
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
    /// actors v2 (specs-actors v2.0.3)
    V4,
    /// tape (specs-actors v2.1.0)
    V5,
    /// kumquat (specs-actors v2.2.0)
    V6,
    /// calico (specs-actors v2.3.2)
    V7,
    /// persian (post-2.3.2 behaviour transition)
    V8,
    /// orange (post-2.3.2 behaviour transition)
    V9,
    /// trust (specs-actors v3.0.1)
    V10,
    /// norwegian (specs-actors v3.1.0)
    V11,
    /// turbo (specs-actors v4.0.0)
    V12,
    /// hyperdrive (specs-actors v5.0.1)
    V13,
    /// chocolate (specs-actors v6.0.0)
    V14,
}
