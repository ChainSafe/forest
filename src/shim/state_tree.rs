// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::actors::LoadActorStateFromBlockstore;
pub use super::fvm_shared_latest::{ActorID, state::StateRoot};
use crate::{
    blocks::Tipset,
    shim::{actors::AccountActorStateLoad as _, address::Address, econ::TokenAmount},
};
use crate::{
    networks::{ACTOR_BUNDLES_METADATA, ActorBundleMetadata},
    shim::actors::account,
};
use anyhow::{Context as _, anyhow, bail};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{
    CborStore as _,
    repr::{Deserialize_repr, Serialize_repr},
};
use fvm_shared2::state::StateTreeVersion as StateTreeVersionV2;
use fvm_shared3::state::StateTreeVersion as StateTreeVersionV3;
use fvm_shared4::state::StateTreeVersion as StateTreeVersionV4;
pub use fvm2::state_tree::{ActorState as ActorStateV2, StateTree as StateTreeV2};
pub use fvm3::state_tree::{ActorState as ActorStateV3, StateTree as StateTreeV3};
pub use fvm4::state_tree::{
    ActorState as ActorStateV4, ActorState as ActorState_latest, StateTree as StateTreeV4,
};
use num::FromPrimitive;
use num_derive::FromPrimitive;
use serde::{Deserialize, Serialize};
use spire_enum::prelude::delegated_enum;
use std::sync::Arc;

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

impl From<StateTreeVersionV4> for StateTreeVersion {
    fn from(value: StateTreeVersionV4) -> Self {
        match value {
            StateTreeVersionV4::V0 => Self::V0,
            StateTreeVersionV4::V1 => Self::V1,
            StateTreeVersionV4::V2 => Self::V2,
            StateTreeVersionV4::V3 => Self::V3,
            StateTreeVersionV4::V4 => Self::V4,
            StateTreeVersionV4::V5 => Self::V5,
        }
    }
}

impl From<StateTreeVersionV3> for StateTreeVersion {
    fn from(value: StateTreeVersionV3) -> Self {
        match value {
            StateTreeVersionV3::V0 => Self::V0,
            StateTreeVersionV3::V1 => Self::V1,
            StateTreeVersionV3::V2 => Self::V2,
            StateTreeVersionV3::V3 => Self::V3,
            StateTreeVersionV3::V4 => Self::V4,
            StateTreeVersionV3::V5 => Self::V5,
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
            StateTreeVersion::V0 => Self::V0,
            StateTreeVersion::V1 => Self::V1,
            StateTreeVersion::V2 => Self::V2,
            StateTreeVersion::V3 => Self::V3,
            StateTreeVersion::V4 => Self::V4,
            StateTreeVersion::V5 => bail!("Impossible conversion"),
        })
    }
}

impl TryFrom<StateTreeVersion> for StateTreeVersionV3 {
    type Error = anyhow::Error;

    fn try_from(value: StateTreeVersion) -> anyhow::Result<Self> {
        Ok(match value {
            StateTreeVersion::V0 => Self::V0,
            StateTreeVersion::V1 => Self::V1,
            StateTreeVersion::V2 => Self::V2,
            StateTreeVersion::V3 => Self::V3,
            StateTreeVersion::V4 => Self::V4,
            StateTreeVersion::V5 => Self::V5,
        })
    }
}

impl TryFrom<StateTreeVersion> for StateTreeVersionV4 {
    type Error = anyhow::Error;

