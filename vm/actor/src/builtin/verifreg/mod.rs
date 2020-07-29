// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;
mod types;
pub use self::state::State;
pub use self::types::*;
use crate::builtin::singletons::STORAGE_MARKET_ACTOR_ADDR;
use crate::{HAMT_BIT_WIDTH, SYSTEM_ACTOR_ADDR};
use address::Address;
use ipld_blockstore::BlockStore;
use ipld_hamt::BytesKey;
use ipld_hamt::Hamt;
use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, Zero};
use runtime::{ActorCode, Runtime};
use vm::{ActorError, ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};

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
        let empty_root = Hamt::<BytesKey, _>::new_with_bit_width(rt.store(), HAMT_BIT_WIDTH)
            .flush()
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("Failed to create registry state {:}", e),
                )
            })?;
        let st = State::new(empty_root, root_key);
        rt.create(&st)?;
        Ok(())
    }

    pub fn add_verifier<BS, RT>(rt: &mut RT, params: AddVerifierParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let state: State = rt.state()?;
        rt.validate_immediate_caller_is(std::iter::once(&state.root_key))?;

        rt.transaction::<_, Result<_, ActorError>, _>(|st: &mut State, rt| {
            st.put_verifier(rt.store(), &params.address, &params.allowance)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to add verifier: {:}", e),
                    )
                })?;
            Ok(())
        })??;

        Ok(())
    }

    pub fn remove_verifier<BS, RT>(rt: &mut RT, params: AddVerifierParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let state: State = rt.state()?;
        rt.validate_immediate_caller_is(std::iter::once(&state.root_key))?;

        rt.transaction::<_, Result<_, ActorError>, _>(|st: &mut State, rt| {
            st.delete_verifier(rt.store(), &params.address)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to add verifier: {:}", e),
                    )
                })?;
            Ok(())
        })??;

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
        if params.allowance <= *MINIMUM_VERIFIED_SIZE {
            return Err(ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!(
                    "Allowance {:} below MinVerifiedDealSize for add verified client {:}",
                    params.allowance, params.address
                ),
            ));
        }

        rt.validate_immediate_caller_accept_any();
        rt.transaction(|st: &mut State, rt| {
            // Validate caller is one of the verifiers.
            let verify_addr = rt.message().caller();

            let verifier_cap = st
                .get_verifier(rt.store(), &verify_addr)
                .map_err(|_| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("Failed to get Verifier {:?}", verify_addr),
                    )
                })?
                .ok_or_else(|| {
                    ActorError::new(
                        ExitCode::ErrNotFound,
                        format!("Invalid Verifier {:}", verify_addr),
                    )
                })?;

            // Compute new verifier cap and update.
            if verifier_cap < params.allowance {
                return Err(ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!(
                        "Add more DataCap {:} for VerifiedClient than allocated {:}",
                        params.allowance, verifier_cap
                    ),
                ));
            }
            let new_verifier_cap = verifier_cap - &params.allowance;
            st.put_verifier(rt.store(), &verify_addr, &new_verifier_cap)
                .map_err(|_| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!(
                            "Failed to update new verifier cap {:?} for {:?}",
                            new_verifier_cap, params.allowance
                        ),
                    )
                })?;

            // Write-once entry and does not get changed for simplicity.
            // If parties neeed more allowance, they can get another VerifiedClient account.
            // This is a one-time, upfront allocation.
            // Returns error if VerifiedClient already exists
            st.get_verified_client(rt.store(), &params.address)
                .map_err(|_| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("Failed to get verified client{:}", params.address),
                    )
                })?
                .ok_or_else(|| {
                    ActorError::new(
                        ExitCode::ErrIllegalArgument,
                        format!("Illegal Argument{:}", params.address),
                    )
                })?;
            st.put_verified_client(rt.store(), &params.address, &params.allowance)
                .map_err(|_| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!(
                            "Failed to add verified client {:?} with cap {:?}",
                            params.address, params.allowance
                        ),
                    )
                })?;

            Ok(())
        })??;

        Ok(())
    }

    pub fn use_bytes<BS, RT>(rt: &mut RT, params: UseBytesParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*STORAGE_MARKET_ACTOR_ADDR))?;
        if params.deal_size < *MINIMUM_VERIFIED_SIZE {
            return Err(ActorError::new(
                ExitCode::ErrIllegalState,
                format!(
                    "Verified Dealsize {:} is below minimum in usedbytes",
                    params.deal_size
                ),
            ));
        }

        rt.transaction(|st: &mut State, rt| {
            let verifier_cap = st
                .get_verifier(rt.store(), &params.address)
                .map_err(|_| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("Failed to get Verifier {:?}", &params.address),
                    )
                })?
                .ok_or_else(|| {
                    ActorError::new(
                        ExitCode::ErrIllegalArgument,
                        format!("Invalid Verifier {:}", params.address),
                    )
                })?;

            if params.deal_size <= verifier_cap {
                return Err(ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!(
                        "Deal size of {:} is greater than verifier_cap {:}",
                        params.deal_size, verifier_cap
                    ),
                ));
            };
            let new_verifier_cap = &verifier_cap - &params.deal_size;
            if new_verifier_cap < *MINIMUM_VERIFIED_SIZE {
                // Delete entry if remaining DataCap is less than MinVerifiedDealSize.
                // Will be restored later if the deal did not get activated with a ProvenSector.
                st.delete_verified_client(rt.store(), &params.address)
                    .map_err(|_| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!(
                                "Failed to delete verified client{:} with bytes {:?}",
                                params.address, params.deal_size
                            ),
                        )
                    })
            } else {
                st.put_verified_client(rt.store(), &params.address, &new_verifier_cap)
                    .map_err(|_| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!(
                                "Failed to put verified client{:} with bytes {:}",
                                params.address, params.deal_size
                            ),
                        )
                    })
            }
        })??;

        Ok(())
    }

    // Called by HandleInitTimeoutDeals from StorageMarketActor when a VerifiedDeal fails to init.
    // Restore allowable cap for the client, creating new entry if the client has been deleted.
    pub fn restore_bytes<BS, RT>(rt: &mut RT, params: RestoreBytesParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*STORAGE_MARKET_ACTOR_ADDR))?;
        if params.deal_size < *MINIMUM_VERIFIED_SIZE {
            return Err(ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!(
                    "Verified Dealsize {:} is below minimum in usedbytes",
                    params.deal_size
                ),
            ));
        }

        rt.transaction(|st: &mut State, rt| {
            let verifier_cap = st
                .get_verified_client(rt.store(), &params.address)
                .map_err(|_| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("Failed to get Verifier {:?}", params.address.clone()),
                    )
                })?
                .unwrap_or_else(Zero::zero);

            let new_verifier_cap = verifier_cap + &params.deal_size;
            st.put_verified_client(rt.store(), &params.address, &new_verifier_cap)
                .map_err(|_| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!(
                            "Failed to put verified client{:} with bytes {:}",
                            params.address, params.deal_size
                        ),
                    )
                })
        })??;

        Ok(())
    }
}

impl ActorCode for Actor {
    fn invoke_method<BS, RT>(
        &self,
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
                Self::constructor(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::AddVerifier) => {
                Self::add_verifier(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::RemoveVerifier) => {
                Self::remove_verifier(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::AddVerifiedClient) => {
                Self::add_verified_client(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::UseBytes) => {
                Self::use_bytes(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::RestoreBytes) => {
                Self::restore_bytes(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            None => Err(rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method".to_owned())),
        }
    }
}
