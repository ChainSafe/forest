// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;
mod types;

pub use self::state::State;
pub use self::types::*;
use crate::{
    make_map_with_root_and_bitwidth, resolve_to_id_addr, ActorDowncast, STORAGE_MARKET_ACTOR_ADDR,
    SYSTEM_ACTOR_ADDR,
};
use address::Address;
use fil_types::HAMT_BIT_WIDTH;
use ipld_blockstore::BlockStore;
use num_bigint::bigint_ser::BigIntDe;
use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, Signed};
use runtime::{ActorCode, Runtime};
use vm::{actor_error, ActorError, ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};

// * Updated to specs-actors commit: 845089a6d2580e46055c24415a6c32ee688e5186 (v3.0.0)

/// Account actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    AddVerifier = 2,
    RemoveVerifier = 3,
    AddVerifiedClient = 4,
    UseBytes = 5,
    RestoreBytes = 6,
}

pub struct Actor;
impl Actor {
    /// Constructor for Registry Actor
    pub fn constructor<BS, RT>(rt: &mut RT, root_key: Address) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*SYSTEM_ACTOR_ADDR))?;

        // root should be an ID address
        let id_addr = rt
            .resolve_address(&root_key)?
            .ok_or_else(|| actor_error!(ErrIllegalArgument, "root should be an ID address"))?;

        let st = State::new(rt.store(), id_addr).map_err(|e| {
            e.downcast_default(ExitCode::ErrIllegalState, "Failed to create verifreg state")
        })?;

        rt.create(&st)?;
        Ok(())
    }

    pub fn add_verifier<BS, RT>(rt: &mut RT, params: AddVerifierParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        if &params.allowance < &MINIMUM_VERIFIED_DEAL_SIZE {
            return Err(actor_error!(
                ErrIllegalArgument,
                "Allowance {} below minimum deal size for add verifier {}",
                params.allowance,
                params.address
            ));
        }

        let verifier = resolve_to_id_addr(rt, &params.address).map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalState,
                format!("failed to resolve addr {} to ID addr", params.address),
            )
        })?;

        let st: State = rt.state()?;
        rt.validate_immediate_caller_is(std::iter::once(&st.root_key))?;

        if verifier == st.root_key {
            return Err(actor_error!(
                ErrIllegalArgument,
                "Rootkey cannot be added as verifier"
            ));
        }

        rt.transaction(|st: &mut State, rt| {
            let mut verifiers =
                make_map_with_root_and_bitwidth(&st.verifiers, rt.store(), HAMT_BIT_WIDTH)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::ErrIllegalState,
                            "failed to load verified clients",
                        )
                    })?;
            let verified_clients = make_map_with_root_and_bitwidth::<_, BigIntDe>(
                &st.verified_clients,
                rt.store(),
                HAMT_BIT_WIDTH,
            )
            .map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to load verified clients")
            })?;

            let found = verified_clients
                .contains_key(&verifier.to_bytes())
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!("failed to get client state for {}", verifier),
                    )
                })?;
            if found {
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "verified client {} cannot become a verifier",
                    verifier
                ));
            }

            verifiers
                .set(
                    verifier.to_bytes().into(),
                    BigIntDe(params.allowance.clone()),
                )
                .map_err(|e| {
                    e.downcast_default(ExitCode::ErrIllegalState, "failed to add verifier")
                })?;
            st.verifiers = verifiers.flush().map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to flush verifiers")
            })?;

            Ok(())
        })?;

        Ok(())
    }

    pub fn remove_verifier<BS, RT>(rt: &mut RT, verifier_addr: Address) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let verifier = resolve_to_id_addr(rt, &verifier_addr).map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalState,
                format!("failed to resolve addr {} to ID addr", verifier_addr),
            )
        })?;

        let state: State = rt.state()?;
        rt.validate_immediate_caller_is(std::iter::once(&state.root_key))?;

        rt.transaction(|st: &mut State, rt| {
            let mut verifiers = make_map_with_root_and_bitwidth::<_, BigIntDe>(
                &st.verifiers,
                rt.store(),
                HAMT_BIT_WIDTH,
            )
            .map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to load verified clients")
            })?;
            verifiers
                .delete(&verifier.to_bytes())
                .map_err(|e| {
                    e.downcast_default(ExitCode::ErrIllegalState, "failed to remove verifier")
                })?
                .ok_or_else(|| {
                    actor_error!(ErrIllegalArgument, "failed to remove verifier: not found")
                })?;

            st.verifiers = verifiers.flush().map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to flush verifiers")
            })?;
            Ok(())
        })?;

        Ok(())
    }

    pub fn add_verified_client<BS, RT>(
        rt: &mut RT,
        params: AddVerifierClientParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // The caller will be verified by checking table below
        rt.validate_immediate_caller_accept_any()?;

        if params.allowance < *MINIMUM_VERIFIED_DEAL_SIZE {
            return Err(actor_error!(
                ErrIllegalArgument,
                "Allowance {} below MinVerifiedDealSize for add verified client {}",
                params.allowance,
                params.address
            ));
        }

        let client = resolve_to_id_addr(rt, &params.address).map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalState,
                format!("failed to resolve addr {} to ID addr", params.address),
            )
        })?;

        let st: State = rt.state()?;
        if client == st.root_key {
            return Err(actor_error!(
                ErrIllegalArgument,
                "Rootkey cannot be added as verifier"
            ));
        }

        rt.transaction(|st: &mut State, rt| {
            let mut verifiers =
                make_map_with_root_and_bitwidth(&st.verifiers, rt.store(), HAMT_BIT_WIDTH)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::ErrIllegalState,
                            "failed to load verified clients",
                        )
                    })?;
            let mut verified_clients =
                make_map_with_root_and_bitwidth(&st.verified_clients, rt.store(), HAMT_BIT_WIDTH)
                    .map_err(|e| {
                    e.downcast_default(ExitCode::ErrIllegalState, "failed to load verified clients")
                })?;

            // Validate caller is one of the verifiers.
            let verifier = rt.message().caller();
            let BigIntDe(verifier_cap) = verifiers
                .get(&verifier.to_bytes())
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!("failed to get Verifier {}", verifier),
                    )
                })?
                .ok_or_else(|| {
                    actor_error!(ErrNotFound, format!("no such Verifier {}", verifier))
                })?;

            // Validate client to be added isn't a verifier
            let found = verifiers.contains_key(&client.to_bytes()).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to get verifier")
            })?;
            if found {
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "verifier {} cannot be added as a verified client",
                    client
                ));
            }

            // Compute new verifier cap and update.
            if verifier_cap < &params.allowance {
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "Add more DataCap {} for VerifiedClient than allocated {}",
                    params.allowance,
                    verifier_cap
                ));
            }
            let new_verifier_cap = verifier_cap - &params.allowance;

            verifiers
                .set(verifier.to_bytes().into(), BigIntDe(new_verifier_cap))
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!("Failed to update new verifier cap for {}", verifier),
                    )
                })?;

            let client_cap = verified_clients.get(&client.to_bytes()).map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    format!("Failed to get verified client {}", client),
                )
            })?;
            // if verified client exists, add allowance to existing cap
            // otherwise, create new client with allownace
            let client_cap = if let Some(BigIntDe(client_cap)) = client_cap {
                client_cap + params.allowance
            } else {
                params.allowance
            };

            verified_clients
                .set(client.to_bytes().into(), BigIntDe(client_cap.clone()))
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!(
                            "Failed to add verified client {} with cap {}",
                            client, client_cap,
                        ),
                    )
                })?;

            st.verifiers = verifiers.flush().map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to flush verifiers")
            })?;
            st.verified_clients = verified_clients.flush().map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    "failed to flush verified clients",
                )
            })?;

            Ok(())
        })?;

        Ok(())
    }

    /// Called by StorageMarketActor during PublishStorageDeals.
    /// Do not allow partially verified deals (DealSize must be greater than equal to allowed cap).
    /// Delete VerifiedClient if remaining DataCap is smaller than minimum VerifiedDealSize.
    pub fn use_bytes<BS, RT>(rt: &mut RT, params: UseBytesParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*STORAGE_MARKET_ACTOR_ADDR))?;

        let client = resolve_to_id_addr(rt, &params.address).map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalState,
                format!("failed to resolve addr {} to ID addr", params.address),
            )
        })?;

        if params.deal_size < *MINIMUM_VERIFIED_DEAL_SIZE {
            return Err(actor_error!(
                ErrIllegalArgument,
                "Verified Dealsize {} is below minimum in usedbytes",
                params.deal_size
            ));
        }

        rt.transaction(|st: &mut State, rt| {
            let mut verified_clients =
                make_map_with_root_and_bitwidth(&st.verified_clients, rt.store(), HAMT_BIT_WIDTH)
                    .map_err(|e| {
                    e.downcast_default(ExitCode::ErrIllegalState, "failed to load verified clients")
                })?;

            let BigIntDe(vc_cap) = verified_clients
                .get(&client.to_bytes())
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!("failed to get verified client {}", &client),
                    )
                })?
                .ok_or_else(|| actor_error!(ErrNotFound, "no such verified client {}", client))?;
            if vc_cap.is_negative() {
                return Err(actor_error!(
                    ErrIllegalState,
                    "negative cap for client {}: {}",
                    client,
                    vc_cap
                ));
            }

            if &params.deal_size > vc_cap {
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "Deal size of {} is greater than verifier_cap {} for verified client {}",
                    params.deal_size,
                    vc_cap,
                    client
                ));
            };

            let new_vc_cap = vc_cap - &params.deal_size;
            if new_vc_cap < *MINIMUM_VERIFIED_DEAL_SIZE {
                // Delete entry if remaining DataCap is less than MinVerifiedDealSize.
                // Will be restored later if the deal did not get activated with a ProvenSector.
                verified_clients
                    .delete(&client.to_bytes())
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::ErrIllegalState,
                            format!("Failed to delete verified client {}", client),
                        )
                    })?
                    .ok_or_else(|| {
                        actor_error!(
                            ErrIllegalState,
                            "Failed to delete verified client {}: not found",
                            client
                        )
                    })?;
            } else {
                verified_clients
                    .set(client.to_bytes().into(), BigIntDe(new_vc_cap))
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::ErrIllegalState,
                            format!("Failed to update verified client {}", client),
                        )
                    })?;
            }

            st.verified_clients = verified_clients.flush().map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    "failed to flush verified clients",
                )
            })?;
            Ok(())
        })?;

        Ok(())
    }

    /// Called by HandleInitTimeoutDeals from StorageMarketActor when a VerifiedDeal fails to init.
    /// Restore allowable cap for the client, creating new entry if the client has been deleted.
    pub fn restore_bytes<BS, RT>(rt: &mut RT, params: RestoreBytesParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*STORAGE_MARKET_ACTOR_ADDR))?;
        if params.deal_size < *MINIMUM_VERIFIED_DEAL_SIZE {
            return Err(actor_error!(
                ErrIllegalArgument,
                "Below minimum VerifiedDealSize requested in RestoreBytes: {}",
                params.deal_size
            ));
        }

        let client = resolve_to_id_addr(rt, &params.address).map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalState,
                format!("failed to resolve addr {} to ID addr", params.address),
            )
        })?;

        let st: State = rt.state()?;
        if client == st.root_key {
            return Err(actor_error!(
                ErrIllegalArgument,
                "Cannot restore allowance for Rootkey"
            ));
        }

        rt.transaction(|st: &mut State, rt| {
            let verifiers = make_map_with_root_and_bitwidth::<_, BigIntDe>(
                &st.verifiers,
                rt.store(),
                HAMT_BIT_WIDTH,
            )
            .map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to load verified clients")
            })?;
            let mut verified_clients =
                make_map_with_root_and_bitwidth(&st.verified_clients, rt.store(), HAMT_BIT_WIDTH)
                    .map_err(|e| {
                    e.downcast_default(ExitCode::ErrIllegalState, "failed to load verified clients")
                })?;

            // validate we are NOT attempting to do this for a verifier
            let found = verifiers.contains_key(&client.to_bytes()).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to get verifier")
            })?;
            if found {
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "cannot restore allowance for a verifier {}",
                    client
                ));
            }

            // Get existing cap
            let BigIntDe(vc_cap) = verified_clients
                .get(&client.to_bytes())
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!("failed to get verified client {}", &client),
                    )
                })?
                .cloned()
                .unwrap_or_default();

            // Update to new cap
            let new_vc_cap = vc_cap + &params.deal_size;
            verified_clients
                .set(client.to_bytes().into(), BigIntDe(new_vc_cap))
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!("Failed to put verified client {}", client),
                    )
                })?;

            st.verified_clients = verified_clients.flush().map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    "failed to flush verified clients",
                )
            })?;
            Ok(())
        })?;

        Ok(())
    }
}

impl ActorCode for Actor {
    fn invoke_method<BS, RT>(
        rt: &mut RT,
        method: MethodNum,
        params: &Serialized,
    ) -> Result<Serialized, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        match FromPrimitive::from_u64(method) {
            Some(Method::Constructor) => {
                Self::constructor(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::AddVerifier) => {
                Self::add_verifier(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::RemoveVerifier) => {
                Self::remove_verifier(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::AddVerifiedClient) => {
                Self::add_verified_client(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::UseBytes) => {
                Self::use_bytes(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::RestoreBytes) => {
                Self::restore_bytes(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            None => Err(actor_error!(SysErrInvalidMethod; "Invalid method")),
        }
    }
}
