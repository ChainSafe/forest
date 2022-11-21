// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use frc46_token::receiver::types::{FRC46TokenReceived, UniversalReceiverParams, FRC46_TOKEN_TYPE};
use frc46_token::token::types::{BurnParams, TransferParams};
use frc46_token::token::TOKEN_PRECISION;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::RawBytes;
use fvm_ipld_hamt::BytesKey;
use fvm_shared::address::Address;
use fvm_shared::bigint::bigint_ser::BigIntDe;
use fvm_shared::bigint::BigInt;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::econ::TokenAmount;
use fvm_shared::error::ExitCode;
use fvm_shared::{ActorID, HAMT_BIT_WIDTH, METHOD_CONSTRUCTOR};
use log::info;
use num_derive::FromPrimitive;
use num_traits::{Signed, Zero};

use fil_actors_runtime_v9::cbor::{deserialize, serialize};
use fil_actors_runtime_v9::runtime::builtins::Type;
use fil_actors_runtime_v9::runtime::{Policy, Runtime};
use fil_actors_runtime_v9::{
    actor_error, make_map_with_root_and_bitwidth, resolve_to_actor_id, ActorDowncast, ActorError,
    BatchReturn, Map, DATACAP_TOKEN_ACTOR_ADDR, STORAGE_MARKET_ACTOR_ADDR, SYSTEM_ACTOR_ADDR,
    VERIFIED_REGISTRY_ACTOR_ADDR,
};
use fil_actors_runtime_v9::{ActorContext, AsActorError, BatchReturnGen};

use crate::ext::datacap::{DestroyParams, MintParams};

pub use self::state::Allocation;
pub use self::state::Claim;
pub use self::state::State;
pub use self::types::*;

#[cfg(feature = "fil-actor")]
fil_actors_runtime::wasm_trampoline!(Actor);

pub mod expiration;
pub mod ext;
pub mod state;
pub mod testing;
pub mod types;

/// Account actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    AddVerifier = 2,
    RemoveVerifier = 3,
    AddVerifiedClient = 4,
    // UseBytes = 5,     // Deprecated
    // RestoreBytes = 6, // Deprecated
    RemoveVerifiedClientDataCap = 7,
    RemoveExpiredAllocations = 8,
    ClaimAllocations = 9,
    GetClaims = 10,
    ExtendClaimTerms = 11,
    RemoveExpiredClaims = 12,
    UniversalReceiverHook = frc42_dispatch::method_hash!("Receive"),
}

pub struct Actor;

