pub mod state;
pub mod types;
pub use self::state::State;
pub use self::types::*;
use crate::StoragePower;
use crate::{HAMT_BIT_WIDTH, SYSTEM_ACTOR_ADDR};
use address::Address;
use ipld_blockstore::BlockStore;
use ipld_hamt::Hamt;
use message::Message;
use num_derive::FromPrimitive;
use runtime::Runtime;
use vm::{ActorError, ExitCode, METHOD_CONSTRUCTOR};
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
        let empty_root = Hamt::<String, _>::new_with_bit_width(rt.store(), HAMT_BIT_WIDTH)
            .flush()
            .map_err(|e| {
                rt.abort(
                    ExitCode::ErrIllegalState,
                    format!("failed to create storage power state: {}", e),
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

        rt.transaction::<_, Result<_, String>, _>(|st: &mut State, rt| {
            st.put_verified(rt.store(), params.address, params.allowance)?;
            Ok(())
        })?
        .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e))?;

        Ok(())
    }

    pub fn delete_verifier<BS, RT>(rt: &mut RT, params: AddVerifierParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let state: State = rt.state()?;
        rt.validate_immediate_caller_is(std::iter::once(&state.root_key))?;

        rt.transaction::<_, Result<_, String>, _>(|st: &mut State, rt| {
            st.delete_verifier(rt.store(), params.address)?;
            Ok(())
        })?
        .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e))?;

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
        if params.allowance <= Datacap::new(StoragePower::new([MINIMUM_VERIFIED_SIZE].to_vec())) {
            return Err(ActorError::new(
                ExitCode::ErrIllegalState,
                format!(
                    "Allowance {:} below MinVerifiedDealSize for add verified client {:}",
                    params.allowance, params.address
                ),
            ));
        }

        rt.transaction::<_, Result<_, ActorError>, _>(|st: &mut State, rt| {
            let message: Box<&dyn Message> = Box::new(rt.message());
            let verify_addr = message.from();

            let verifier_cap = st
                .get_verifier(rt.store(), *verify_addr)
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

            if verifier_cap < params.allowance {
                return Err(ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!(
                        "Add more DataCap {:} for VerifiedClient than allocated {:}",
                        params.allowance, verifier_cap
                    ),
                ));
            }
            let new_verifier_cap = verifier_cap - params.allowance.clone();
            st.put_verified(rt.store(), *verify_addr, new_verifier_cap.clone())
                .map_err(|_| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!(
                            "Failed to update new verifier cap {:?} for {:?}",
                            new_verifier_cap.clone(),
                            params.allowance
                        ),
                    )
                })?;
            st.get_verified_clients(rt.store(), params.address)
                .map_err(|_| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("Failed to get verified client{:}", params.address),
                    )
                })?
                .ok_or_else(|| {
                    ActorError::new(
                        ExitCode::ErrNotFound,
                        format!("Illegal Argument{:}", params.address),
                    )
                })?;
            st.put_verified(rt.store(), params.address, params.allowance.clone())
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
        rt.validate_immediate_caller_is(std::iter::once(&*SYSTEM_ACTOR_ADDR))?;
        if params.deal_size < Datacap::new(StoragePower::new([MINIMUM_VERIFIED_SIZE].to_vec())) {
            return Err(ActorError::new(
                ExitCode::ErrIllegalState,
                format!(
                    "Verified Dealsize {:} is below minimum in usedbytes",
                    params.deal_size
                ),
            ));
        }

        rt.transaction::<_, Result<_, ActorError>, _>(|st: &mut State, rt| {
            let verifier_cap = st
                .get_verifier(rt.store(), params.address.clone())
                .map_err(|_| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("Failed to get Verifier {:?}", params.address.clone()),
                    )
                })?
                .ok_or_else(|| {
                    ActorError::new(
                        ExitCode::ErrNotFound,
                        format!("Invalid Verifier {:}", params.address),
                    )
                })?;

            if verifier_cap >= Datacap::new(StoragePower::new([0].to_vec())) {
                panic!("new verifier cap should be greater than or equal to 0");
            }
            let new_verifier_cap = verifier_cap - params.deal_size.clone();
            if new_verifier_cap < Datacap::new(StoragePower::new([MINIMUM_VERIFIED_SIZE].to_vec()))
            {
                st.delete_verified_clients(rt.store(), params.address)
                    .map_err(|_| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!(
                                "Failed to delete verified client{:} with bytes {:?}",
                                params.clone().address,
                                params.deal_size
                            ),
                        )
                    })
            } else {
                st.put_verified_client(rt.store(), params.address, new_verifier_cap)
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

    pub fn restore_bytes<BS, RT>(rt: &mut RT, params: UseBytesParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*SYSTEM_ACTOR_ADDR))?;
        if params.deal_size < Datacap::new(StoragePower::new([MINIMUM_VERIFIED_SIZE].to_vec())) {
            return Err(ActorError::new(
                ExitCode::ErrIllegalState,
                format!(
                    "Verified Dealsize {:} is below minimum in usedbytes",
                    params.deal_size
                ),
            ));
        }

        rt.transaction::<_, Result<_, ActorError>, _>(|st: &mut State, rt| {
            let verifier_cap = st
                .get_verifier(rt.store(), params.address.clone())
                .map_err(|_| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("Failed to get Verifier {:?}", params.address.clone()),
                    )
                })?
                .unwrap_or_else(|| {
                    Datacap::new(StoragePower::new([MINIMUM_VERIFIED_SIZE].to_vec()))
                });

            let new_verifier_cap = verifier_cap + params.deal_size.clone();
            st.put_verified_client(rt.store(), params.address, new_verifier_cap)
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
