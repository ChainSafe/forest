// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::ops::{Deref, DerefMut};

use anyhow::{bail, Context};
use cid::Cid;
use fvm::state_tree::{ActorState as ActorStateV2, StateTree as StateTreeV2};
use fvm3::state_tree::{ActorState as ActorStateV3, StateTree as StateTreeV3};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::repr::{Deserialize_repr, Serialize_repr};
use fvm_shared::state::StateTreeVersion as StateTreeVersionV2;
use fvm_shared3::state::StateTreeVersion as StateTreeVersionV3;
pub use fvm_shared3::ActorID;
use num::FromPrimitive;
use num_derive::FromPrimitive;
use serde::{Deserialize, Serialize};

use crate::shim::{address::Address, econ::TokenAmount, Inner};

#[derive(
    Debug, PartialEq, Eq, Clone, Copy, PartialOrd, Serialize_repr, Deserialize_repr, FromPrimitive,
)]
#[repr(u64)]
pub enum StateTreeVersion {
    V0,
    V1,
    V2,
    V3,
    V4,
    V5,
}

impl TryFrom<StateTreeVersionV3> for StateTreeVersion {
    type Error = anyhow::Error;
    fn try_from(value: StateTreeVersionV3) -> anyhow::Result<Self> {
        if let Some(v) = FromPrimitive::from_u32(value as u32) {
            Ok(v)
        } else {
            bail!("Invalid conversion");
        }
    }
}

impl TryFrom<StateTreeVersionV2> for StateTreeVersion {
    type Error = anyhow::Error;
    fn try_from(value: StateTreeVersionV2) -> anyhow::Result<Self> {
        if let Some(v) = FromPrimitive::from_u32(value as u32) {
            Ok(v)
        } else {
            bail!("Invalid conversion");
        }
    }
}

impl TryFrom<StateTreeVersion> for StateTreeVersionV2 {
    type Error = anyhow::Error;

    fn try_from(value: StateTreeVersion) -> anyhow::Result<Self> {
        Ok(match value {
            StateTreeVersion::V0 => StateTreeVersionV2::V0,
            StateTreeVersion::V1 => StateTreeVersionV2::V1,
            StateTreeVersion::V2 => StateTreeVersionV2::V2,
            StateTreeVersion::V3 => StateTreeVersionV2::V3,
            StateTreeVersion::V4 => StateTreeVersionV2::V4,
            StateTreeVersion::V5 => bail!("Impossible conversion"),
        })
    }
}

impl TryFrom<StateTreeVersion> for StateTreeVersionV3 {
    type Error = anyhow::Error;

    fn try_from(value: StateTreeVersion) -> anyhow::Result<Self> {
        Ok(match value {
            StateTreeVersion::V0 => StateTreeVersionV3::V0,
            StateTreeVersion::V1 => StateTreeVersionV3::V1,
            StateTreeVersion::V2 => StateTreeVersionV3::V2,
            StateTreeVersion::V3 => StateTreeVersionV3::V3,
            StateTreeVersion::V4 => StateTreeVersionV3::V4,
            StateTreeVersion::V5 => StateTreeVersionV3::V5,
        })
    }
}

/// FVM `StateTree` variant. The `new_from_root` constructor will try to resolve
/// to a valid `StateTree` version or fail if we don't support it at the moment.
/// Other methods usage should be transparent (using shimmed versions of
/// structures introduced in this crate::shim.
///
/// Not all the inner methods are implemented, only those that are needed. Feel
/// free to add those when necessary.
pub enum StateTree<S> {
    V0(state_tree_v0::StateTreeV0<S>),
    V2(StateTreeV2<S>),
    V3(StateTreeV3<S>),
}

