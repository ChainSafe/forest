// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::gas_block_store::GasBlockStore;
use super::gas_tracker::{price_list_by_epoch, GasCharge, GasTracker, PriceList};
use super::{CircSupplyCalc, LookbackStateGetter, Rand};
use actor::{
    account, actorv0,
    actorv2::{self, ActorDowncast},
    ActorVersion,
};
use address::{Address, Protocol};
use blocks::BlockHeader;
use byteorder::{BigEndian, WriteBytesExt};
use cid::{Cid, Code::Blake2b256};
use clock::ChainEpoch;
use crypto::{DomainSeparationTag, Signature};
use fil_types::{verifier::ProofVerifier, DevnetParams, NetworkParams, NetworkVersion, Randomness};
use fil_types::{PieceInfo, RegisteredSealProof, SealVerifyInfo, WindowPoStVerifyInfo};
use forest_encoding::{blake2b_256, to_vec, Cbor};
use ipld_blockstore::BlockStore;
use log::warn;
use message::{Message, UnsignedMessage};
use num_bigint::BigInt;
use num_traits::Zero;
use rayon::prelude::*;
use runtime::{
    compute_unsealed_sector_cid, ActorCode, ConsensusFault, ConsensusFaultType, MessageInfo,
    Runtime, Syscalls,
};
use state_tree::StateTree;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::error::Error as StdError;
use std::marker::PhantomData;
use std::rc::Rc;
use vm::{
    actor_error, ActorError, ActorState, ExitCode, MethodNum, Serialized, TokenAmount,
    EMPTY_ARR_CID, METHOD_SEND,
};

/// Max runtime call depth
const MAX_CALL_DEPTH: u64 = 4096;

// This is just used for gas tracing, intentionally 0 and could be removed.
const ACTOR_EXEC_GAS: GasCharge = GasCharge {
    name: "OnActorExec",
    compute_gas: 0,
    storage_gas: 0,
};

#[derive(Debug, Clone)]
struct VMMsg {
    caller: Address,
    receiver: Address,
    value_received: TokenAmount,
}

impl MessageInfo for VMMsg {
    fn caller(&self) -> &Address {
        assert!(
            matches!(self.caller.protocol(), Protocol::ID),
            "runtime message caller was not resolved to ID address"
        );
        &self.caller
    }
    fn receiver(&self) -> &Address {
        // * Can't assert that receiver is an ID address here because it was not being done
        // * pre NetworkVersion3. Can maybe add in assertion later
        &self.receiver
    }
    fn value_received(&self) -> &TokenAmount {
        &self.value_received
    }
}

/// Implementation of the Runtime trait.
pub struct DefaultRuntime<'db, 'vm, BS, R, C, LB, V, P = DevnetParams> {
    version: NetworkVersion,
    state: &'vm mut StateTree<'db, BS>,
    store: GasBlockStore<'db, BS>,
    gas_tracker: Rc<RefCell<GasTracker>>,
    vm_msg: VMMsg,
    epoch: ChainEpoch,

    origin: Address,
    origin_nonce: u64,

    depth: u64,
    num_actors_created: u64,
    price_list: PriceList,
    rand: &'vm R,
    caller_validated: bool,
    allow_internal: bool,
    registered_actors: &'vm HashSet<Cid>,
    circ_supply_calc: &'vm C,
    lb_state: &'vm LB,

    verifier: PhantomData<V>,
    params: PhantomData<P>,
}

