// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::actors::miner;
use crate::shim::{
    actors::{is_account_actor, is_ethaccount_actor, is_placeholder_actor},
    address::{Address, Payload},
    randomness::Randomness,
    sector::{ExtendedSectorInfo, RegisteredPoStProof, RegisteredSealProof},
    state_tree::ActorState,
    version::NetworkVersion,
};
use crate::utils::encoding::prover_id_from_u64;
use cid::Cid;
use fil_actors_shared::filecoin_proofs_api::post;
use fil_actors_shared::fvm_ipld_bitfield::BitField;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::bytes_32;

use crate::state_manager::{errors::*, StateManager};

use super::MinerActorStateLoad as _;

impl<DB> StateManager<DB>
where
    DB: Blockstore,
{
    /// Retrieves and generates a vector of sector info for the winning `PoSt`
    /// verification.
    pub fn get_sectors_for_winning_post(
        &self,
        st: &Cid,
        nv: NetworkVersion,
        miner_address: &Address,
        rand: Randomness,
    ) -> Result<Vec<ExtendedSectorInfo>, anyhow::Error> {
        let store = self.blockstore();

        let actor = self
            .get_actor(miner_address, *st)?
            .ok_or_else(|| Error::State("Miner actor address could not be resolved".to_string()))?;
        let mas = miner::State::load(self.blockstore(), actor.code, actor.state)?;

        let proving_sectors = {
            let mut proving_sectors = BitField::new();

            if nv < NetworkVersion::V7 {
                mas.for_each_deadline(&self.chain_config.policy, store, |_, deadline| {
                    let mut fault_sectors = BitField::new();
                    deadline.for_each(store, |_, partition: miner::Partition| {
                        proving_sectors |= partition.all_sectors();
                        fault_sectors |= partition.faulty_sectors();
                        Ok(())
                    })?;

                    proving_sectors -= &fault_sectors;
                    Ok(())
                })?;
            } else {
                mas.for_each_deadline(&self.chain_config.policy, store, |_, deadline| {
                    deadline.for_each(store, |_, partition: miner::Partition| {
                        proving_sectors |= &partition.active_sectors();
                        Ok(())
                    })?;
                    Ok(())
                })?;
            }
            proving_sectors
        };

        let num_prov_sect = proving_sectors.len();

        if num_prov_sect == 0 {
            return Ok(Vec::new());
        }

        let info = mas.info(store)?;
        let spt = RegisteredSealProof::from_sector_size(info.sector_size().into(), nv);

        let wpt = spt.registered_winning_post_proof()?;

        let m_id = miner_address.id()?;

        let ids = generate_winning_post_sector_challenge(wpt.into(), m_id, rand, num_prov_sect)?;

        let mut iter = proving_sectors.iter();

        let mut selected_sectors = BitField::new();
        for n in ids {
            let sno = iter.nth(n as usize).ok_or_else(|| {
                anyhow::anyhow!(
                    "Error iterating over proving sectors, id {} does not exist",
                    n
                )
            })?;
            selected_sectors.set(sno);
        }

        let sectors = mas.load_sectors(store, Some(&selected_sectors))?;

        let out = sectors
            .into_iter()
            .map(|s_info| ExtendedSectorInfo {
                proof: s_info.seal_proof.into(),
                sector_number: s_info.sector_number,
                sector_key: s_info.sector_key_cid,
                sealed_cid: s_info.sealed_cid,
            })
            .collect();

        Ok(out)
    }
}

pub fn is_valid_for_sending(network_version: NetworkVersion, actor: &ActorState) -> bool {
    // Comments from Lotus:
    // Before nv18 (Hygge), we only supported built-in account actors as senders.
    //
    // Note: this gate is probably superfluous, since:
    // 1. Placeholder actors cannot be created before nv18.
    // 2. EthAccount actors cannot be created before nv18.
    // 3. Delegated addresses cannot be created before nv18.
    //
    // But it's a safeguard.
    //
    // Note 2: ad-hoc checks for network versions like this across the codebase
    // will be problematic with networks with diverging version lineages
    // (e.g. Hyperspace). We need to revisit this strategy entirely.
    if network_version < NetworkVersion::V18 {
        return is_account_actor(&actor.code);
    }

    // After nv18, we also support other kinds of senders.
    if is_account_actor(&actor.code) || is_ethaccount_actor(&actor.code) {
        return true;
    }

    // Allow placeholder actors with a delegated address and nonce 0 to send a
    // message. These will be converted to an EthAccount actor on first send.
    if !is_placeholder_actor(&actor.code)
        || actor.sequence != 0
        || actor.delegated_address.is_none()
    {
        return false;
    }

    // Only allow such actors to send if their delegated address is in the EAM's
    // namespace.
    if let Payload::Delegated(address) = actor
        .delegated_address
        .as_ref()
        .expect("unfallible")
        .payload()
    {
        address.namespace() == Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR.id().unwrap()
    } else {
        false
    }
}