impl<S> StateTree<S>
where
    S: Blockstore + Clone,
{
    /// Constructor for a HAMT state tree given an IPLD store
    pub fn new(store: S, version: StateTreeVersion) -> anyhow::Result<Self> {
        if let Ok(st) = StateTreeV3::new(store.clone(), version.try_into()?) {
            Ok(StateTree::V3(st))
        } else if let Ok(st) = StateTreeV2::new(store, version.try_into()?) {
            Ok(StateTree::V2(st))
        } else {
            bail!("Can't create a valid state tree for the given version.");
        }
    }

    pub fn new_from_root(store: S, c: &Cid) -> anyhow::Result<Self> {
        if let Ok(st) = StateTreeV3::new_from_root(store.clone(), c) {
            Ok(StateTree::V3(st))
        } else if let Ok(st) = StateTreeV2::new_from_root(store.clone(), c) {
            Ok(StateTree::V2(st))
        } else if let Ok(st) = state_tree_v0::StateTreeV0::new_from_root(store, c) {
            Ok(StateTree::V0(st))
        } else {
            bail!("Can't create a valid state tree from the given root. This error may indicate unsupported version.")
        }
    }

    /// Get actor state from an address. Will be resolved to ID address.
    pub fn get_actor(&self, addr: &Address) -> anyhow::Result<Option<ActorState>> {
        match self {
            StateTree::V2(st) => Ok(st
                .get_actor(&addr.into())
                .map_err(|e| anyhow::anyhow!("{e}"))?
                .map(Into::into)),
            StateTree::V3(st) => {
                let id = st.lookup_id(addr)?;
                if let Some(id) = id {
                    Ok(st
                        .get_actor(id)
                        .map_err(|e| anyhow::anyhow!("{e}"))?
                        .map(Into::into))
                } else {
                    Ok(None)
                }
            }
            StateTree::V0(st) => {
                let id = st.lookup_id(addr)?;
                if let Some(id) = id {
                    Ok(st
                        .get_actor(&id)
                        .map_err(|e| anyhow::anyhow!("{e}"))?
                        .map(Into::into))
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Retrieve store reference to modify db.
    pub fn store(&self) -> &S {
        match self {
            StateTree::V2(st) => st.store(),
            StateTree::V3(st) => st.store(),
            StateTree::V0(st) => st.store(),
        }
    }

    /// Get an ID address from any Address
    pub fn lookup_id(&self, addr: &Address) -> anyhow::Result<Option<ActorID>> {
        match self {
            StateTree::V2(st) => st
                .lookup_id(&addr.into())
                .map_err(|e| anyhow::anyhow!("{e}")),
            StateTree::V3(st) => Ok(st.lookup_id(&addr.into())?),
            _ => todo!(),
        }
    }

    pub fn for_each<F>(&self, mut f: F) -> anyhow::Result<()>
    where
        F: FnMut(Address, &ActorState) -> anyhow::Result<()>,
    {
        match self {
            StateTree::V2(st) => {
                let inner = |address: fvm_shared::address::Address, actor_state: &ActorStateV2| {
                    f(address.into(), &actor_state.into())
                };
                st.for_each(inner)
            }
            StateTree::V3(st) => {
                let inner = |address: fvm_shared3::address::Address, actor_state: &ActorStateV3| {
                    f(address.into(), &actor_state.into())
                };
                st.for_each(inner)
            }
            _ => todo!(),
        }
    }

    /// Flush state tree and return Cid root.
    pub fn flush(&mut self) -> anyhow::Result<Cid> {
        match self {
            StateTree::V2(st) => st.flush().map_err(|e| anyhow::anyhow!("{e}")),
            StateTree::V3(st) => Ok(st.flush()?),
            _ => todo!(),
        }
    }

    /// Set actor state with an actor ID.
    pub fn set_actor(&mut self, addr: &Address, actor: ActorState) -> anyhow::Result<()> {
        match self {
            StateTree::V2(st) => st
                .set_actor(&addr.into(), actor.into())
                .map_err(|e| anyhow::anyhow!("{e}")),
            StateTree::V3(st) => {
                let id = st
                    .lookup_id(&addr.into())?
                    .context("couldn't find actor id")?;
                st.set_actor(id, actor.into());
                Ok(())
            }
            _ => todo!(),
        }
    }
}

/// `Newtype` to wrap different versions of `fvm::state_tree::ActorState`
///
/// # Examples
/// ```
/// # use forest_filecoin::doctest_private::ActorState;
/// use cid::Cid;
///
/// // Create FVM2 ActorState normally
/// let fvm2_actor_state = fvm::state_tree::ActorState::new(Cid::default(), Cid::default(),
/// fvm_shared::econ::TokenAmount::from_atto(42), 0);
///
/// // Create a correspndoning FVM3 ActorState
/// let fvm3_actor_state = fvm3::state_tree::ActorState::new(Cid::default(), Cid::default(),
/// fvm_shared3::econ::TokenAmount::from_atto(42), 0, None);
///
/// // Create a shim out of fvm2 state, ensure conversions are correct
/// let state_shim = ActorState::from(fvm2_actor_state.clone());
/// assert_eq!(fvm3_actor_state, *state_shim);
/// assert_eq!(fvm2_actor_state, state_shim.into());
/// ```
#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActorState(ActorStateV3);

impl ActorState {
    pub fn new(
        code: Cid,
        state: Cid,
        balance: TokenAmount,
        sequence: u64,
        address: Option<Address>,
    ) -> Self {
        Self(ActorStateV3::new(
            code,
            state,
            balance.into(),
            sequence,
            address.map(Into::into),
        ))
    }
    /// Construct a new empty actor with the specified code.
    pub fn new_empty(code: Cid, delegated_address: Option<Address>) -> Self {
        Self(ActorStateV3::new_empty(
            code,
            delegated_address.map(Into::into),
        ))
    }
}
impl Inner for ActorState {
    type FVM = ActorStateV3;
}

impl Deref for ActorState {
    type Target = ActorStateV3;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ActorState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<ActorStateV3> for ActorState {
    fn from(value: ActorStateV3) -> Self {
        ActorState(value)
    }
}

impl From<&ActorStateV3> for ActorState {
    fn from(value: &ActorStateV3) -> Self {
        ActorState(value.clone())
    }
}

impl From<ActorStateV2> for ActorState {
    fn from(value: ActorStateV2) -> Self {
        ActorState(ActorStateV3 {
            code: value.code,
            state: value.state,
            sequence: value.sequence,
            balance: TokenAmount::from(value.balance).into(),
            delegated_address: None,
        })
    }
}

impl From<&ActorStateV2> for ActorState {
    fn from(value: &ActorStateV2) -> Self {
        ActorState(ActorStateV3 {
            code: value.code,
            state: value.state,
            sequence: value.sequence,
            balance: TokenAmount::from(&value.balance).into(),
            delegated_address: None,
        })
    }
}

impl From<ActorState> for ActorStateV3 {
    fn from(other: ActorState) -> Self {
        other.0
    }
}

impl From<ActorState> for ActorStateV2 {
    fn from(other: ActorState) -> ActorStateV2 {
        ActorStateV2 {
            code: other.code,
            state: other.state,
            sequence: other.sequence,
            balance: TokenAmount::from(&other.balance).into(),
        }
    }
}

impl From<&ActorState> for ActorStateV2 {
    fn from(other: &ActorState) -> ActorStateV2 {
        ActorStateV2 {
            code: other.code,
            state: other.state,
            sequence: other.sequence,
            balance: TokenAmount::from(&other.balance).into(),
        }
    }
}

impl From<state_tree_v0::ActorState> for ActorState {
    fn from(value: state_tree_v0::ActorState) -> Self {
        ActorState(ActorStateV3 {
            code: value.code,
            state: value.state,
            sequence: value.sequence,
            balance: value.balance.into(),
            delegated_address: None,
        })
    }
}

impl quickcheck::Arbitrary for ActorState {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        ActorState(ActorStateV3::arbitrary(g))
    }
}

// ported from commit hash b622af
pub mod state_tree_v0 {
    use cid::Cid;
    use fvm_ipld_blockstore::Blockstore;
    use fvm_ipld_encoding::repr::*;
    use fvm_ipld_encoding::tuple::*;
    use fvm_ipld_encoding3::CborStore;

    use crate::shim::address::Address;
    use crate::shim::econ::TokenAmount;
    use crate::shim::hamtv0::Hamt;
    use crate::shim::hamtv0::DEFAULT_BIT_WIDTH;

    /// State of all actor implementations.
    #[derive(PartialEq, Eq, Clone, Debug, Serialize_tuple, Deserialize_tuple)]
    pub struct ActorState {
        /// Link to code for the actor.
        pub code: Cid,
        /// Link to the state of the actor.
        pub state: Cid,
        /// Sequence of the actor.
        pub sequence: u64,
        /// Tokens available to the actor.
        pub balance: TokenAmount,
    }

    /// State tree implementation using HAMT. This structure is not thread safe and should only be used
    /// in sync contexts.
    pub struct StateTreeV0<S> {
        hamt: Hamt<S, ActorState>,

        _version: StateTreeVersion,
        _info: Option<Cid>,
        // /// State cache
        // snaps: StateSnapshots, // XXX: This is needed when reading writing state tree v0. As present we only implement the
        // v0 version of state tree usable enough to support the STATE_NETWORK_NAME RPC API.
    }

    /// Specifies the version of the state tree
    #[derive(Debug, PartialEq, Clone, Copy, PartialOrd, Serialize_repr, Deserialize_repr)]
    #[repr(u64)]
    pub enum StateTreeVersion {
        /// Corresponds to actors less than version 2
        V0,
        /// Corresponds to actors equal to version 2
        V1,
        /// Corresponds to actors equal to version 3
        V2,
        /// Corresponds to actors equal to version 4
        V3,
        /// Corresponds to actors greater than or equal to version 5
        V4,
    }

    /// State root information. Contains information about the version of the state tree,
    /// the root of the tree, and a link to the information about the tree.
    #[derive(Deserialize_tuple, Serialize_tuple)]
    pub struct StateRoot {
        /// State tree version
        pub version: StateTreeVersion,

        /// Actors tree. The structure depends on the state root version.
        pub actors: Cid,

        /// Info. The structure depends on the state root version.
        pub info: Cid,
    }

    impl<S> StateTreeV0<S>
    where
        S: Blockstore,
    {
        /// Constructor for a HAMT state tree given an IPLD store
        pub fn new_from_root(store: S, c: &Cid) -> anyhow::Result<Self> {
            // Try to load state root, if versioned
            let (version, info, actors) = if let Ok(Some(StateRoot {
                version,
                info,
                actors,
            })) = store.get_cbor(c)
            {
                (version, Some(info), actors)
            } else {
                // Fallback to v0 state tree if retrieval fails
                (StateTreeVersion::V0, None, *c)
            };

            match version {
                StateTreeVersion::V0 => {
                    let hamt: Hamt<S, ActorState> =
                        Hamt::load_with_bit_width(&actors, store, DEFAULT_BIT_WIDTH)?;

                    Ok(Self {
                        hamt,
                        _version: version,
                        _info: info,
                    })
                }
                _ => unreachable!("expecting state tree version 0"),
            }
        }

        /// Retrieve store reference to modify db.
        pub fn store(&self) -> &S {
            self.hamt.store()
        }

        /// Get actor state from an address. Will be resolved to ID address.
        pub fn get_actor(&self, addr: &Address) -> anyhow::Result<Option<ActorState>> {
            let addr = match self.lookup_id(addr)? {
                Some(addr) => addr,
                None => return Ok(None),
            };

            // if state doesn't exist, find using hamt
            let act = self.hamt.get(&addr.to_bytes())?.cloned();

            Ok(act)
        }

        /// Get an ID address from any Address
        pub fn lookup_id(&self, addr: &Address) -> anyhow::Result<Option<Address>> {
            if addr.protocol() == fvm_shared3::address::Protocol::ID {
                return Ok(Some(*addr));
            }

            let init_act = self
                .get_actor(&Address::INIT_ACTOR)?
                .ok_or(anyhow::anyhow!("Init actor address could not be resolved"))?;

            let _state = fil_actor_interface::init::State::load(
                self.hamt.store(),
                init_act.code,
                init_act.state,
            )?;

            // XXX: can be fixed by adding resolve_method in fil-actor-states
            todo!("resolve_address method required on init state.")
        }
    }
}