impl<'db, 'vm, BS, R, C, LB, V, P> DefaultRuntime<'db, 'vm, BS, R, C, LB, V, P>
where
    BS: BlockStore,
    V: ProofVerifier,
    P: NetworkParams,
    R: Rand,
    C: CircSupplyCalc,
    LB: LookbackStateGetter<'db, BS>,
{
    /// Constructs a new Runtime
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        version: NetworkVersion,
        state: &'vm mut StateTree<'db, BS>,
        store: &'db BS,
        gas_used: i64,
        message: &UnsignedMessage,
        epoch: ChainEpoch,
        origin: Address,
        origin_nonce: u64,
        num_actors_created: u64,
        depth: u64,
        rand: &'vm R,
        registered_actors: &'vm HashSet<Cid>,
        circ_supply_calc: &'vm C,
        lb_state: &'vm LB,
    ) -> Result<Self, ActorError> {
        let price_list = price_list_by_epoch(epoch);
        let gas_tracker = Rc::new(RefCell::new(GasTracker::new(message.gas_limit(), gas_used)));
        let gas_block_store = GasBlockStore {
            price_list: price_list.clone(),
            gas: Rc::clone(&gas_tracker),
            store,
        };

        let caller_id = state
            .lookup_id(&message.from())
            .map_err(|e| e.downcast_fatal("failed to lookup id"))?
            .ok_or_else(|| {
                actor_error!(SysErrInvalidReceiver, "resolve msg from address failed")
            })?;

        let receiver = if version <= NetworkVersion::V3 {
            *message.to()
        } else {
            state
                .lookup_id(&message.to())
                .map_err(|e| e.downcast_fatal("failed to lookup id"))?
                // * Go implementation changes this to undef address. To avoid using optional
                // * value here, the non-id address is kept here (should never be used)
                .unwrap_or(*message.to())
        };

        let vm_msg = VMMsg {
            caller: caller_id,
            receiver,
            value_received: message.value().clone(),
        };

        Ok(DefaultRuntime {
            version,
            state,
            store: gas_block_store,
            gas_tracker,
            vm_msg,
            epoch,
            origin,
            origin_nonce,
            depth,
            num_actors_created,
            price_list,
            rand,
            registered_actors,
            circ_supply_calc,
            lb_state,
            allow_internal: true,
            caller_validated: false,
            params: PhantomData,
            verifier: PhantomData,
        })
    }

    /// Adds to amount of used
    /// * Will borrow gas tracker RefCell, do not call if any reference to this exists
    pub fn charge_gas(&mut self, gas: GasCharge) -> Result<(), ActorError> {
        self.gas_tracker.borrow_mut().charge_gas(gas)
    }

    /// Returns gas used by runtime
    /// * Will borrow gas tracker RefCell, do not call if a mutable reference exists
    pub fn gas_used(&self) -> i64 {
        self.gas_tracker.borrow().gas_used()
    }

    fn gas_available(&self) -> i64 {
        self.gas_tracker.borrow().gas_available()
    }

    /// Returns the price list for gas charges within the runtime
    pub fn price_list(&self) -> &PriceList {
        &self.price_list
    }

    /// Get the balance of a particular Actor from their Address
    fn get_balance(&self, addr: &Address) -> Result<BigInt, ActorError> {
        Ok(self
            .state
            .get_actor(&addr)
            .map_err(|e| e.downcast_fatal("failed to get actor in get balance"))?
            .map(|act| act.balance)
            .unwrap_or_default())
    }

    /// Update the state Cid of the Message receiver
    fn state_commit(&mut self, old_h: &Cid, new_h: Cid) -> Result<(), ActorError> {
        let to_addr = *self.message().receiver();
        let mut actor = self
            .state
            .get_actor(&to_addr)
            .map_err(|e| e.downcast_fatal("failed to get actor to commit state"))?
            .ok_or_else(|| actor_error!(fatal("failed to get actor to commit state")))?;

        if &actor.state != old_h {
            return Err(actor_error!(fatal(
                "failed to update, inconsistent base reference"
            )));
        }
        actor.state = new_h;
        self.state
            .set_actor(&to_addr, actor)
            .map_err(|e| e.downcast_fatal("failed to set actor in state_commit"))?;

        Ok(())
    }

    fn abort_if_already_validated(&mut self) -> Result<(), ActorError> {
        if self.caller_validated {
            Err(actor_error!(SysErrIllegalActor;
                    "Method must validate caller identity exactly once"))
        } else {
            self.caller_validated = true;
            Ok(())
        }
    }

    /// Helper function for inserting into blockstore.
    fn put<T>(&self, obj: &T) -> Result<Cid, ActorError>
    where
        T: Cbor,
    {
        self.store
            .put(obj, Blake2b256)
            .map_err(|e| e.downcast_fatal("failed to put cbor object"))
    }

    /// Helper function for getting deserializable objects from blockstore.
    fn get<T>(&self, cid: &Cid) -> Result<Option<T>, ActorError>
    where
        T: Cbor,
    {
        self.store
            .get(cid)
            .map_err(|e| e.downcast_fatal("failed to get cbor object"))
    }

    fn internal_send(
        &mut self,
        from: Address,
        to: Address,
        method: MethodNum,
        value: TokenAmount,
        params: Serialized,
    ) -> Result<Serialized, ActorError> {
        // ID must be resolved because otherwise would be done in creation of new runtime.
        // TODO revisit this later, it's possible there are no code paths this is needed.
        let from_id = self.resolve_address(&from)?.ok_or_else(|| {
            actor_error!(SysErrInvalidReceiver;
            "resolving from address in internal send failed")
        })?;

        let msg = UnsignedMessage {
            from,
            to,
            method_num: method,
            value: value.clone(),
            params,
            gas_limit: self.gas_available(),
            version: Default::default(),
            sequence: Default::default(),
            gas_fee_cap: Default::default(),
            gas_premium: Default::default(),
        };

        // snapshot state tree
        self.state
            .snapshot()
            .map_err(|e| actor_error!(fatal("failed to create snapshot: {}", e)))?;

        // Since it is unsafe to share a mutable reference to the state tree by copying
        // the runtime, all variables must be copied and reset at the end of the transition.
        let prev_val = self.caller_validated;
        let prev_depth = self.depth;
        let prev_msg = self.vm_msg.clone();
        self.vm_msg = VMMsg {
            caller: from_id,
            receiver: to,
            value_received: value,
        };
        self.caller_validated = false;
        self.depth += 1;
        if self.depth > MAX_CALL_DEPTH && self.network_version() >= NetworkVersion::V6 {
            return Err(actor_error!(
                SysErrForbidden,
                "message execution exceeds call depth"
            ));
        }

        let send_res = vm_send::<BS, R, C, LB, V, P>(self, &msg, None);

        // Reset values back to their values before the call
        self.vm_msg = prev_msg;
        self.caller_validated = prev_val;
        self.depth = prev_depth;

        let ret = send_res.map_err(|e| {
            if let Err(e) = self.state.revert_to_snapshot() {
                actor_error!(fatal("failed to revert snapshot: {}", e))
            } else {
                e
            }
        });
        if let Err(e) = self.state.clear_snapshot() {
            actor_error!(fatal("failed to clear snapshot: {}", e));
        }

        Ok(ret?)
    }

    /// creates account actors from only BLS/SECP256K1 addresses.
    pub fn try_create_account_actor(
        &mut self,
        addr: &Address,
    ) -> Result<(ActorState, Address), ActorError> {
        self.charge_gas(self.price_list().on_create_actor())?;

        let addr_id = self
            .state
            .register_new_address(addr)
            .map_err(|e| e.downcast_fatal("failed to register new address"))?;

        let version = ActorVersion::from(self.network_version());
        let act = make_actor(addr, version)?;

        self.state
            .set_actor(&addr_id, act)
            .map_err(|e| e.downcast_fatal("failed to set actor"))?;

        let p = Serialized::serialize(&addr).map_err(|e| {
            actor_error!(fatal(
                "couldn't serialize params for actor construction: {}",
                e
            ))
        })?;

        self.internal_send(
            **actor::system::ADDRESS,
            addr_id,
            account::Method::Constructor as u64,
            TokenAmount::from(0),
            p,
        )
        .map_err(|e| e.wrap("failed to invoke account constructor"))?;

        let act = self
            .state
            .get_actor(&addr_id)
            .map_err(|e| e.downcast_fatal("failed to get actor"))?
            .ok_or_else(|| actor_error!(fatal("failed to retrieve created actor state")))?;

        Ok((act, addr_id))
    }

    fn verify_block_signature(&self, bh: &BlockHeader) -> Result<(), Box<dyn StdError>> {
        let worker_addr = self.worker_key_at_lookback(bh.epoch(), bh.miner_address())?;

        bh.check_block_signature(&worker_addr)?;
        Ok(())
    }

    fn worker_key_at_lookback(
        &self,
        height: ChainEpoch,
        address: &Address,
    ) -> Result<Address, Box<dyn StdError>> {
        if self.network_version() >= NetworkVersion::V7
            && height < self.epoch - actor::CHAIN_FINALITY
        {
            return Err(format!(
                "cannot get worker key (current epoch: {}, height: {})",
                self.epoch, height
            )
            .into());
        }

        let lb_state = self.lb_state.state_lookback(height)?;
        let actor = lb_state
            // * @austinabell: Yes, this is intentional (should be updated with v3 actors though)
            .get_actor(self.vm_msg.receiver())?
            .ok_or_else(|| format!("actor not found {:?}", address))?;

        let ms = actor::miner::State::load(self.store(), &actor)?;

        let info = ms.info(&self.store)?;

        resolve_to_key_addr(&self.state, &self.store, &info.worker)
    }
}