impl Actor {
    /// Constructor for Registry Actor
    pub fn constructor<BS, RT>(rt: &mut RT, root_key: Address) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&SYSTEM_ACTOR_ADDR))?;

        // root should be an ID address
        let id_addr = rt.resolve_address(&root_key).context_code(
            ExitCode::USR_ILLEGAL_ARGUMENT,
            "root should be an ID address",
        )?;

        let st = State::new(rt.store(), Address::new_id(id_addr))
            .context("failed to create verifreg state")?;

        rt.create(&st)?;
        Ok(())
    }

    pub fn add_verifier<BS, RT>(rt: &mut RT, params: AddVerifierParams) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        if params.allowance < rt.policy().minimum_verified_allocation_size {
            return Err(actor_error!(
                illegal_argument,
                "Allowance {} below minimum deal size for add verifier {}",
                params.allowance,
                params.address
            ));
        }

        let verifier = resolve_to_actor_id(rt, &params.address)?;

        let verifier = Address::new_id(verifier);

        let st: State = rt.state()?;
        rt.validate_immediate_caller_is(std::iter::once(&st.root_key))?;

        // Disallow root as a verifier.
        if verifier == st.root_key {
            return Err(actor_error!(
                illegal_argument,
                "Rootkey cannot be added as verifier"
            ));
        }

        // Disallow existing clients as verifiers.
        let token_balance = balance_of(rt, &verifier)?;
        if token_balance.is_positive() {
            return Err(actor_error!(
                illegal_argument,
                "verified client {} cannot become a verifier",
                verifier
            ));
        }

        // Store the new verifier and allowance (over-writing).
        rt.transaction(|st: &mut State, rt| {
            st.put_verifier(rt.store(), &verifier, &params.allowance)
                .context("failed to add verifier")
        })
    }

    pub fn remove_verifier<BS, RT>(rt: &mut RT, verifier_addr: Address) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        let verifier = resolve_to_actor_id(rt, &verifier_addr)?;
        let verifier = Address::new_id(verifier);

        let state: State = rt.state()?;
        rt.validate_immediate_caller_is(std::iter::once(&state.root_key))?;

        rt.transaction(|st: &mut State, rt| {
            st.remove_verifier(rt.store(), &verifier)
                .context("failed to remove verifier")
        })
    }

    pub fn add_verified_client<BS, RT>(
        rt: &mut RT,
        params: AddVerifierClientParams,
    ) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        // The caller will be verified by checking table below
        rt.validate_immediate_caller_accept_any()?;

        if params.allowance < rt.policy().minimum_verified_allocation_size {
            return Err(actor_error!(
                illegal_argument,
                "allowance {} below MinVerifiedDealSize for add verified client {}",
                params.allowance,
                params.address
            ));
        }

        let client = resolve_to_actor_id(rt, &params.address)?;
        let client = Address::new_id(client);

        let st: State = rt.state()?;
        if client == st.root_key {
            return Err(actor_error!(
                illegal_argument,
                "root cannot be added as client"
            ));
        }

        // Validate caller is one of the verifiers, i.e. has an allowance (even if zero).
        let verifier = rt.message().caller();
        let verifier_cap = st
            .get_verifier_cap(rt.store(), &verifier)?
            .ok_or_else(|| actor_error!(not_found, "caller {} is not a verifier", verifier))?;

        // Disallow existing verifiers as clients.
        if st.get_verifier_cap(rt.store(), &client)?.is_some() {
            return Err(actor_error!(
                illegal_argument,
                "verifier {} cannot be added as a verified client",
                client
            ));
        }

        // Compute new verifier allowance.
        if verifier_cap < params.allowance {
            return Err(actor_error!(
                illegal_argument,
                "add more DataCap {} for client than allocated {}",
                params.allowance,
                verifier_cap
            ));
        }

        // Reduce verifier's cap.
        let new_verifier_cap = verifier_cap - &params.allowance;
        rt.transaction(|st: &mut State, rt| {
            st.put_verifier(rt.store(), &verifier, &new_verifier_cap)
                .context("failed to update verifier allowance")
        })?;

        // Credit client token allowance.
        let operators = vec![STORAGE_MARKET_ACTOR_ADDR];
        mint(rt, &client, &params.allowance, operators).context(format!(
            "failed to mint {} data cap to client {}",
            &params.allowance, client
        ))?;
        Ok(())
    }

    /// Removes DataCap allocated to a verified client.
    pub fn remove_verified_client_data_cap<BS, RT>(
        rt: &mut RT,
        params: RemoveDataCapParams,
    ) -> Result<RemoveDataCapReturn, ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        let client = resolve_to_actor_id(rt, &params.verified_client_to_remove)?;
        let client = Address::new_id(client);

        let verifier_1 = resolve_to_actor_id(rt, &params.verifier_request_1.verifier)?;
        let verifier_1 = Address::new_id(verifier_1);

        let verifier_2 = resolve_to_actor_id(rt, &params.verifier_request_2.verifier)?;
        let verifier_2 = Address::new_id(verifier_2);

        if verifier_1 == verifier_2 {
            return Err(actor_error!(
                illegal_argument,
                "need two different verifiers to send remove datacap request"
            ));
        }

        // Validate and then remove the proposal.
        rt.transaction(|st: &mut State, rt| {
            rt.validate_immediate_caller_is(std::iter::once(&st.root_key))?;

            if params.verified_client_to_remove == VERIFIED_REGISTRY_ACTOR_ADDR {
                return Err(actor_error!(
                    illegal_argument,
                    "cannot remove data cap from verified registry itself"
                ));
            }

            if !is_verifier(rt, st, verifier_1)? {
                return Err(actor_error!(not_found, "{} is not a verifier", verifier_1));
            }

            if !is_verifier(rt, st, verifier_2)? {
                return Err(actor_error!(not_found, "{} is not a verifier", verifier_2));
            }

            // validate signatures
            let mut proposal_ids = make_map_with_root_and_bitwidth::<_, RemoveDataCapProposalID>(
                &st.remove_data_cap_proposal_ids,
                rt.store(),
                HAMT_BIT_WIDTH,
            )
            .map_err(|e| {
                e.downcast_default(
                    ExitCode::USR_ILLEGAL_STATE,
                    "failed to load datacap removal proposal ids",
                )
            })?;

            let verifier_1_id = use_proposal_id(&mut proposal_ids, verifier_1, client)?;
            let verifier_2_id = use_proposal_id(&mut proposal_ids, verifier_2, client)?;

            remove_data_cap_request_is_valid(
                rt,
                &params.verifier_request_1,
                verifier_1_id,
                &params.data_cap_amount_to_remove,
                client,
            )?;
            remove_data_cap_request_is_valid(
                rt,
                &params.verifier_request_2,
                verifier_2_id,
                &params.data_cap_amount_to_remove,
                client,
            )?;

            st.remove_data_cap_proposal_ids = proposal_ids
                .flush()
                .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to flush proposal ids")?;
            Ok(())
        })?;

        // Burn the client's data cap tokens.
        let balance = balance_of(rt, &client).context("failed to fetch balance")?;
        let burnt = std::cmp::min(balance, params.data_cap_amount_to_remove);
        destroy(rt, &client, &burnt).context(format!(
            "failed to destroy {} from allowance for {}",
            &burnt, &client
        ))?;

        Ok(RemoveDataCapReturn {
            verified_client: client, // Changed to the resolved address
            data_cap_removed: burnt,
        })
    }

    // An allocation may be removed after its expiration epoch has passed (by anyone).
    // When removed, the DataCap tokens are transferred back to the client.
    // If no allocations are specified, all eligible allocations are removed.
    pub fn remove_expired_allocations<BS, RT>(
        rt: &mut RT,
        params: RemoveExpiredAllocationsParams,
    ) -> Result<RemoveExpiredAllocationsReturn, ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        // Since the allocations are expired, this is safe to be called by anyone.
        rt.validate_immediate_caller_accept_any()?;
        let curr_epoch = rt.curr_epoch();
        let mut batch_ret = BatchReturn::empty();
        let mut considered = Vec::<ClaimID>::new();
        let mut recovered_datacap = DataCap::zero();
        let recovered_datacap = rt
            .transaction(|st: &mut State, rt| {
                let mut allocs = st.load_allocs(rt.store())?;

                let to_remove: Vec<AllocationID>;
                if params.allocation_ids.is_empty() {
                    // Find all expired allocations for the client.
                    considered = expiration::find_expired(&mut allocs, params.client, curr_epoch)?;
                    batch_ret = BatchReturn::ok(considered.len() as u32);
                    to_remove = considered.clone();
                } else {
                    considered = params.allocation_ids.clone();
                    batch_ret = expiration::check_expired(
                        &mut allocs,
                        &params.allocation_ids,
                        params.client,
                        curr_epoch,
                    )?;
                    to_remove = batch_ret.successes(&params.allocation_ids);
                }

                for id in to_remove {
                    let existing = allocs.remove(params.client, id).context_code(
                        ExitCode::USR_ILLEGAL_STATE,
                        format!("failed to remove allocation {}", id),
                    )?;
                    // Unwrapping here as both paths to here should ensure the allocation exists.
                    recovered_datacap += existing.unwrap().size.0;
                }

                st.save_allocs(&mut allocs)?;
                Ok(recovered_datacap)
            })
            .context("state transaction failed")?;

        // Transfer the recovered datacap back to the client.
        transfer(rt, params.client, &recovered_datacap).with_context(|| {
            format!(
                "failed to transfer recovered datacap {} back to client {}",
                &recovered_datacap, params.client
            )
        })?;

        Ok(RemoveExpiredAllocationsReturn {
            considered,
            results: batch_ret,
            datacap_recovered: recovered_datacap,
        })
    }

    // Called by storage provider actor to claim allocations for data provably committed to storage.
    // For each allocation claim, the registry checks that the provided piece CID
    // and size match that of the allocation.
    pub fn claim_allocations<BS, RT>(
        rt: &mut RT,
        params: ClaimAllocationsParams,
    ) -> Result<ClaimAllocationsReturn, ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&Type::Miner))?;
        let provider = rt.message().caller().id().unwrap();
        if params.sectors.is_empty() {
            return Err(actor_error!(
                illegal_argument,
                "claim allocations called with no claims"
            ));
        }
        let mut datacap_claimed = DataCap::zero();
        let mut ret_gen = BatchReturnGen::new(params.sectors.len());
        let all_or_nothing = params.all_or_nothing;
        rt.transaction(|st: &mut State, rt| {
            let mut claims = st.load_claims(rt.store())?;
            let mut allocs = st.load_allocs(rt.store())?;

            for claim_alloc in params.sectors {
                let maybe_alloc = state::get_allocation(
                    &mut allocs,
                    claim_alloc.client,
                    claim_alloc.allocation_id,
                )?;
                let alloc: &Allocation = match maybe_alloc {
                    None => {
                        ret_gen.add_fail(ExitCode::USR_NOT_FOUND);
                        info!(
                            "no allocation {} for client {}",
                            claim_alloc.allocation_id, claim_alloc.client,
                        );
                        continue;
                    }
                    Some(a) => a,
                };

                if !can_claim_alloc(&claim_alloc, provider, alloc, rt.curr_epoch()) {
                    ret_gen.add_fail(ExitCode::USR_FORBIDDEN);
                    info!(
                        "invalid sector {:?} for allocation {}",
                        claim_alloc.sector, claim_alloc.allocation_id,
                    );
                    continue;
                }

                let new_claim = Claim {
                    provider,
                    client: alloc.client,
                    data: alloc.data,
                    size: alloc.size,
                    term_min: alloc.term_min,
                    term_max: alloc.term_max,
                    term_start: rt.curr_epoch(),
                    sector: claim_alloc.sector,
                };

                let inserted = claims
                    .put_if_absent(provider, claim_alloc.allocation_id, new_claim)
                    .context_code(
                        ExitCode::USR_ILLEGAL_STATE,
                        format!("failed to write claim {}", claim_alloc.allocation_id),
                    )?;
                if !inserted {
                    ret_gen.add_fail(ExitCode::USR_ILLEGAL_STATE); // should be unreachable since claim and alloc can't exist at once
                    info!(
                        "claim for allocation {} could not be inserted as it already exists",
                        claim_alloc.allocation_id,
                    );
                    continue;
                }

                allocs
                    .remove(claim_alloc.client, claim_alloc.allocation_id)
                    .context_code(
                        ExitCode::USR_ILLEGAL_STATE,
                        format!("failed to remove allocation {}", claim_alloc.allocation_id),
                    )?;

                datacap_claimed += DataCap::from(claim_alloc.size.0);
                ret_gen.add_success();
            }
            st.save_allocs(&mut allocs)?;
            st.save_claims(&mut claims)?;
            Ok(())
        })
        .context("state transaction failed")?;
        let batch_info = ret_gen.gen();
        if all_or_nothing && !batch_info.all_ok() {
            return Err(actor_error!(
                illegal_argument,
                "all or nothing call contained failures: {}",
                batch_info.to_string()
            ));
        }

        // Burn the datacap tokens from verified registry's own balance.
        burn(rt, &datacap_claimed)?;

        Ok(ClaimAllocationsReturn {
            batch_info,
            claimed_space: datacap_claimed,
        })
    }

    // get claims for a provider
    pub fn get_claims<BS, RT>(
        rt: &mut RT,
        params: GetClaimsParams,
    ) -> Result<GetClaimsReturn, ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;
        let mut batch_gen = BatchReturnGen::new(params.claim_ids.len());
        let claims = rt
            .transaction(|st: &mut State, rt| {
                let mut st_claims = st.load_claims(rt.store())?;
                let mut ret_claims = Vec::new();
                for id in params.claim_ids {
                    let maybe_claim = state::get_claim(&mut st_claims, params.provider, id)?;
                    match maybe_claim {
                        None => {
                            batch_gen.add_fail(ExitCode::USR_NOT_FOUND);
                            info!("no claim {} for provider {}", id, params.provider,);
                        }
                        Some(claim) => {
                            batch_gen.add_success();
                            ret_claims.push(claim.clone());
                        }
                    };
                }
                Ok(ret_claims)
            })
            .context("state transaction failed")?;
        Ok(GetClaimsReturn {
            batch_info: batch_gen.gen(),
            claims,
        })
    }

    /// Extends the maximum term of some claims up to the largest value they could have been
    /// originally allocated.
    /// Callable only by the claims' client.
    /// Cannot reduce a claim's term.
    /// Can extend the term even if the claim has already expired.
    /// Note that this method can't extend the term past the original limit,
    /// even if the term has previously been extended past that by spending new datacap.
    pub fn extend_claim_terms<BS, RT>(
        rt: &mut RT,
        params: ExtendClaimTermsParams,
    ) -> Result<ExtendClaimTermsReturn, ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        // Permissions are checked per-claim.
        rt.validate_immediate_caller_accept_any()?;
        let caller_id = rt.message().caller().id().unwrap();
        let term_limit = rt.policy().maximum_verified_allocation_term;
        let mut batch_gen = BatchReturnGen::new(params.terms.len());
        rt.transaction(|st: &mut State, rt| {
            let mut st_claims = st.load_claims(rt.store())?;
            for term in params.terms {
                // Confirm the new term limit is allowed.
                if term.term_max > term_limit {
                    batch_gen.add_fail(ExitCode::USR_ILLEGAL_ARGUMENT);
                    info!(
                        "term_max {} for claim {} exceeds maximum {}",
                        term.term_max, term.claim_id, term_limit,
                    );
                    continue;
                }

                let maybe_claim = state::get_claim(&mut st_claims, term.provider, term.claim_id)?;
                if let Some(claim) = maybe_claim {
                    // Confirm the caller is the claim's client.
                    if claim.client != caller_id {
                        batch_gen.add_fail(ExitCode::USR_FORBIDDEN);
                        info!(
                            "client {} for claim {} does not match caller {}",
                            claim.client, term.claim_id, caller_id,
                        );
                        continue;
                    }
                    // Confirm the new term limit is no less than the old one.
                    if term.term_max < claim.term_max {
                        batch_gen.add_fail(ExitCode::USR_ILLEGAL_ARGUMENT);
                        info!(
                            "term_max {} for claim {} is less than current {}",
                            term.term_max, term.claim_id, claim.term_max,
                        );
                        continue;
                    }

                    let new_claim = Claim {
                        term_max: term.term_max,
                        ..*claim
                    };
                    st_claims
                        .put(term.provider, term.claim_id, new_claim)
                        .context_code(
                            ExitCode::USR_ILLEGAL_STATE,
                            "HAMT put failure storing new claims",
                        )?;
                    batch_gen.add_success();
                } else {
                    batch_gen.add_fail(ExitCode::USR_NOT_FOUND);
                    info!("no claim {} for provider {}", term.claim_id, term.provider);
                }
            }
            st.save_claims(&mut st_claims)?;
            Ok(())
        })
        .context("state transaction failed")?;
        Ok(batch_gen.gen())
    }

    // A claim may be removed after its maximum term has elapsed (by anyone).
    // If no claims are specified, all eligible claims are removed.
    pub fn remove_expired_claims<BS, RT>(
        rt: &mut RT,
        params: RemoveExpiredClaimsParams,
    ) -> Result<RemoveExpiredClaimsReturn, ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        // Since the claims are expired, this is safe to be called by anyone.
        rt.validate_immediate_caller_accept_any()?;
        let curr_epoch = rt.curr_epoch();
        let mut batch_ret = BatchReturn::empty();
        let mut considered = Vec::<ClaimID>::new();
        rt.transaction(|st: &mut State, rt| {
            let mut claims = st.load_claims(rt.store())?;
            let to_remove: Vec<ClaimID>;
            if params.claim_ids.is_empty() {
                // Find all expired claims for the provider.
                considered = expiration::find_expired(&mut claims, params.provider, curr_epoch)?;
                batch_ret = BatchReturn::ok(considered.len() as u32);
                to_remove = considered.clone();
            } else {
                considered = params.claim_ids.clone();
                batch_ret = expiration::check_expired(
                    &mut claims,
                    &params.claim_ids,
                    params.provider,
                    curr_epoch,
                )?;
                to_remove = batch_ret.successes(&params.claim_ids);
            }

            for id in to_remove {
                claims.remove(params.provider, id).context_code(
                    ExitCode::USR_ILLEGAL_STATE,
                    format!("failed to remove claim {}", id),
                )?;
            }

            st.save_claims(&mut claims)?;
            Ok(())
        })
        .context("state transaction failed")?;

        Ok(RemoveExpiredClaimsReturn {
            considered,
            results: batch_ret,
        })
    }

    // Receives data cap tokens (only) and creates allocations according to one or more
    // allocation requests specified in the transfer's operator data.
    // The token amount received must exactly correspond to the sum of the requested allocation sizes.
    // This method does not support partial success (yet): all allocations must succeed,
    // or the transfer will be rejected.
    // Returns the ids of the created allocations.
    pub fn universal_receiver_hook<BS, RT>(
        rt: &mut RT,
        params: UniversalReceiverParams,
    ) -> Result<AllocationsResponse, ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        // Accept only the data cap token.
        rt.validate_immediate_caller_is(&[DATACAP_TOKEN_ACTOR_ADDR])?;

        let my_id = rt.message().receiver().id().unwrap();
        let curr_epoch = rt.curr_epoch();

        // Validate receiver hook payload.
        let tokens_received = validate_tokens_received(&params, my_id)?;
        let client = tokens_received.from;

        // Extract and validate allocation request from the operator data.
        let reqs: AllocationRequests =
            deserialize(&tokens_received.operator_data, "allocation requests")?;
        let mut datacap_total = DataCap::zero();

        // Construct new allocation records.
        let mut new_allocs = Vec::with_capacity(reqs.allocations.len());
        for req in &reqs.allocations {
            validate_new_allocation(req, rt.policy(), curr_epoch)?;
            // Require the provider for new allocations to be a miner actor.
            // This doesn't matter much, but is more ergonomic to fail rather than lock up datacap.
            let provider_id = resolve_miner_id(rt, &req.provider)?;
            new_allocs.push(Allocation {
                client,
                provider: provider_id,
                data: req.data,
                size: req.size,
                term_min: req.term_min,
                term_max: req.term_max,
                expiration: req.expiration,
            });
            datacap_total += DataCap::from(req.size.0);
        }

        let st: State = rt.state()?;
        let mut claims = st.load_claims(rt.store())?;
        let mut updated_claims = Vec::<(ClaimID, Claim)>::new();
        let mut extension_total = DataCap::zero();
        for req in &reqs.extensions {
            // Note: we don't check the client address here, by design.
            // Any client can spend datacap to extend an existing claim.
            let provider_id = rt
                .resolve_address(&req.provider)
                .with_context_code(ExitCode::USR_ILLEGAL_ARGUMENT, || {
                    format!("failed to resolve provider address {}", req.provider)
                })?;
            let claim = state::get_claim(&mut claims, provider_id, req.claim)?
                .with_context_code(ExitCode::USR_NOT_FOUND, || {
                    format!("no claim {} for provider {}", req.claim, provider_id)
                })?;
            let policy = rt.policy();

            validate_claim_extension(req, claim, policy, curr_epoch)?;
            // The claim's client is not changed to be the address of the token sender.
            // It remains the original allocation client.
            updated_claims.push((
                req.claim,
                Claim {
                    term_max: req.term_max,
                    ..*claim
                },
            ));
            datacap_total += DataCap::from(claim.size.0);
            extension_total += DataCap::from(claim.size.0);
        }

        // Allocation size must match the tokens received exactly (we don't return change).
        let tokens_as_datacap = tokens_to_datacap(&tokens_received.amount);
        if datacap_total != tokens_as_datacap {
            return Err(actor_error!(
                illegal_argument,
                "total allocation size {} must match data cap amount received {}",
                datacap_total,
                tokens_as_datacap
            ));
        }

        // Burn the received datacap tokens spent on extending existing claims.
        // The tokens spent on new allocations will be burnt when claimed later, or refunded.
        burn(rt, &extension_total)?;

        // Partial success isn't supported yet, but these results make space for it in the future.
        let allocation_results = BatchReturn::ok(new_allocs.len() as u32);
        let extension_results = BatchReturn::ok(updated_claims.len() as u32);

        // Save new allocations and updated claims.
        let ids = rt.transaction(|st: &mut State, rt| {
            let ids = st.insert_allocations(rt.store(), client, new_allocs.into_iter())?;
            st.put_claims(rt.store(), updated_claims.into_iter())?;
            Ok(ids)
        })?;

        Ok(AllocationsResponse {
            allocation_results,
            extension_results,
            new_allocations: ids,
        })
    }
}

