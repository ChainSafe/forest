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
        } else if let Ok(st) = StateTreeV2::new_from_root(store, c) {
            Ok(StateTree::V2(st))
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
        }
    }

    /// Retrieve store reference to modify db.
    pub fn store(&self) -> &S {
        match self {
            StateTree::V2(st) => st.store(),
            StateTree::V3(st) => st.store(),
        }
    }

    /// Get an ID address from any Address
    pub fn lookup_id(&self, addr: &Address) -> anyhow::Result<Option<ActorID>> {
        match self {
            StateTree::V2(st) => st
                .lookup_id(&addr.into())
                .map_err(|e| anyhow::anyhow!("{e}")),
            StateTree::V3(st) => Ok(st.lookup_id(&addr.into())?),
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
        }
    }

    /// Flush state tree and return Cid root.
    pub fn flush(&mut self) -> anyhow::Result<Cid> {
        match self {
            StateTree::V2(st) => st.flush().map_err(|e| anyhow::anyhow!("{e}")),
            StateTree::V3(st) => Ok(st.flush()?),
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

impl quickcheck::Arbitrary for ActorState {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let cid = Cid::new_v1(
            u64::arbitrary(g),
            cid::multihash::Multihash::wrap(u64::arbitrary(g), &[u8::arbitrary(g)]).unwrap(),
        );
        ActorState::new(cid, cid, TokenAmount::arbitrary(g), u64::arbitrary(g), None)
    }
}