/// Generates sector challenge indexes for use in winning PoSt verification.
fn generate_winning_post_sector_challenge(
    proof: RegisteredPoStProof,
    prover_id: u64,
    mut rand: Randomness,
    eligible_sector_count: u64,
) -> Result<Vec<u64>, anyhow::Error> {
    // Necessary to be valid bls12 381 element.
    if let Some(b31) = rand.0.get_mut(31) {
        *b31 &= 0x3f;
    } else {
        anyhow::bail!("rand should have at least 32 bytes");
    }

    post::generate_winning_post_sector_challenge(
        proof.try_into()?,
        &bytes_32(&rand.0),
        eligible_sector_count,
        prover_id_from_u64(prover_id),
    )
}

#[cfg(test)]
mod test {
    use crate::shim::{address::Address, econ::TokenAmount, state_tree::ActorState};
    use cid::Cid;

    use super::*;

    #[test]
    fn is_valid_for_sending_test() {
        let create_actor = |code: &Cid, sequence: u64, delegated_address: Option<Address>| {
            ActorState::new(
                code.to_owned(),
                // changing this cid will unleash unthinkable horrors upon the world
                Cid::try_from("bafk2bzaceavfgpiw6whqigmskk74z4blm22nwjfnzxb4unlqz2e4wgcthulhu")
                    .unwrap(),
                TokenAmount::default(),
                sequence,
                delegated_address,
            )
        };

        // calibnet actor version 10
        let account_actor_cid =
            Cid::try_from("bafk2bzaceavfgpiw6whqigmskk74z4blm22nwjfnzxb4unlqz2e4wg3c5ujpw")
                .unwrap();
        let ethaccount_actor_cid =
            Cid::try_from("bafk2bzacebiyrhz32xwxi6xql67aaq5nrzeelzas472kuwjqmdmgwotpkj35e")
                .unwrap();
        let placeholder_actor_cid =
            Cid::try_from("bafk2bzacedfvut2myeleyq67fljcrw4kkmn5pb5dpyozovj7jpoez5irnc3ro")
                .unwrap();

        // happy path for account actor
        let actor = create_actor(&account_actor_cid, 0, None);
        assert!(is_valid_for_sending(NetworkVersion::V17, &actor));

        // eth account not allowed before v18, should fail
        let actor = create_actor(&ethaccount_actor_cid, 0, None);
        assert!(!is_valid_for_sending(NetworkVersion::V17, &actor));

        // happy path for eth account
        assert!(is_valid_for_sending(NetworkVersion::V18, &actor));

        // no delegated address for placeholder actor, should fail
        let actor = create_actor(&placeholder_actor_cid, 0, None);
        assert!(!is_valid_for_sending(NetworkVersion::V18, &actor));

        // happy path for the placeholder actor
        let delegated_address = Address::new_delegated(
            Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR.id().unwrap(),
            &[0; 20],
        )
        .ok();
        let actor = create_actor(&placeholder_actor_cid, 0, delegated_address);
        assert!(is_valid_for_sending(NetworkVersion::V18, &actor));

        // sequence not 0, should fail
        let actor = create_actor(&placeholder_actor_cid, 1, delegated_address);
        assert!(!is_valid_for_sending(NetworkVersion::V18, &actor));

        // delegated address not in EAM namespace, should fail
        let delegated_address =
            Address::new_delegated(Address::CHAOS_ACTOR.id().unwrap(), &[0; 20]).ok();
        let actor = create_actor(&placeholder_actor_cid, 0, delegated_address);
        assert!(!is_valid_for_sending(NetworkVersion::V18, &actor));
    }
}

/// Parsed tree of [`fvm4::trace::ExecutionEvent`]s
pub mod structured {
    use crate::{
        rpc::state::{ActorTrace, ExecutionTrace, GasTrace, MessageTrace, ReturnTrace},
        shim::kernel::ErrorNumber,
    };
    use std::collections::VecDeque;

    use crate::shim::{
        address::Address,
        error::ExitCode,
        gas::GasCharge,
        kernel::SyscallError,
        trace::{Call, CallReturn, ExecutionEvent},
    };
    use fvm_ipld_encoding::{ipld_block::IpldBlock, RawBytes};
    use itertools::Either;