// Checks whether an address has a verifier entry (which could be zero).
fn is_verifier<BS, RT>(rt: &RT, st: &State, address: Address) -> Result<bool, ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    let verifiers =
        make_map_with_root_and_bitwidth::<_, BigIntDe>(&st.verifiers, rt.store(), HAMT_BIT_WIDTH)
            .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to load verifiers")?;

    // check that the `address` is currently a verified client
    let found = verifiers
        .contains_key(&address.to_bytes())
        .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to get verifier")?;

    Ok(found)
}

// Invokes BalanceOf on the data cap token actor, and converts the result to whole units of data cap.
fn balance_of<BS, RT>(rt: &mut RT, owner: &Address) -> Result<DataCap, ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    let params = serialize(owner, "owner address")?;
    let ret = rt
        .send(
            &DATACAP_TOKEN_ACTOR_ADDR,
            ext::datacap::Method::BalanceOf as u64,
            params,
            TokenAmount::zero(),
        )
        .context(format!("failed to query datacap balance of {}", owner))?;
    let x: TokenAmount = deserialize(&ret, "balance result")?;
    Ok(tokens_to_datacap(&x))
}

// Invokes Mint on a data cap token actor for whole units of data cap.
fn mint<BS, RT>(
    rt: &mut RT,
    to: &Address,
    amount: &DataCap,
    operators: Vec<Address>,
) -> Result<(), ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    let token_amt = datacap_to_tokens(amount);
    let params = MintParams {
        to: *to,
        amount: token_amt,
        operators,
    };
    rt.send(
        &DATACAP_TOKEN_ACTOR_ADDR,
        ext::datacap::Method::Mint as u64,
        serialize(&params, "mint params")?,
        TokenAmount::zero(),
    )
    .context(format!("failed to send mint {:?} to datacap", params))?;
    Ok(())
}

