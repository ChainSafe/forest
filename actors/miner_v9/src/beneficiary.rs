use fvm_ipld_encoding::tuple::*;
use fvm_ipld_encoding::Cbor;
use fvm_shared::address::Address;

use fvm_shared::clock::ChainEpoch;
use fvm_shared::econ::TokenAmount;
use num_traits::Zero;
use std::ops::Sub;

#[derive(Debug, PartialEq, Eq, Clone, Serialize_tuple, Deserialize_tuple)]
pub struct BeneficiaryTerm {
    /// The total amount the current beneficiary can withdraw. Monotonic, but reset when beneficiary changes.
    pub quota: TokenAmount,
    /// The amount of quota the current beneficiary has already withdrawn
    pub used_quota: TokenAmount,
    /// The epoch at which the beneficiary's rights expire and revert to the owner
    pub expiration: ChainEpoch,
}

impl Cbor for BeneficiaryTerm {}

impl BeneficiaryTerm {
    pub fn default_value() -> BeneficiaryTerm {
        BeneficiaryTerm {
            quota: TokenAmount::zero(),
            expiration: 0,
            used_quota: TokenAmount::zero(),
        }
    }

    pub fn new(
        quota: TokenAmount,
        used_quota: TokenAmount,
        expiration: ChainEpoch,
    ) -> BeneficiaryTerm {
        BeneficiaryTerm {
            quota,
            expiration,
            used_quota,
        }
    }

    /// Get the amount that the beneficiary has not yet withdrawn
    /// return 0 when expired
    /// return 0 when the usedQuota >= Quota for safe
    /// otherwise Return quota-used_quota
    pub fn available(&self, cur: ChainEpoch) -> TokenAmount {
        if self.expiration > cur {
            (&self.quota).sub(&self.used_quota).max(TokenAmount::zero())
        } else {
            TokenAmount::zero()
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct PendingBeneficiaryChange {
    pub new_beneficiary: Address,
    pub new_quota: TokenAmount,
    pub new_expiration: ChainEpoch,
    pub approved_by_beneficiary: bool,
    pub approved_by_nominee: bool,
}

impl Cbor for PendingBeneficiaryChange {}

impl PendingBeneficiaryChange {
    pub fn new(
        new_beneficiary: Address,
        new_quota: TokenAmount,
        new_expiration: ChainEpoch,
    ) -> Self {
        PendingBeneficiaryChange {
            new_beneficiary,
            new_quota,
            new_expiration,
            approved_by_beneficiary: false,
            approved_by_nominee: false,
        }
    }
}