impl<'bs, BS, R, CS, LB, V, P> Runtime<GasBlockStore<'bs, BS>>
    for DefaultRuntime<'bs, '_, BS, R, CS, LB, V, P>
where
    BS: BlockStore,
    V: ProofVerifier,
    P: NetworkParams,
    R: Rand,
    CS: CircSupplyCalc,
    LB: LookbackStateGetter<'bs, BS>,
{
    fn network_version(&self) -> NetworkVersion {
        self.version
    }
    fn message(&self) -> &dyn MessageInfo {
        &self.vm_msg
    }
    fn curr_epoch(&self) -> ChainEpoch {
        self.epoch
    }
    fn validate_immediate_caller_accept_any(&mut self) -> Result<(), ActorError> {
        self.abort_if_already_validated()
    }
    fn validate_immediate_caller_is<'db, I>(&mut self, addresses: I) -> Result<(), ActorError>
    where
        I: IntoIterator<Item = &'db Address>,
    {
        self.abort_if_already_validated()?;

        let imm = self.message().caller();

        // Check if theres is at least one match
        if !addresses.into_iter().any(|a| a == imm) {
            return Err(actor_error!(SysErrForbidden;
                "caller {} is not one of supported", self.message().caller()
            ));
        }
        Ok(())
    }

    fn validate_immediate_caller_type<'db, I>(&mut self, types: I) -> Result<(), ActorError>
    where
        I: IntoIterator<Item = &'db Cid>,
    {
        self.abort_if_already_validated()?;

        let caller_cid = self
            .get_actor_code_cid(self.message().caller())?
            .ok_or_else(|| actor_error!(fatal("failed to lookup code cid for caller")))?;
        if !types.into_iter().any(|c| *c == caller_cid) {
            return Err(actor_error!(SysErrForbidden;
                    "caller cid type {} not one of supported", caller_cid));
        }
        Ok(())
    }

    fn current_balance(&self) -> Result<TokenAmount, ActorError> {
        self.get_balance(self.message().receiver())
    }

    fn resolve_address(&self, address: &Address) -> Result<Option<Address>, ActorError> {
        self.state
            .lookup_id(&address)
            .map_err(|e| e.downcast_fatal("failed to look up id"))
    }

    fn get_actor_code_cid(&self, addr: &Address) -> Result<Option<Cid>, ActorError> {
        Ok(self
            .state
            .get_actor(&addr)
            .map_err(|e| e.downcast_fatal("failed to get actor"))?
            .map(|act| act.code))
    }

    fn get_randomness_from_tickets(
        &self,
        personalization: DomainSeparationTag,
        rand_epoch: ChainEpoch,
        entropy: &[u8],
    ) -> Result<Randomness, ActorError> {
        let r = self
            .rand
            .get_chain_randomness(personalization, rand_epoch, entropy)
            .map_err(|e| e.downcast_fatal("could not get randomness"))?;

        Ok(Randomness(r))
    }

    fn get_randomness_from_beacon(
        &self,
        personalization: DomainSeparationTag,
        rand_epoch: ChainEpoch,
        entropy: &[u8],
    ) -> Result<Randomness, ActorError> {
        let r = self
            .rand
            .get_beacon_randomness(personalization, rand_epoch, entropy)
            .map_err(|e| e.downcast_fatal("could not get randomness"))?;

        Ok(Randomness(r))
    }

    fn create<C: Cbor>(&mut self, obj: &C) -> Result<(), ActorError> {
        let c = self.put(obj)?;

        self.state_commit(&EMPTY_ARR_CID, c)
    }

    fn state<C: Cbor>(&self) -> Result<C, ActorError> {
        let actor = self
            .state
            .get_actor(self.message().receiver())
            .map_err(|e| {
                e.downcast_default(
                    ExitCode::SysErrIllegalArgument,
                    "failed to get actor for Readonly state",
                )
            })?
            .ok_or_else(
                || actor_error!(SysErrIllegalArgument; "Actor readonly state does not exist"),
            )?;

        // TODO revisit as the go impl doesn't handle not exists and nil cases
        self.get(&actor.state)?.ok_or_else(|| {
            actor_error!(fatal(
                "State does not exist for actor state cid: {}",
                actor.state
            ))
        })
    }

    fn transaction<C, RT, F>(&mut self, f: F) -> Result<RT, ActorError>
    where
        C: Cbor,
        F: FnOnce(&mut C, &mut Self) -> Result<RT, ActorError>,
    {
        // get actor
        let act = self
            .state
            .get_actor(self.message().receiver())
            .map_err(|e| {
                e.downcast_default(
                    ExitCode::SysErrIllegalActor,
                    "failed to get actor for transaction",
                )
            })?
            .ok_or_else(|| {
                actor_error!(SysErrIllegalActor;
                "actor state for transaction doesn't exist")
            })?;

        // get state for actor based on generic C
        let mut state: C = self
            .get(&act.state)?
            .ok_or_else(|| actor_error!(fatal("Actor state does not exist: {}", act.state)))?;

        // Update the state
        self.allow_internal = false;
        let r = f(&mut state, self);
        self.allow_internal = true;

        // Return error after allow_internal is reset
        let r = r?;

        let c = self.put(&state)?;

        // Committing that change
        self.state_commit(&act.state, c)?;
        Ok(r)
    }

    fn store(&self) -> &GasBlockStore<'bs, BS> {
        &self.store
    }

    fn send(
        &mut self,
        to: Address,
        method: MethodNum,
        params: Serialized,
        value: TokenAmount,
    ) -> Result<Serialized, ActorError> {
        if !self.allow_internal {
            return Err(actor_error!(SysErrIllegalActor; "runtime.send() is not allowed"));
        }

        let ret = self
            .internal_send(*self.message().receiver(), to, method, value, params)
            .map_err(|e| {
                warn!(
                    "internal send failed: (to: {}) (method: {}) {}",
                    to, method, e
                );
                e
            })?;

        Ok(ret)
    }
    fn new_actor_address(&mut self) -> Result<Address, ActorError> {
        // ! Go implementation doesn't handle the error for some reason here and will panic
        let oa = resolve_to_key_addr(self.state, self.store.store, &self.origin)
            .map_err(|e| e.downcast_fatal("failed to resolve key addr"))?;
        let mut b = to_vec(&oa).map_err(|e| {
            actor_error!(fatal(
                "Could not serialize address in new_actor_address: {}",
                e
            ))
        })?;
        b.write_u64::<BigEndian>(self.origin_nonce)
            .map_err(|e| actor_error!(fatal("Writing nonce address into a buffer: {}", e)))?;
        b.write_u64::<BigEndian>(self.num_actors_created)
            .map_err(|e| {
                actor_error!(fatal(
                    "Writing number of actors created into a buffer: {}",
                    e
                ))
            })?;
        let addr = Address::new_actor(&b);
        self.num_actors_created += 1;
        Ok(addr)
    }
    fn create_actor(&mut self, code_id: Cid, address: &Address) -> Result<(), ActorError> {
        if !actor::is_builtin_actor(&code_id) {
            return Err(actor_error!(SysErrIllegalArgument; "Can only create built-in actors."));
        }

        if actor::is_singleton_actor(&code_id) {
            return Err(actor_error!(SysErrIllegalArgument;
                    "Can only have one instance of singleton actors."));
        }

        if let Ok(Some(_)) = self.state.get_actor(address) {
            return Err(actor_error!(SysErrIllegalArgument; "Actor address already exists"));
        }

        self.charge_gas(self.price_list.on_create_actor())?;
        self.state
            .set_actor(
                &address,
                ActorState::new(code_id, *EMPTY_ARR_CID, 0.into(), 0),
            )
            .map_err(|e| e.downcast_fatal("creating actor entry"))
    }

    /// DeleteActor deletes the executing actor from the state tree, transferring
    /// any balance to beneficiary.
    /// Aborts if the beneficiary does not exist.
    /// May only be called by the actor itself.
    fn delete_actor(&mut self, beneficiary: &Address) -> Result<(), ActorError> {
        self.charge_gas(self.price_list.on_delete_actor())?;
        let receiver = *self.message().receiver();
        let balance = self
            .state
            .get_actor(&receiver)
            .map_err(|e| e.downcast_fatal(format!("failed to get actor {}", receiver)))?
            .ok_or_else(|| actor_error!(SysErrIllegalActor; "failed to load actor in delete actor"))
            .map(|act| act.balance)?;
        if balance != 0.into() {
            // Transfer the executing actor's balance to the beneficiary
            transfer(self.state, &receiver, beneficiary, &balance)
                .map_err(|e| e.wrap("failed to transfer balance to beneficiary actor"))?;
        }

        // Delete the executing actor
        self.state
            .delete_actor(&receiver)
            .map_err(|e| e.downcast_fatal("failed to delete actor"))
    }
    fn total_fil_circ_supply(&self) -> Result<TokenAmount, ActorError> {
        self.circ_supply_calc
            .get_supply(self.epoch, self.state)
            .map_err(|e| actor_error!(ErrIllegalState, "failed to get total circ supply: {}", e))
    }
    fn charge_gas(&mut self, name: &'static str, compute: i64) -> Result<(), ActorError> {
        self.charge_gas(GasCharge::new(name, compute, 0))
    }
}