// Invokes Burn on a data cap token actor for whole units of data cap.
fn burn<BS, RT>(rt: &mut RT, amount: &DataCap) -> Result<(), ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    if amount.is_zero() {
        return Ok(());
    }

    let token_amt = datacap_to_tokens(amount);
    let params = BurnParams { amount: token_amt };
    rt.send(
        &DATACAP_TOKEN_ACTOR_ADDR,
        ext::datacap::Method::Burn as u64,
        serialize(&params, "burn params")?,
        TokenAmount::zero(),
    )
    .context(format!("failed to send burn {:?} to datacap", params))?;
    // The burn return value gives the new balance, but it's dropped here.
    // This also allows the check for zero burns inside this method.
    Ok(())
}

// Invokes Destroy on a data cap token actor for whole units of data cap.
fn destroy<BS, RT>(rt: &mut RT, owner: &Address, amount: &DataCap) -> Result<(), ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    if amount.is_zero() {
        return Ok(());
    }
    let token_amt = datacap_to_tokens(amount);
    let params = DestroyParams {
        owner: *owner,
        amount: token_amt,
    };
    rt.send(
        &DATACAP_TOKEN_ACTOR_ADDR,
        ext::datacap::Method::Destroy as u64,
        serialize(&params, "destroy params")?,
        TokenAmount::zero(),
    )
    .context(format!("failed to send destroy {:?} to datacap", params))?;
    Ok(())
}

