// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::sector::StoragePower;
use fil_actor_verifreg_state::v17::*;

/// Creates state decode params tests for the Verified Registry actor.
pub fn create_tests(tipset: &Tipset) -> Result<Vec<RpcTest>> {
    let verified_reg_constructor_params = ConstructorParams {
        root_key: Address::new_id(1000).into(),
    };

    let verified_reg_add_verifier_params = AddVerifierParams {
        address: Address::new_id(1234).into(),
        allowance: StoragePower::from(1048576u64), // 1MB
    };

    let verified_reg_remove_verifier_params = RemoveVerifierParams {
        verifier: Address::new_id(1234).into(),
    };

    let verified_reg_add_verified_client_params = AddVerifiedClientParams {
        address: Address::new_id(1235).into(),
        allowance: types::DataCap::from(2097152u64), // 2MB
    };

    let verified_reg_remove_data_cap_params = RemoveDataCapParams {
        verified_client_to_remove: Address::new_id(1236).into(),
        data_cap_amount_to_remove: types::DataCap::from(1048576u64),
        verifier_request_1: RemoveDataCapRequest {
            verifier: Address::new_id(1237).into(),
            signature: fvm_shared4::crypto::signature::Signature::new_bls(
                b"test_signature_1".to_vec(),
            ),
        },
        verifier_request_2: RemoveDataCapRequest {
            verifier: Address::new_id(1238).into(),
            signature: fvm_shared4::crypto::signature::Signature::new_secp256k1(
                b"test_signature_2".to_vec(),
            ),
        },
    };

    let verified_reg_remove_expired_allocations_params = RemoveExpiredAllocationsParams {
        client: 1239,
        allocation_ids: vec![1001, 1002, 1003],
    };

    let verified_reg_claim_allocations_params = ClaimAllocationsParams {
        sectors: vec![SectorAllocationClaims {
            sector: 42,
            expiry: 2000000,
            claims: vec![
                AllocationClaim {
                    client: 1240,
                    allocation_id: 2001,
                    data: Cid::default(),
                    size: fvm_shared4::piece::PaddedPieceSize(1024),
                },
                AllocationClaim {
                    client: 1241,
                    allocation_id: 2002,
                    data: Cid::default(),
                    size: fvm_shared4::piece::PaddedPieceSize(2048),
                },
            ],
        }],
        all_or_nothing: false,
    };

    let verified_reg_get_claims_params = GetClaimsParams {
        provider: 1242,
        claim_ids: vec![3001, 3002, 3003],
    };

    let verified_reg_extend_claim_terms_params = ExtendClaimTermsParams {
        terms: vec![ClaimTerm {
            provider: 12,
            claim_id: 12,
            term_max: 123,
        }],
    };

    let verified_reg_remove_expired_claims_params = RemoveExpiredClaimsParams {
        provider: 1243,
        claim_ids: vec![4001, 4002, 4003],
    };

    let verified_reg_universal_receiver_params =
        fvm_actor_utils::receiver::UniversalReceiverParams {
            type_: 42,
            payload: fvm_ipld_encoding::RawBytes::new(vec![0x12, 0x34, 0x56, 0x78]),
        };

    Ok(vec![
        RpcTest::identity(StateDecodeParams::request((
            Address::VERIFIED_REGISTRY_ACTOR,
            Method::Constructor as u64,
            to_vec(&verified_reg_constructor_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::VERIFIED_REGISTRY_ACTOR,
            Method::AddVerifier as u64,
            to_vec(&verified_reg_add_verifier_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::VERIFIED_REGISTRY_ACTOR,
            Method::RemoveVerifier as u64,
            to_vec(&verified_reg_remove_verifier_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::VERIFIED_REGISTRY_ACTOR,
            Method::AddVerifiedClient as u64,
            to_vec(&verified_reg_add_verified_client_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::VERIFIED_REGISTRY_ACTOR,
            Method::RemoveVerifiedClientDataCap as u64,
            to_vec(&verified_reg_remove_data_cap_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::VERIFIED_REGISTRY_ACTOR,
            Method::RemoveExpiredAllocations as u64,
            to_vec(&verified_reg_remove_expired_allocations_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::VERIFIED_REGISTRY_ACTOR,
            Method::ClaimAllocations as u64,
            to_vec(&verified_reg_claim_allocations_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::VERIFIED_REGISTRY_ACTOR,
            Method::GetClaims as u64,
            to_vec(&verified_reg_get_claims_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::VERIFIED_REGISTRY_ACTOR,
            Method::ExtendClaimTerms as u64,
            to_vec(&verified_reg_extend_claim_terms_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::VERIFIED_REGISTRY_ACTOR,
            Method::RemoveExpiredClaims as u64,
            to_vec(&verified_reg_remove_expired_claims_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::VERIFIED_REGISTRY_ACTOR,
            Method::AddVerifiedClientExported as u64,
            to_vec(&verified_reg_add_verified_client_params)?, // reuse same params
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::VERIFIED_REGISTRY_ACTOR,
            Method::RemoveExpiredAllocationsExported as u64,
            to_vec(&verified_reg_remove_expired_allocations_params)?, // reuse same params
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::VERIFIED_REGISTRY_ACTOR,
            Method::GetClaimsExported as u64,
            to_vec(&verified_reg_get_claims_params)?, // reuse same params
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::VERIFIED_REGISTRY_ACTOR,
            Method::ExtendClaimTermsExported as u64,
            to_vec(&verified_reg_extend_claim_terms_params)?, // reuse same params
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::VERIFIED_REGISTRY_ACTOR,
            Method::RemoveExpiredClaimsExported as u64,
            to_vec(&verified_reg_remove_expired_claims_params)?, // reuse same params
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::VERIFIED_REGISTRY_ACTOR,
            Method::UniversalReceiverHook as u64,
            to_vec(&verified_reg_universal_receiver_params)?,
            tipset.key().into(),
        ))?),
    ])
}
