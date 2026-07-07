// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::prelude::*;
use crate::shim::address::Payload;
use bls_signatures::{PublicKey as BlsPublicKey, Serialize as _};

impl StateManager {
    /// Returns a BLS public key from provided address
    pub fn get_bls_public_key(
        db: &(impl Blockstore + ShallowClone),
        addr: Address,
        state_cid: Cid,
    ) -> Result<BlsPublicKey, Error> {
        let state =
            StateTree::new_from_root(db, &state_cid).map_err(|e| Error::Other(e.to_string()))?;
        let kaddr = state
            .resolve_to_deterministic_address(db, addr)
            .context("Failed to resolve key address")?;

        match kaddr.into_payload() {
            Payload::BLS(key) => BlsPublicKey::from_bytes(&key)
                .context("Failed to construct bls public key")
                .map_err(Error::from),
            _ => Err(Error::state(
                "Address must be BLS address to load bls public key",
            )),
        }
    }

    /// Looks up ID [Address] from the state at the given [Tipset].
    pub fn lookup_id(&self, addr: &Address, ts: &Tipset) -> Result<Option<Address>, Error> {
        let state_tree =
            StateTree::new_from_root(self.db(), ts.parent_state()).map_err(|e| format!("{e:?}"))?;
        Ok(state_tree
            .lookup_id(addr)
            .map_err(|e| Error::Other(e.to_string()))?
            .map(Address::new_id))
    }

    /// Looks up required ID [Address] from the state at the given [Tipset].
    pub fn lookup_required_id(&self, addr: &Address, ts: &Tipset) -> Result<Address, Error> {
        self.lookup_id(addr, ts)?
            .ok_or_else(|| Error::Other(format!("Failed to lookup the id address {addr}")))
    }

    /// Similar to [`StateTree::resolve_to_deterministic_addr`] but does not allow [`crate::shim::address::Protocol::Actor`] type of addresses.
    /// Uses the [`Tipset`] `ts` to generate the VM state.
    pub async fn resolve_to_deterministic_address(
        &self,
        address: Address,
        ts: &Tipset,
    ) -> anyhow::Result<Address> {
        use crate::shim::address::Protocol::*;
        match address.protocol() {
            BLS | Secp256k1 | Delegated => Ok(address),
            Actor => anyhow::bail!("cannot resolve actor address to key address"),
            ID => {
                let id = address.id()?;
                let resolve = async {
                    // First try to resolve the actor in the parent state, so we don't have to compute anything.
                    let resolved = if let Ok(state) =
                        StateTree::new_from_root(self.db(), ts.parent_state())
                        && let Ok(address) =
                            state.resolve_to_deterministic_address(self.db(), address)
                    {
                        address
                    } else {
                        // If that fails, compute the tip-set and try again.
                        let TipsetState { state_root, .. } = self.load_tipset_state(ts).await?;
                        let state = StateTree::new_from_root(self.db(), &state_root)?;
                        state.resolve_to_deterministic_address(self.db(), address)?
                    };
                    anyhow::Ok(resolved)
                };
                // Memoize unless the cache is disabled (test-snapshot generation
                // and replay, see the field docs on `StateManager`).
                match &self.id_to_deterministic_address_cache {
                    Some(cache) => cache.get_or_insert_async(&id, resolve).await,
                    None => resolve.await,
                }
            }
        }
    }
}