// Invokes transfer on a data cap token actor for whole units of data cap.
fn transfer<BS, RT>(rt: &mut RT, to: ActorID, amount: &DataCap) -> Result<(), ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    let token_amt = datacap_to_tokens(amount);
    let params = TransferParams {
        to: Address::new_id(to),
        amount: token_amt,
        operator_data: Default::default(),
    };
    rt.send(
        &DATACAP_TOKEN_ACTOR_ADDR,
        ext::datacap::Method::Transfer as u64,
        serialize(&params, "transfer params")?,
        TokenAmount::zero(),
    )
    .context(format!("failed to send transfer to datacap {:?}", params))?;
    Ok(())
}

fn datacap_to_tokens(amount: &DataCap) -> TokenAmount {
    TokenAmount::from_atto(amount.clone()) * TOKEN_PRECISION
}

fn tokens_to_datacap(amount: &TokenAmount) -> BigInt {
    amount.atto() / TOKEN_PRECISION
}

fn use_proposal_id<BS>(
    proposal_ids: &mut Map<BS, RemoveDataCapProposalID>,
    verifier: Address,
    client: Address,
) -> Result<RemoveDataCapProposalID, ActorError>
where
    BS: Blockstore,
{
    let key = AddrPairKey::new(verifier, client);

    let maybe_id = proposal_ids.get(&key.to_bytes()).map_err(|e| {
        actor_error!(
            illegal_state,
            "failed to get proposal id for verifier {} and client {}: {}",
            verifier,
            client,
            e
        )
    })?;

    let curr_id = if let Some(RemoveDataCapProposalID { id }) = maybe_id {
        RemoveDataCapProposalID { id: *id }
    } else {
        RemoveDataCapProposalID { id: 0 }
    };

    let next_id = RemoveDataCapProposalID { id: curr_id.id + 1 };
    proposal_ids
        .set(BytesKey::from(key.to_bytes()), next_id)
        .map_err(|e| {
            actor_error!(
                illegal_state,
                "failed to update proposal id for verifier {} and client {}: {}",
                verifier,
                client,
                e
            )
        })?;

    Ok(curr_id)
}

