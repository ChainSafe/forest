// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;

/// CodeID is the reference to the code which is attached to the Actor state.
/// There are builtin IDs and the option for custom code with a Cid
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum CodeID {
    Init,
    Cron,
    Account,
    Reward,
    PaymentChannel,
    StoragePower,
    StorageMarket,
    StorageMiner,
    System,
    Verifreg,
    Multisig,
    CustomCode(Cid),
}

// TODO define builtin Cids

impl CodeID {
    /// Returns true if cid is builtin Actor
    pub fn is_builtin(&self) -> bool {
        match *self {
            CodeID::CustomCode(_) => false,
            _ => true,
        }
    }
    /// Returns true if cid is singleton Actor
    pub fn is_singleton(&self) -> bool {
        match *self {
            CodeID::StorageMarket
            | CodeID::Init
            | CodeID::StoragePower
            | CodeID::Cron
            | CodeID::Reward
            | CodeID::System
            | CodeID::Verifreg => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use cid::Cid;

    #[test]
    fn builtin_checks() {
        // Tests all builtins will return true
        assert!(CodeID::Init.is_builtin());
        assert!(CodeID::System.is_builtin());
        assert!(CodeID::StorageMarket.is_builtin());
        assert!(CodeID::StoragePower.is_builtin());
        assert!(CodeID::Cron.is_builtin());
        assert!(CodeID::Account.is_builtin());
        assert!(CodeID::PaymentChannel.is_builtin());
        assert!(CodeID::StorageMiner.is_builtin());
        assert!(CodeID::Verifreg.is_builtin());
        assert!(CodeID::Multisig.is_builtin());
        assert!(CodeID::Reward.is_builtin());

        assert!(!CodeID::CustomCode(Cid::default()).is_builtin());
    }

    #[test]
    fn singleton_checks() {
        // singletons
        assert!(CodeID::Init.is_singleton());
        assert!(CodeID::StorageMarket.is_singleton());
        assert!(CodeID::StoragePower.is_singleton());
        assert!(CodeID::Verifreg.is_singleton());
        assert!(CodeID::Reward.is_singleton());
        assert!(CodeID::System.is_singleton());
        assert!(CodeID::Cron.is_singleton());
        // non-singletons
        assert!(!CodeID::Account.is_singleton());
        assert!(!CodeID::PaymentChannel.is_singleton());
        assert!(!CodeID::StorageMiner.is_singleton());
        assert!(!CodeID::Multisig.is_singleton());
        assert!(!CodeID::CustomCode(Cid::default()).is_singleton());
    }
}