    enum CallTreeReturn {
        Return(CallReturn),
        Abort(ExitCode),
        Error(SyscallError),
    }

    #[derive(Debug, thiserror::Error)]
    pub enum BuildExecutionTraceError {
        #[error("every ExecutionEvent::Return | ExecutionEvent::CallError should be preceded by an ExecutionEvent::Call, but this one wasn't")]
        UnexpectedReturn,
        #[error("every ExecutionEvent::Call should have a corresponding ExecutionEvent::Return, but this one didn't")]
        NoReturn,
        #[error("unrecognised ExecutionEvent variant: {0:?}")]
        UnrecognisedEvent(Box<dyn std::fmt::Debug + Send + Sync + 'static>),
    }

    /// Construct a single [`ExecutionTrace`]s from a linear array of [`ExecutionEvent`](fvm4::trace::ExecutionEvent)s.
    ///
    /// This function is so-called because it similar to the parse step in a traditional compiler:
    /// ```text
    /// text --lex-->     tokens     --parse-->   AST
    ///               ExecutionEvent --parse--> ExecutionTrace
    /// ```
    ///
    /// This function is notable in that [`GasCharge`](fvm4::gas::GasCharge)s which precede a [`ExecutionTrace`] at the root level
    /// are attributed to that node.
    ///
    /// We call this "front loading", and is copied from [this (rather obscure) code in `filecoin-ffi`](https://github.com/filecoin-project/filecoin-ffi/blob/v1.23.0/rust/src/fvm/machine.rs#L209)
    ///
    /// ```text
    /// GasCharge GasCharge Call GasCharge Call CallError CallReturn
    /// ────┬──── ────┬──── ─┬── ────┬──── ─┬── ───┬───── ────┬─────
    ///     │         │      │       │      │      │          │
    ///     │         │      │       │      └─(T)──┘          │
    ///     │         │      └───────┴───(T)───┴──────────────┘
    ///     └─────────┴──────────────────►│
    ///     ("front loaded" GasCharges)   │
    ///                                  (T)
    ///
    /// (T): a ExecutionTrace node
    /// ```
    ///
    /// Multiple call trees and trailing gas will be warned and ignored.
    /// If no call tree is found, returns [`Ok(None)`]
    pub fn parse_events(
        events: Vec<ExecutionEvent>,
    ) -> anyhow::Result<Option<ExecutionTrace>, BuildExecutionTraceError> {
        let mut events = VecDeque::from(events);
        let mut front_load_me = vec![];
        let mut call_trees = vec![];

        // we don't use a `for` loop so we can pass events them to inner parsers
        while let Some(event) = events.pop_front() {
            match event {
                ExecutionEvent::GasCharge(gc) => front_load_me.push(gc),
                ExecutionEvent::Call(call) => call_trees.push(ExecutionTrace::parse(call, {
                    // if ExecutionTrace::parse took impl Iterator<Item = ExecutionEvent>
                    // the compiler would infinitely recurse trying to resolve
                    // &mut &mut &mut ..: Iterator
                    // so use a VecDeque instead
                    for gc in front_load_me.drain(..).rev() {
                        events.push_front(ExecutionEvent::GasCharge(gc))
                    }
                    &mut events
                })?),
                ExecutionEvent::CallReturn(_)
                | ExecutionEvent::CallAbort(_)
                | ExecutionEvent::CallError(_) => {
                    return Err(BuildExecutionTraceError::UnexpectedReturn)
                }
                ExecutionEvent::Log(_ignored) => {}
                ExecutionEvent::InvokeActor(_cid) => {}
                ExecutionEvent::Unknown(u) => {
                    return Err(BuildExecutionTraceError::UnrecognisedEvent(Box::new(u)))
                }
            }
        }

        if !front_load_me.is_empty() {
            tracing::warn!(
                "vm tracing: ignoring {} trailing gas charges",
                front_load_me.len()
            );
        }

        match call_trees.len() {
            0 => Ok(None),
            1 => Ok(Some(call_trees.remove(0))),
            many => {
                tracing::warn!(
                    "vm tracing: ignoring {} call trees at the root level",
                    many - 1
                );
                Ok(Some(call_trees.remove(0)))
            }
        }
    }