fn remove_data_cap_request_is_valid<BS, RT>(
    rt: &RT,
    request: &RemoveDataCapRequest,
    id: RemoveDataCapProposalID,
    to_remove: &DataCap,
    client: Address,
) -> Result<(), ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    let proposal = RemoveDataCapProposal {
        removal_proposal_id: id,
        data_cap_amount: to_remove.clone(),
        verified_client: client,
    };

    let b = RawBytes::serialize(proposal).map_err(|e| {
        actor_error!(
                serialization; "failed to marshal remove datacap request: {}", e)
    })?;

    let payload = [SIGNATURE_DOMAIN_SEPARATION_REMOVE_DATA_CAP, b.bytes()].concat();

    // verify signature of proposal
    rt.verify_signature(&request.signature, &request.verifier, &payload).map_err(
        |e| actor_error!(illegal_argument; "invalid signature for datacap removal request: {}", e),
    )
}

// Deserializes and validates a receiver hook payload, expecting only an FRC-46 transfer.
fn validate_tokens_received(
    params: &UniversalReceiverParams,
    my_id: u64,
) -> Result<FRC46TokenReceived, ActorError> {
    if params.type_ != FRC46_TOKEN_TYPE {
        return Err(actor_error!(
            illegal_argument,
            "invalid token type {}, expected {} (FRC-46)",
            params.type_,
            FRC46_TOKEN_TYPE
        ));
    }
    let payload: FRC46TokenReceived = deserialize(&params.payload, "receiver hook payload")?;
    // Payload to address must match receiving actor.
    if payload.to != my_id {
        return Err(actor_error!(
            illegal_argument,
            "token receiver expected to {}, was {}",
            my_id,
            payload.to
        ));
    }
    Ok(payload)
}