    fn try_from(value: StateTreeVersion) -> anyhow::Result<Self> {
        Ok(match value {
            StateTreeVersion::V0 => Self::V0,
            StateTreeVersion::V1 => Self::V1,
            StateTreeVersion::V2 => Self::V2,
            StateTreeVersion::V3 => Self::V3,
            StateTreeVersion::V4 => Self::V4,
            StateTreeVersion::V5 => Self::V5,
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
#[delegated_enum(impl_conversions)]
pub enum StateTree<S> {
    // Version 0 is used to parse the genesis block.
    V0(super::state_tree_v0::StateTreeV0<Arc<S>>),
    // fvm-2 support state tree versions 3 and 4.
    FvmV2(StateTreeV2<Arc<S>>),
    // fvm-3 support state tree versions 5.
    FvmV3(StateTreeV3<Arc<S>>),
    // fvm-4 support state tree versions *.
    FvmV4(StateTreeV4<Arc<S>>),
}

impl<S> StateTree<S>
where
    S: Blockstore,
{
    /// Constructor for a HAMT state tree given an IPLD store
    pub fn new(store: Arc<S>, version: StateTreeVersion) -> anyhow::Result<Self> {
        if let Ok(st) = StateTreeV4::new(store.clone(), version.try_into()?) {
            Ok(StateTree::FvmV4(st))
        } else if let Ok(st) = StateTreeV3::new(store.clone(), version.try_into()?) {
            Ok(StateTree::FvmV3(st))
        } else if let Ok(st) = StateTreeV2::new(store, version.try_into()?) {
            Ok(StateTree::FvmV2(st))
        } else {
            bail!("Can't create a valid state tree for the given version.");
        }
    }

    pub fn new_from_root(store: Arc<S>, c: &Cid) -> anyhow::Result<Self> {
        if let Ok(st) = StateTreeV4::new_from_root(store.clone(), c) {
            Ok(StateTree::FvmV4(st))
        } else if let Ok(st) = StateTreeV3::new_from_root(store.clone(), c) {
            Ok(StateTree::FvmV3(st))
        } else if let Ok(st) = StateTreeV2::new_from_root(store.clone(), c) {
            Ok(StateTree::FvmV2(st))
        } else if let Ok(st) = super::state_tree_v0::StateTreeV0::new_from_root(store.clone(), c) {
            Ok(StateTree::V0(st))
        } else if !store.has(c)? {
            bail!("No state tree exists for the root {c}.")
        } else {
            let state_root = store.get_cbor::<StateRoot>(c).ok().flatten();
            let state_root_version = state_root
                .map(|sr| format!("{:?}", sr.version))
                .unwrap_or_else(|| "unknown".into());
            bail!(
                "Can't create a valid state tree from the given root. This error may indicate unsupported version. state_root_cid={c}, state_root_version={state_root_version}"
            )
        }
    }

    pub fn new_from_tipset(store: Arc<S>, ts: &Tipset) -> anyhow::Result<Self> {
        Self::new_from_root(store, ts.parent_state())
    }

    /// Get required actor state from an address. Will be resolved to ID address.
    pub fn get_required_actor(&self, addr: &Address) -> anyhow::Result<ActorState> {
        self.get_actor(addr)?
            .with_context(|| format!("Actor not found: addr {addr}"))
    }

    /// Get the actor bundle metadata
    pub fn get_actor_bundle_metadata(&self) -> anyhow::Result<&ActorBundleMetadata> {
        let system_actor_code = self.get_required_actor(&Address::SYSTEM_ACTOR)?.code;
        ACTOR_BUNDLES_METADATA
            .values()
            .find(|v| v.manifest.get_system() == system_actor_code)
            .with_context(|| format!("actor bundle not found for system actor {system_actor_code}"))
    }

    /// Get actor state from an address. Will be resolved to ID address.
    pub fn get_actor(&self, addr: &Address) -> anyhow::Result<Option<ActorState>> {
        match self {
            StateTree::FvmV2(st) => {
                anyhow::ensure!(
                    addr.protocol() != crate::shim::address::Protocol::Delegated,
                    "Delegated addresses are not supported in FVMv2 state trees"
                );
                Ok(st
                    .get_actor(&addr.into())
                    .map_err(|e| anyhow!("{e}"))?
                    .map(Into::into))
            }
            StateTree::FvmV3(st) => {
                let id = st.lookup_id(&addr.into())?;
                if let Some(id) = id {
                    Ok(st
                        .get_actor(id)
                        .map_err(|e| anyhow!("{e}"))?
                        .map(Into::into))
                } else {
                    Ok(None)
                }
            }
            StateTree::FvmV4(st) => {
                let id = st.lookup_id(addr)?;
                if let Some(id) = id {
                    Ok(st
                        .get_actor(id)
                        .map_err(|e| anyhow!("{e}"))?
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
                        .map_err(|e| anyhow!("{e}"))?
                        .map(Into::into))
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Gets actor state from implicit actor address
    pub fn get_actor_state<STATE: LoadActorStateFromBlockstore>(&self) -> anyhow::Result<STATE> {
        let address = STATE::ACTOR.with_context(|| {
            format!(
                "No associated actor address for {}, use `get_actor_state_from_address` instead.",
                std::any::type_name::<STATE>()
            )
        })?;
        let actor = self.get_required_actor(&address)?;
        STATE::load_from_blockstore(self.store(), &actor)
    }

    /// Gets actor state from explicit actor address
    pub fn get_actor_state_from_address<STATE: LoadActorStateFromBlockstore>(
        &self,
        actor_address: &Address,
    ) -> anyhow::Result<STATE> {
        let actor = self.get_required_actor(actor_address)?;
        STATE::load_from_blockstore(self.store(), &actor)
    }

    /// Retrieve store reference to modify db.
    pub fn store(&self) -> &S {
        delegate_state_tree!(self.store())
    }

    /// Get an ID address from any Address
    pub fn lookup_id(&self, addr: &Address) -> anyhow::Result<Option<ActorID>> {
        match self {
            StateTree::FvmV2(st) => st.lookup_id(&addr.into()).map_err(|e| anyhow!("{e}")),
            StateTree::FvmV3(st) => Ok(st.lookup_id(&addr.into())?),
            StateTree::FvmV4(st) => Ok(st.lookup_id(&addr.into())?),
            StateTree::V0(_) => bail!("StateTree::lookup_id not supported on old state trees"),
        }
    }

    /// Get an required ID address from any Address
    pub fn lookup_required_id(&self, addr: &Address) -> anyhow::Result<ActorID> {
        self.lookup_id(addr)?
            .with_context(|| format!("actor id not found for address {addr}"))
    }

    pub fn for_each<F>(&self, mut f: F) -> anyhow::Result<()>
    where
        F: FnMut(Address, &ActorState) -> anyhow::Result<()>,
    {
        match self {
            StateTree::FvmV2(st) => {
                st.for_each(|address, actor_state| f(address.into(), &actor_state.into()))
            }
            StateTree::FvmV3(st) => {
                st.for_each(|address, actor_state| f(address.into(), &actor_state.into()))
            }
            StateTree::FvmV4(st) => {
                st.for_each(|address, actor_state| f(address.into(), &actor_state.into()))
            }
            StateTree::V0(_) => bail!("StateTree::for_each not supported on old state trees"),
        }
    }

    /// Flush state tree and return Cid root.
    pub fn flush(&mut self) -> anyhow::Result<Cid> {
        match self {
            StateTree::FvmV2(st) => st.flush().map_err(|e| anyhow!("{e}")),
            StateTree::FvmV3(st) => Ok(st.flush()?),
            StateTree::FvmV4(st) => Ok(st.flush()?),
            StateTree::V0(_) => bail!("StateTree::flush not supported on old state trees"),
        }
    }

    /// Set actor state with an actor ID.
    pub fn set_actor(&mut self, addr: &Address, actor: ActorState) -> anyhow::Result<()> {
        match self {
            StateTree::FvmV2(st) => st
                .set_actor(&addr.into(), actor.into())
                .map_err(|e| anyhow!("{e}")),
            StateTree::FvmV3(st) => {
                let id = st
                    .lookup_id(&addr.into())?
                    .context("couldn't find actor id")?;
                st.set_actor(id, actor.into());
                Ok(())
            }
            StateTree::FvmV4(st) => {
                let id = st
                    .lookup_id(&addr.into())?
                    .context("couldn't find actor id")?;
                st.set_actor(id, actor.into());
                Ok(())
            }
            StateTree::V0(_) => bail!("StateTree::set_actor not supported on old state trees"),
        }
    }

    /// Returns the public key type of
    /// address(`BLS`/`SECP256K1`) of an actor identified by `addr`,
    /// or its delegated address.
    pub fn resolve_to_deterministic_addr(
        &self,
        store: &impl Blockstore,
        addr: Address,
    ) -> anyhow::Result<Address> {
        use crate::shim::address::Protocol::*;
        match addr.protocol() {
            BLS | Secp256k1 | Delegated => Ok(addr),
            _ => {
                let actor = self
                    .get_actor(&addr)?
                    .with_context(|| format!("failed to find actor: {addr}"))?;

                // A workaround to implement `if state.Version() >= types.StateTreeVersion5`
                // When state tree version is not available in rust APIs
                if !matches!(self, Self::FvmV2(_) | Self::V0(_))
                    && let Some(address) = actor.delegated_address
                {
                    return Ok(address.into());
                }

                let account_state = account::State::load(store, actor.code, actor.state)?;
                Ok(account_state.pubkey_address())
            }
        }
    }
}

/// `Newtype` to wrap different versions of `fvm::state_tree::ActorState`
///
/// # Examples
/// ```
/// # use forest::doctest_private::ActorState;
/// use cid::Cid;
///
/// // Create FVM2 ActorState normally
/// let fvm2_actor_state = fvm2::state_tree::ActorState::new(Cid::default(), Cid::default(),
/// fvm_shared2::econ::TokenAmount::from_atto(42), 0);
///
/// // Create a correspndoning FVM3 ActorState
/// let fvm3_actor_state = fvm3::state_tree::ActorState::new(Cid::default(), Cid::default(),
/// fvm_shared3::econ::TokenAmount::from_atto(42), 0, None);
///
/// // Create a correspndoning FVM4 ActorState
/// let fvm4_actor_state = fvm4::state_tree::ActorState::new(Cid::default(), Cid::default(),
/// fvm_shared4::econ::TokenAmount::from_atto(42), 0, None);
///
/// // Create a shim out of fvm2 state, ensure conversions are correct
/// let state_shim = ActorState::from(fvm2_actor_state.clone());
/// assert_eq!(fvm4_actor_state, *state_shim);
/// assert_eq!(fvm3_actor_state, state_shim.clone().into());
/// assert_eq!(fvm2_actor_state, state_shim.into());
/// ```
#[derive(
    PartialEq, Eq, Clone, Debug, Serialize, Deserialize, derive_more::Deref, derive_more::DerefMut,
)]
#[serde(transparent)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub struct ActorState(ActorState_latest);

impl ActorState {
    pub fn new(
        code: Cid,
        state: Cid,
        balance: TokenAmount,
        sequence: u64,
        address: Option<Address>,
    ) -> Self {
        Self(ActorState_latest::new(
            code,
            state,
            balance.into(),
            sequence,
            address.map(Into::into),
        ))
    }
    /// Construct a new empty actor with the specified code.
    pub fn new_empty(code: Cid, delegated_address: Option<Address>) -> Self {
        Self(ActorState_latest::new_empty(
            code,
            delegated_address.map(Into::into),
        ))
    }
}

impl From<&ActorStateV2> for ActorState {
    fn from(value: &ActorStateV2) -> Self {
        Self(ActorState_latest {
            code: value.code,
            state: value.state,
            sequence: value.sequence,
            balance: TokenAmount::from(&value.balance).into(),
            delegated_address: None,
        })
    }
}

impl From<ActorStateV2> for ActorState {
    fn from(value: ActorStateV2) -> Self {
        (&value).into()
    }
}

impl From<ActorStateV3> for ActorState {
    fn from(value: ActorStateV3) -> Self {
        Self(ActorState_latest {
            code: value.code,
            state: value.state,
            sequence: value.sequence,
            balance: TokenAmount::from(value.balance).into(),
            delegated_address: value
                .delegated_address
                .map(|addr| Address::from(addr).into()),
        })
    }
}

impl From<&ActorStateV3> for ActorState {
    fn from(value: &ActorStateV3) -> Self {
        value.clone().into()
    }
}

impl From<ActorStateV4> for ActorState {
    fn from(value: ActorStateV4) -> Self {
        ActorState(value)
    }
}

impl From<&ActorStateV4> for ActorState {
    fn from(value: &ActorStateV4) -> Self {
        value.clone().into()
    }
}

impl From<ActorState> for ActorStateV2 {
    fn from(other: ActorState) -> ActorStateV2 {
        Self {
            code: other.code,
            state: other.state,
            sequence: other.sequence,
            balance: TokenAmount::from(&other.balance).into(),
        }
    }
}

impl From<&ActorState> for ActorStateV2 {
    fn from(other: &ActorState) -> ActorStateV2 {
        Self {
            code: other.code,
            state: other.state,
            sequence: other.sequence,
            balance: TokenAmount::from(&other.balance).into(),
        }
    }
}

impl From<ActorState> for ActorStateV3 {
    fn from(other: ActorState) -> Self {
        Self {
            code: other.code,
            state: other.state,
            sequence: other.sequence,
            balance: TokenAmount::from(&other.balance).into(),
            delegated_address: other
                .delegated_address
                .map(|addr| Address::from(addr).into()),
        }
    }
}

impl From<ActorState> for ActorStateV4 {
    fn from(other: ActorState) -> Self {
        other.0
    }
}

#[cfg(test)]
mod tests {
    use super::StateTree;
    use crate::blocks::CachingBlockHeader;
    use crate::db::car::AnyCar;
    use crate::networks::{calibnet, mainnet};
    use crate::shim::actors::init;
    use cid::Cid;
    use std::sync::Arc;

    // refactored from `StateManager::get_network_name`
    fn get_network_name(car: &'static [u8], genesis_cid: Cid) -> String {
        let forest_car = AnyCar::new(car).unwrap();
        let genesis_block = CachingBlockHeader::load(&forest_car, genesis_cid)
            .unwrap()
            .unwrap();
        let state_tree =
            StateTree::new_from_root(Arc::new(&forest_car), &genesis_block.state_root).unwrap();
        let state: init::State = state_tree.get_actor_state().unwrap();
        state.into_network_name()
    }

    #[test]
    fn calibnet_network_name() {
        assert_eq!(
            get_network_name(calibnet::DEFAULT_GENESIS, *calibnet::GENESIS_CID),
            "calibrationnet"
        );
    }

    #[test]
    fn mainnet_network_name() {
        // Yes, the name of `mainnet` in the genesis block really is `testnetnet`.
        assert_eq!(
            get_network_name(mainnet::DEFAULT_GENESIS, *mainnet::GENESIS_CID),
            "testnetnet"
        );
    }
}