    impl ExecutionTrace {
        /// ```text
        ///    events: GasCharge Call CallError CallReturn ...
        ///            ────┬──── ─┬── ───┬───── ────┬─────
        ///                │      │      │          │
        /// ┌──────┐       │      └─(T)──┘          │
        /// │ Call ├───────┴───(T)───┴──────────────┘
        /// └──────┘            |                   ▲
        ///                     ▼                   │
        ///              Returned ExecutionTrace    │
        ///                                     parsing end
        /// ```
        fn parse(
            call: Call,
            events: &mut VecDeque<ExecutionEvent>,
        ) -> Result<ExecutionTrace, BuildExecutionTraceError> {
            let mut gas_charges = vec![];
            let mut subcalls = vec![];
            let mut actor_trace = None;

            // we don't use a for loop over `events` so we can pass them to recursive calls
            while let Some(event) = events.pop_front() {
                let found_return = match event {
                    ExecutionEvent::GasCharge(gc) => {
                        gas_charges.push(to_gas_trace(gc));
                        None
                    }
                    ExecutionEvent::Call(call) => {
                        subcalls.push(Self::parse(call, events)?);
                        None
                    }
                    ExecutionEvent::CallReturn(ret) => Some(CallTreeReturn::Return(ret)),
                    ExecutionEvent::CallAbort(ab) => Some(CallTreeReturn::Abort(ab)),
                    ExecutionEvent::CallError(e) => Some(CallTreeReturn::Error(e)),
                    ExecutionEvent::Log(_ignored) => None,
                    ExecutionEvent::InvokeActor(cid) => {
                        actor_trace = match cid {
                            Either::Left(_cid) => None,
                            Either::Right(actor) => Some(ActorTrace {
                                id: actor.id,
                                state: actor.state,
                            }),
                        };
                        None
                    }
                    // RUST: This should be caught at compile time with #[deny(non_exhaustive_omitted_patterns)]
                    //       So that BuildExecutionTraceError::UnrecognisedEvent is never constructed
                    //       But that lint is not yet stabilised: https://github.com/rust-lang/rust/issues/89554
                    ExecutionEvent::Unknown(u) => {
                        return Err(BuildExecutionTraceError::UnrecognisedEvent(Box::new(u)))
                    }
                };

                // commonise the return branch
                if let Some(ret) = found_return {
                    return Ok(ExecutionTrace {
                        msg: to_message_trace(call),
                        msg_rct: to_return_trace(ret),
                        gas_charges,
                        subcalls,
                        invoked_actor: actor_trace,
                    });
                }
            }

            Err(BuildExecutionTraceError::NoReturn)
        }
    }

    fn to_message_trace(call: Call) -> MessageTrace {
        let (bytes, codec) = to_bytes_codec(call.params);
        MessageTrace {
            from: Address::new_id(call.from),
            to: call.to,
            value: call.value,
            method: call.method_num,
            params: bytes,
            params_codec: codec,
            gas_limit: call.gas_limit,
            read_only: call.read_only,
        }
    }

    fn to_return_trace(ret: CallTreeReturn) -> ReturnTrace {
        match ret {
            CallTreeReturn::Return(return_code) => {
                let exit_code = return_code.exit_code.unwrap_or(0.into());
                let (bytes, codec) = to_bytes_codec(return_code.data);
                ReturnTrace {
                    exit_code,
                    r#return: bytes,
                    return_codec: codec,
                }
            }
            CallTreeReturn::Abort(exit_code) => ReturnTrace {
                exit_code,
                r#return: RawBytes::default(),
                return_codec: 0,
            },
            CallTreeReturn::Error(syscall_error) => match syscall_error.number {
                ErrorNumber::InsufficientFunds => ReturnTrace {
                    exit_code: ExitCode::from(6),
                    r#return: RawBytes::default(),
                    return_codec: 0,
                },
                _ => ReturnTrace {
                    exit_code: ExitCode::from(0),
                    r#return: RawBytes::default(),
                    return_codec: 0,
                },
            },
        }
    }

    fn to_bytes_codec(data: Either<RawBytes, Option<IpldBlock>>) -> (RawBytes, u64) {
        match data {
            Either::Left(l) => (l, 0),
            Either::Right(r) => match r {
                Some(b) => (RawBytes::from(b.data), b.codec),
                None => (RawBytes::default(), 0),
            },
        }
    }

    fn to_gas_trace(gc: GasCharge) -> GasTrace {
        GasTrace {
            name: gc.name().into(),
            total_gas: gc.total().round_up(),
            compute_gas: gc.compute_gas().round_up(),
            storage_gas: gc.other_gas().round_up(),
            time_taken: gc.elapsed().as_nanos(),
        }
    }
}