impl<'bs, BS, R, C, LB, V, P> Syscalls for DefaultRuntime<'bs, '_, BS, R, C, LB, V, P>
where
    BS: BlockStore,
    V: ProofVerifier,
    P: NetworkParams,
    R: Rand,
    C: CircSupplyCalc,
    LB: LookbackStateGetter<'bs, BS>,
{
    fn verify_signature(
        &self,
        signature: &Signature,
        signer: &Address,
        plaintext: &[u8],
    ) -> Result<(), Box<dyn StdError>> {
        self.gas_tracker.borrow_mut().charge_gas(
            self.price_list
                .on_verify_signature(signature.signature_type()),
        )?;

        // Resolve to key address before verifying signature.
        let signing_addr = resolve_to_key_addr(self.state, &self.store, signer)?;
        Ok(signature.verify(plaintext, &signing_addr)?)
    }
    fn hash_blake2b(&self, data: &[u8]) -> Result<[u8; 32], Box<dyn StdError>> {
        self.gas_tracker
            .borrow_mut()
            .charge_gas(self.price_list.on_hashing(data.len()))?;

        Ok(blake2b_256(data))
    }
    fn compute_unsealed_sector_cid(
        &self,
        reg: RegisteredSealProof,
        pieces: &[PieceInfo],
    ) -> Result<Cid, Box<dyn StdError>> {
        self.gas_tracker
            .borrow_mut()
            .charge_gas(self.price_list.on_compute_unsealed_sector_cid(reg, pieces))?;

        compute_unsealed_sector_cid(reg, pieces)
    }
    fn verify_seal(&self, vi: &SealVerifyInfo) -> Result<(), Box<dyn StdError>> {
        self.gas_tracker
            .borrow_mut()
            .charge_gas(self.price_list.on_verify_seal(vi))?;

        V::verify_seal(vi)
    }
    fn verify_post(&self, vi: &WindowPoStVerifyInfo) -> Result<(), Box<dyn StdError>> {
        self.gas_tracker
            .borrow_mut()
            .charge_gas(self.price_list.on_verify_post(vi))?;

        V::verify_window_post(vi.randomness, &vi.proofs, &vi.challenged_sectors, vi.prover)
    }
    fn verify_consensus_fault(
        &self,
        h1: &[u8],
        h2: &[u8],
        extra: &[u8],
    ) -> Result<Option<ConsensusFault>, Box<dyn StdError>> {
        self.gas_tracker
            .borrow_mut()
            .charge_gas(self.price_list.on_verify_consensus_fault())?;

        // Note that block syntax is not validated. Any validly signed block will be accepted pursuant to the below conditions.
        // Whether or not it could ever have been accepted in a chain is not checked/does not matter here.
        // for that reason when checking block parent relationships, rather than instantiating a Tipset to do so
        // (which runs a syntactic check), we do it directly on the CIDs.

        // (0) cheap preliminary checks

        if h1 == h2 {
            return Err(format!(
                "no consensus fault: submitted blocks are the same: {:?}, {:?}",
                h1, h2
            )
            .into());
        };
        let bh_1 = BlockHeader::unmarshal_cbor(h1)?;
        let bh_2 = BlockHeader::unmarshal_cbor(h2)?;

        // (1) check conditions necessary to any consensus fault

        if bh_1.miner_address() != bh_2.miner_address() {
            return Err(format!(
                "no consensus fault: blocks not mined by same miner: {:?}, {:?}",
                bh_1.miner_address(),
                bh_2.miner_address()
            )
            .into());
        };
        // block a must be earlier or equal to block b, epoch wise (ie at least as early in the chain).
        if bh_1.epoch() < bh_2.epoch() {
            return Err(format!(
                "first block must not be of higher height than second: {:?}, {:?}",
                bh_1.epoch(),
                bh_2.epoch()
            )
            .into());
        };
        let mut cf: Option<ConsensusFault> = None;
        // (a) double-fork mining fault
        if bh_1.epoch() == bh_2.epoch() {
            cf = Some(ConsensusFault {
                target: *bh_1.miner_address(),
                epoch: bh_2.epoch(),
                fault_type: ConsensusFaultType::DoubleForkMining,
            })
        };
        // (b) time-offset mining fault
        // strictly speaking no need to compare heights based on double fork mining check above,
        // but at same height this would be a different fault.
        if bh_1.parents() != bh_2.parents() && bh_1.epoch() != bh_2.epoch() {
            cf = Some(ConsensusFault {
                target: *bh_1.miner_address(),
                epoch: bh_2.epoch(),
                fault_type: ConsensusFaultType::TimeOffsetMining,
            })
        };
        // (c) parent-grinding fault
        // Here extra is the "witness", a third block that shows the connection between A and B as
        // A's sibling and B's parent.
        // Specifically, since A is of lower height, it must be that B was mined omitting A from its tipset
        if !extra.is_empty() {
            let bh_3 = BlockHeader::unmarshal_cbor(extra)?;
            if bh_1.parents() != bh_3.parents()
                && bh_1.epoch() != bh_3.epoch()
                && bh_2.parents().cids().contains(bh_3.cid())
                && !bh_2.parents().cids().contains(bh_1.cid())
            {
                cf = Some(ConsensusFault {
                    target: *bh_1.miner_address(),
                    epoch: bh_2.epoch(),
                    fault_type: ConsensusFaultType::ParentGrinding,
                })
            }
        };

        // (3) return if no consensus fault by now
        if cf.is_none() {
            Ok(cf)
        } else {
            // (4) expensive final checks

            // check blocks are properly signed by their respective miner
            // note we do not need to check extra's: it is a parent to block b
            // which itself is signed, so it was willingly included by the miner
            self.verify_block_signature(&bh_1)?;
            self.verify_block_signature(&bh_2)?;

            Ok(cf)
        }
    }

    fn batch_verify_seals(
        &self,
        vis: &[(&Address, &Vec<SealVerifyInfo>)],
    ) -> Result<HashMap<Address, Vec<bool>>, Box<dyn StdError>> {
        // Gas charged for batch verify in actor

        let out = vis
            .par_iter()
            .map(|(&addr, seals)| {
                let results = seals
                    .par_iter()
                    .map(|s| {
                        if let Err(err) = V::verify_seal(s) {
                            warn!(
                                "seal verify in batch failed (miner: {}) (err: {})",
                                addr, err
                            );
                            false
                        } else {
                            true
                        }
                    })
                    .collect();
                (addr, results)
            })
            .collect();
        Ok(out)
    }
}