// Validates an allocation request.
fn validate_new_allocation(
    req: &AllocationRequest,
    policy: &Policy,
    curr_epoch: ChainEpoch,
) -> Result<(), ActorError> {
    // Size must be at least the policy minimum.
    if DataCap::from(req.size.0) < policy.minimum_verified_allocation_size {
        return Err(actor_error!(
            illegal_argument,
            "allocation size {} below minimum {}",
            req.size.0,
            policy.minimum_verified_allocation_size
        ));
    }
    // Term must be at least the policy minimum.
    if req.term_min < policy.minimum_verified_allocation_term {
        return Err(actor_error!(
            illegal_argument,
            "allocation term min {} below limit {}",
            req.term_min,
            policy.minimum_verified_allocation_term
        ));
    }
    // Term cannot exceed the policy maximum.
    if req.term_max > policy.maximum_verified_allocation_term {
        return Err(actor_error!(
            illegal_argument,
            "allocation term max {} above limit {}",
            req.term_max,
            policy.maximum_verified_allocation_term
        ));
    }
    // Term range must be non-empty.
    if req.term_min > req.term_max {
        return Err(actor_error!(
            illegal_argument,
            "allocation term min {} exceeds term max {}",
            req.term_min,
            req.term_max
        ));
    }

    // Allocation must expire in the future.
    if req.expiration < curr_epoch {
        return Err(actor_error!(
            illegal_argument,
            "allocation expiration epoch {} has passed current epoch {}",
            req.expiration,
            curr_epoch
        ));
    }
    // Allocation must expire soon enough.
    let max_expiration = curr_epoch + policy.maximum_verified_allocation_expiration;
    if req.expiration > max_expiration {
        return Err(actor_error!(
            illegal_argument,
            "allocation expiration {} exceeds maximum {}",
            req.expiration,
            max_expiration
        ));
    }
    Ok(())
}

