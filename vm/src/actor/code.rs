use crate::actor::{Error, MethodNum, MethodParams};
use crate::runtime::{InvocOutput, Runtime};
use cid::Cid;

/// CodeID is the reference to the code which is attached to the Actor state.
/// There are builtin IDs and the option for custom code with a Cid
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum CodeID {
    Init,
    Cron,
    Account,
    PaymentChannel,
    StoragePower,
    StorageMiner,
    StorageMarket,
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
            CodeID::StorageMarket | CodeID::Init | CodeID::StoragePower => true,
            _ => false,
        }
    }
}

pub trait ActorCode {
    fn invoke_method(
        rt: &dyn Runtime,
        method: MethodNum,
        params: &MethodParams,
    ) -> Result<InvocOutput, Error>;
}

#[cfg(test)]
mod test {
    use super::*;
    use cid::{Cid, Codec, Version};

    #[test]
    fn builtin_checks() {
        // Tests all builtins will return true
        assert!(CodeID::Init.is_builtin());
        assert!(CodeID::StorageMarket.is_builtin());
        assert!(CodeID::StoragePower.is_builtin());
        assert!(CodeID::Cron.is_builtin());
        assert!(CodeID::Account.is_builtin());
        assert!(CodeID::PaymentChannel.is_builtin());
        assert!(CodeID::StorageMiner.is_builtin());

        assert!(
            !CodeID::CustomCode(Cid::new(Codec::DagProtobuf, Version::V1, &[0u8])).is_builtin()
        );
    }

    #[test]
    fn singleton_checks() {
        // singletons
        assert!(CodeID::Init.is_singleton());
        assert!(CodeID::StorageMarket.is_singleton());
        assert!(CodeID::StoragePower.is_singleton());
        // non-singletons
        assert!(!CodeID::Cron.is_singleton());
        assert!(!CodeID::Account.is_singleton());
        assert!(!CodeID::PaymentChannel.is_singleton());
        assert!(!CodeID::StorageMiner.is_singleton());
        assert!(
            !CodeID::CustomCode(Cid::new(Codec::DagProtobuf, Version::V1, &[0u8])).is_singleton()
        );
    }
}