/// Shared logic between the DefaultRuntime and the Interpreter.
/// It invokes methods on different Actors based on the Message.
pub fn vm_send<'db, 'vm, BS, R, C, LB, V, P>(
    rt: &mut DefaultRuntime<'db, 'vm, BS, R, C, LB, V, P>,
    msg: &UnsignedMessage,
    gas_cost: Option<GasCharge>,
) -> Result<Serialized, ActorError>
where
    BS: BlockStore,
    V: ProofVerifier,
    P: NetworkParams,
    R: Rand,
    C: CircSupplyCalc,
    LB: LookbackStateGetter<'db, BS>,
{
    if let Some(cost) = gas_cost {
        rt.charge_gas(cost)?;
    }

    let to_actor = match rt
        .state
        .get_actor(msg.to())
        .map_err(|e| e.downcast_fatal("failed to get actor"))?
    {
        Some(act) => act,
        None => {
            // Try to create actor if not exist
            let (to_actor, id_addr) = rt.try_create_account_actor(msg.to())?;
            if rt.network_version() > NetworkVersion::V3 {
                // Update the receiver to the created ID address
                rt.vm_msg.receiver = id_addr;
            }
            to_actor
        }
    };

    rt.charge_gas(
        rt.price_list()
            .on_method_invocation(msg.value(), msg.method_num()),
    )?;

    if !msg.value().is_zero() {
        transfer(rt.state, &msg.from(), &msg.to(), &msg.value())
            .map_err(|e| e.wrap("failed to transfer funds"))?;
    }

    if msg.method_num() != METHOD_SEND {
        rt.charge_gas(ACTOR_EXEC_GAS)?;
        return invoke(rt, to_actor.code, msg.method_num(), msg.params(), msg.to());
    }

    Ok(Serialized::default())
}

