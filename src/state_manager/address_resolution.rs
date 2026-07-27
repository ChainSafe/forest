// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::chain::AtFinalityResolution;
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
                // The cache is disabled for the RPC test-snapshot generator
                // and replay harness (see the field docs on `StateManager`).
                let Some(cache) = &self.id_to_deterministic_address_cache else {
                    return self.resolve_id_address_at_tipset(address, ts).await;
                };
                if let Some(resolved) = cache.get(&id) {
                    return Ok(resolved);
                }
                // Only a resolution witnessed at a finality-deep tipset is
                // safe to memoize by bare ID: ID assignments within the
                // finality window can differ between competing forks, while
                // anything at or below the lookback is identical on every
                // possible future chain.
                let at_finality = {
                    let cs = self.chain_store().shallow_clone();
                    let ts = ts.clone();
                    tokio::task::spawn_blocking(move || {
                        cs.resolve_to_deterministic_address_at_finality(&address, &ts)
                    })
                    .await
                    .context("tokio join error")?
                };
                if let Ok(resolution) = at_finality {
                    return Ok(match resolution {
                        AtFinalityResolution::ReorgStable(resolved) => {
                            cache.insert(id, resolved);
                            resolved
                        }
                        AtFinalityResolution::Unstable(resolved) => resolved,
                    });
                }
                self.resolve_id_address_at_tipset(address, ts).await
            }
        }
    }

    /// Resolves an ID address against `ts` without touching the cache: first
    /// via the parent state, then by computing the tipset state if needed.
    async fn resolve_id_address_at_tipset(
        &self,
        address: Address,
        ts: &Tipset,
    ) -> anyhow::Result<Address> {
        // First try to resolve the actor in the parent state, so we don't have to compute anything.
        if let Ok(state) = self.get_state_tree(ts.parent_state())
            && let Ok(address) = state.resolve_to_deterministic_address(self.db(), address)
        {
            return Ok(address);
        }
        // If that fails, compute the tip-set and try again.
        let TipsetState { state_root, .. } = self.load_tipset_state(ts).await?;
        let state = self.get_state_tree(&state_root)?;
        state.resolve_to_deterministic_address(self.db(), address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::{CachingBlockHeader, RawBlockHeader, Tipset};
    use crate::chain::ChainStore;
    use crate::db::{DbImpl, MemoryDB};
    use crate::networks::ChainConfig;
    use crate::shim::state_tree::{ActorState, StateTree, StateTreeVersion};
    use crate::test_utils::dummy_ticket;
    use crate::utils::db::CborStoreExt as _;

    /// Present in the state from genesis onwards, so it is visible at the
    /// finality lookback of the head.
    const OLD_ACTOR: u64 = 300;
    /// Present only in the head's parent state — younger than finality.
    const YOUNG_ACTOR: u64 = 400;

    /// Builds a 3-tipset chain (genesis, ts1, head at epoch 2). Genesis and
    /// ts1 carry `root_a` (contains f0300 -> bls_a); head carries `root_b`,
    /// built on top of `root_a` (contains f0300 -> bls_a and f0400 -> bls_b).
    /// Returns (state_manager, head, bls_a, bls_b).
    fn setup_with_finality(chain_finality: ChainEpoch) -> (StateManager, Tipset, Address, Address) {
        let db: DbImpl = Arc::new(MemoryDB::default()).into();

        let mut cfg = ChainConfig::default();
        cfg.policy.chain_finality = chain_finality;
        let cfg = Arc::new(cfg);

        let bls_a = Address::new_bls(&[8u8; 48]).unwrap();
        let bls_b = Address::new_bls(&[9u8; 48]).unwrap();

        let mut st_a = StateTree::new(&db, StateTreeVersion::V5).unwrap();
        st_a.set_actor(
            &Address::new_id(OLD_ACTOR),
            ActorState::new_empty(Cid::default(), Some(bls_a)),
        )
        .unwrap();
        let root_a = st_a.flush().unwrap();

        // Builds on top of `root_a` (rather than a fresh tree) so f0300 stays
        // resolvable at head's own parent state too, matching how a real
        // state tree accumulates actors across epochs, and keeping every
        // resolution in these tests on the cheap parent-state path.
        let mut st_b = StateTree::new_from_root(&db, &root_a).unwrap();
        st_b.set_actor(
            &Address::new_id(YOUNG_ACTOR),
            ActorState::new_empty(Cid::default(), Some(bls_b)),
        )
        .unwrap();
        let root_b = st_b.flush().unwrap();

        let genesis = Tipset::from(CachingBlockHeader::new(RawBlockHeader {
            ticket: dummy_ticket(0),
            state_root: root_a,
            // `StateManager::new` builds a beacon schedule from the genesis
            // timestamp, which must be non-zero.
            timestamp: 1,
            ..Default::default()
        }));
        db.put_cbor_default(genesis.block_headers().first())
            .unwrap();

        let ts1 = Tipset::from(CachingBlockHeader::new(RawBlockHeader {
            parents: genesis.key().clone(),
            ticket: dummy_ticket(1),
            epoch: 1,
            state_root: root_a,
            timestamp: 1,
            ..Default::default()
        }));
        db.put_cbor_default(ts1.block_headers().first()).unwrap();

        let head = Tipset::from(CachingBlockHeader::new(RawBlockHeader {
            parents: ts1.key().clone(),
            ticket: dummy_ticket(2),
            epoch: 2,
            state_root: root_b,
            timestamp: 2,
            ..Default::default()
        }));
        db.put_cbor_default(head.block_headers().first()).unwrap();

        let cs = ChainStore::new(db, cfg, genesis.block_headers().first().clone()).unwrap();
        let sm = StateManager::new(cs).unwrap();
        (sm, head, bls_a, bls_b)
    }

    #[tokio::test]
    async fn caches_resolution_witnessed_at_finality_lookback() {
        let (sm, head, bls_a, bls_b) = setup_with_finality(1);
        let resolved = sm
            .resolve_to_deterministic_address(Address::new_id(OLD_ACTOR), &head)
            .await
            .unwrap();
        assert_eq!(resolved, bls_a);
        let cache = sm.id_to_deterministic_address_cache().unwrap();
        assert_eq!(cache.get(&OLD_ACTOR), Some(bls_a));

        // Prove subsequent calls are served from the cache: poison the entry
        // and observe the poisoned value coming back.
        cache.insert(OLD_ACTOR, bls_b);
        let resolved = sm
            .resolve_to_deterministic_address(Address::new_id(OLD_ACTOR), &head)
            .await
            .unwrap();
        assert_eq!(resolved, bls_b);
    }

    #[tokio::test]
    async fn does_not_cache_actor_younger_than_finality() {
        let (sm, head, _bls_a, bls_b) = setup_with_finality(1);
        // f0400 is absent at the lookback: resolvable at the head, uncached.
        let resolved = sm
            .resolve_to_deterministic_address(Address::new_id(YOUNG_ACTOR), &head)
            .await
            .unwrap();
        assert_eq!(resolved, bls_b);
        let cache = sm.id_to_deterministic_address_cache().unwrap();
        assert_eq!(cache.get(&YOUNG_ACTOR), None);
    }

    #[tokio::test]
    async fn does_not_cache_on_chain_younger_than_finality() {
        // Finality deeper than the whole chain: the lookback would degrade to
        // resolving at `ts` itself, which is not reorg-stable — never cache.
        let (sm, head, _bls_a, bls_b) = setup_with_finality(900);
        let resolved = sm
            .resolve_to_deterministic_address(Address::new_id(YOUNG_ACTOR), &head)
            .await
            .unwrap();
        assert_eq!(resolved, bls_b);
        assert_eq!(
            sm.id_to_deterministic_address_cache().unwrap().len(),
            0,
            "nothing may be cached without a finality-deep witness"
        );
    }

    #[tokio::test]
    async fn does_not_cache_at_exact_finality_boundary() {
        // Head epoch == chain_finality: the guard is strictly `>`, so this is
        // not finality-deep and the lookback degrades to resolving at the
        // head's own parent state (which does contain f0300, since `root_b`
        // was built on top of `root_a`). The resolution succeeds but, being
        // `Unstable`, must not be cached.
        let (sm, head, bls_a, _bls_b) = setup_with_finality(2);
        let resolved = sm
            .resolve_to_deterministic_address(Address::new_id(OLD_ACTOR), &head)
            .await
            .unwrap();
        assert_eq!(resolved, bls_a);
        assert_eq!(
            sm.id_to_deterministic_address_cache().unwrap().len(),
            0,
            "epoch == chain_finality is not finality-deep and must not be cached"
        );
    }

    #[tokio::test]
    async fn non_id_addresses_bypass_cache_and_lookback() {
        let (sm, head, bls_a, _bls_b) = setup_with_finality(1);
        let resolved = sm
            .resolve_to_deterministic_address(bls_a, &head)
            .await
            .unwrap();
        assert_eq!(resolved, bls_a);
        assert_eq!(sm.id_to_deterministic_address_cache().unwrap().len(), 0);
    }
}
