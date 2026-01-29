// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod actors;
pub mod address;
pub mod bigint;
pub mod clock;
pub mod crypto;
pub mod deal;
pub mod econ;
pub mod error;
pub mod executor;
pub mod externs;
pub mod gas;
pub mod kernel;
pub mod machine;
pub mod message;
pub mod piece;
pub mod policy;
pub mod randomness;
pub mod runtime;
pub mod sector;
pub mod state_tree;
pub mod state_tree_v0;
pub mod trace;
pub mod version;

pub mod fvm_shared_latest {
    // If `#[doc(inline)]`, we steal these docs from an external crate.
    // But they contain dead links, which means our dead link checker (lychee)
    // will complain.
    #[doc(no_inline)]
    pub use fvm_shared4::*;
}
pub mod fvm_latest {
    pub use fvm4::*;
}

pub type MethodNum = fvm_shared_latest::MethodNum;