/// Transfers funds from one Actor to another Actor
fn transfer<BS: BlockStore>(
    state: &mut StateTree<BS>,
    from: &Address,
    to: &Address,
    value: &TokenAmount,
) -> Result<(), ActorError> {
    if from == to {
        return Ok(());
    }

    let from_id = state
        .lookup_id(from)
        .map_err(|e| e.downcast_fatal("failed to lookup from id for address"))?
        .ok_or_else(|| actor_error!(fatal("Failed to lookup from id for address {}", from)))?;
    let to_id = state
        .lookup_id(to)
        .map_err(|e| e.downcast_fatal("failed to lookup to id for address"))?
        .ok_or_else(|| actor_error!(fatal("Failed to lookup to id for address {}", to)))?;

    if from_id == to_id {
        return Ok(());
    }

    if value < &0.into() {
        return Err(actor_error!(SysErrForbidden;
                "attempted to transfer negative transfer value {}", value));
    }

    let mut f = state
        .get_actor(&from_id)
        .map_err(|e| e.downcast_fatal("failed to get actor"))?
        .ok_or_else(|| {
            actor_error!(fatal(
                "sender actor does not exist in state during transfer"
            ))
        })?;
    let mut t = state
        .get_actor(&to_id)
        .map_err(|e| e.downcast_fatal("failed to get actor: {}"))?
        .ok_or_else(|| {
            actor_error!(fatal(
                "receiver actor does not exist in state during transfer"
            ))
        })?;

    f.deduct_funds(&value).map_err(|e| {
        actor_error!(SysErrInsufficientFunds;
        "transfer failed when deducting funds ({}): {}", value, e)
    })?;
    t.deposit_funds(&value);

    state
        .set_actor(from, f)
        .map_err(|e| e.downcast_fatal("failed to set from actor"))?;
    state
        .set_actor(to, t)
        .map_err(|e| e.downcast_fatal("failed to set to actor"))?;

    Ok(())
}