fn validate_claim_extension(
    req: &ClaimExtensionRequest,
    claim: &Claim,
    policy: &Policy,
    curr_epoch: ChainEpoch,
) -> Result<(), ActorError> {
    // The new term max is the policy limit after current epoch (not after the old term max).
    let term_limit_absolute = curr_epoch + policy.maximum_verified_allocation_term;
    let term_limit_relative = term_limit_absolute - claim.term_start;
    if req.term_max > term_limit_relative {
        return Err(actor_error!(
            illegal_argument,
            format!(
                "term_max {} for claim {} exceeds maximum {} at current epoch {}",
                req.term_max, req.claim, term_limit_relative, curr_epoch
            )
        ));
    }
    // The new term max must be larger than the old one.
    // Cannot reduce term, and cannot spend datacap on a zero increase.
    // There is no policy on minimum extension duration.
    if req.term_max <= claim.term_max {
        return Err(actor_error!(
            illegal_argument,
            "term_max {} for claim {} is not larger than existing term max {}",
            req.term_max,
            req.claim,
            claim.term_max
        ));
    }
    // The claim must not have already expired.
    // Unlike when the claim client extends term up to the originally-allowed max,
    // allowing extension of expired claims with new datacap could revive a claim arbitrarily
    // far into the future.
    // A claim can be extended continuously into the future, but once it has expired
    // it is expired for good.
    let claim_expiration = claim.term_start + claim.term_max;
    if curr_epoch > claim_expiration {
        return Err(actor_error!(
            forbidden,
            "claim {} expired at {}, current epoch {}",
            req.claim,
            claim_expiration,
            curr_epoch
        ));
    }
    Ok(())
}

// Checks that an address corresponsds to a miner actor.
fn resolve_miner_id<BS, RT>(rt: &mut RT, addr: &Address) -> Result<ActorID, ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    let id = rt
        .resolve_address(addr)
        .with_context_code(ExitCode::USR_ILLEGAL_ARGUMENT, || {
            format!("failed to resolve provider address {}", addr)
        })?;
    let code_cid = rt
        .get_actor_code_cid(&id)
        .with_context_code(ExitCode::USR_ILLEGAL_ARGUMENT, || {
            format!("no code CID for provider {}", addr)
        })?;
    let provider_type = rt
        .resolve_builtin_actor_type(&code_cid)
        .with_context_code(ExitCode::USR_ILLEGAL_ARGUMENT, || {
            format!("provider code {} must be built-in miner actor", code_cid)
        })?;
    if provider_type != Type::Miner {
        return Err(actor_error!(
            illegal_argument,
            "allocation provider {} must be a miner actor, was {:?}",
            addr,
            provider_type
        ));
    }
    Ok(id)
}

fn can_claim_alloc(
    claim_alloc: &SectorAllocationClaim,
    provider: ActorID,
    alloc: &Allocation,
    curr_epoch: ChainEpoch,
) -> bool {
    let sector_lifetime = claim_alloc.sector_expiry - curr_epoch;

    provider == alloc.provider
        && claim_alloc.client == alloc.client
        && claim_alloc.data == alloc.data
        && claim_alloc.size == alloc.size
        && curr_epoch <= alloc.expiration
        && sector_lifetime >= alloc.term_min
        && sector_lifetime <= alloc.term_max
}