/// Calls actor code with method and parameters.
fn invoke<'db, 'vm, BS, R, C, LB, V, P>(
    rt: &mut DefaultRuntime<'db, 'vm, BS, R, C, LB, V, P>,
    code: Cid,
    method_num: MethodNum,
    params: &Serialized,
    to: &Address,
) -> Result<Serialized, ActorError>
where
    BS: BlockStore,
    V: ProofVerifier,
    P: NetworkParams,
    R: Rand,
    C: CircSupplyCalc,
    LB: LookbackStateGetter<'db, BS>,
{
    let ret = if let Some(ret) = {
        match actor::ActorVersion::from(rt.network_version()) {
            ActorVersion::V0 => actorv0::invoke_code(&code, rt, method_num, params),
            ActorVersion::V2 => actorv2::invoke_code(&code, rt, method_num, params),
        }
    } {
        ret
    } else if code == *actorv2::CHAOS_ACTOR_CODE_ID && rt.registered_actors.contains(&code) {
        actorv2::chaos::Actor::invoke_method(rt, method_num, params)
    } else {
        Err(actor_error!(
            SysErrIllegalActor,
            "no code for actor at address {}",
            to
        ))
    }?;

    if !rt.caller_validated {
        Err(actor_error!(SysErrIllegalActor; "Caller must be validated during method execution"))
    } else {
        Ok(ret)
    }
}

/// returns the public key type of address (`BLS`/`SECP256K1`) of an account actor
/// identified by `addr`.
pub fn resolve_to_key_addr<'st, 'bs, BS, S>(
    st: &'st StateTree<'bs, S>,
    store: &'bs BS,
    addr: &Address,
) -> Result<Address, Box<dyn StdError>>
where
    BS: BlockStore,
    S: BlockStore,
{
    if addr.protocol() == Protocol::BLS || addr.protocol() == Protocol::Secp256k1 {
        return Ok(*addr);
    }

    let act = st
        .get_actor(&addr)
        .map_err(|e| e.downcast_wrap("Failed to get actor"))?
        .ok_or_else(|| format!("Failed to retrieve actor: {}", addr))?;

    let acc_st = account::State::load(store, &act)?;

    Ok(acc_st.pubkey_address())
}

fn make_actor(addr: &Address, version: ActorVersion) -> Result<ActorState, ActorError> {
    match addr.protocol() {
        Protocol::BLS | Protocol::Secp256k1 => Ok(new_account_actor(version)),
        Protocol::ID => {
            Err(actor_error!(SysErrInvalidReceiver; "no actor with given id: {}", addr))
        }
        Protocol::Actor => Err(actor_error!(SysErrInvalidReceiver; "no such actor: {}", addr)),
    }
}

fn new_account_actor(version: ActorVersion) -> ActorState {
    ActorState {
        code: match version {
            ActorVersion::V0 => *actorv0::ACCOUNT_ACTOR_CODE_ID,
            ActorVersion::V2 => *actorv2::ACCOUNT_ACTOR_CODE_ID,
        },
        balance: TokenAmount::from(0),
        state: *EMPTY_ARR_CID,
        sequence: 0,
    }
}
