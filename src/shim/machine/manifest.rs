// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
// Copyright 2021-2023 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::BTreeMap;

use crate::{
    networks::{ACTOR_BUNDLES_METADATA, ActorBundleMetadata},
    utils::db::CborStoreExt as _,
};
use anyhow::{Context as _, ensure};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools as _;
use num::FromPrimitive;
use serde::{Deserialize, Serialize};

/// This should be the latest enumeration of all builtin actors
pub use fil_actors_shared::v11::runtime::builtins::Type as BuiltinActor;

/// A list of [`BuiltinActor`]s to their CIDs
// Theoretically, this struct could just have fields for all the actors,
// acting as a kind of perfect hash map, but performance will be fine as-is
// #[derive(Serialize, Deserialize, Debug)]
#[derive(Debug, PartialEq, Default)]
pub struct BuiltinActorManifest {
    builtin2cid: BTreeMap<BuiltinActor, Cid>,
    /// The CID that this manifest was built from
    pub actor_list_cid: Cid,
}

// We need to implement Serialize and Deserialize manually because `BuiltinActor` is not `Serialize` or `Deserialize`,
// and it's an external dependency. Additionally, we want the name of the actor to be visible in the serialized form,
// even though it's not reliable for deserialization.
impl Serialize for BuiltinActorManifest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;

        let builtin2cid = self
            .builtin2cid
            .iter()
            .map(|(&k, v)| (k.name(), k as i32, v.to_string()))
            .collect_vec();
        let actor_list_cid = self.actor_list_cid.to_string();
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("actors", &builtin2cid)?;
        map.serialize_entry("actor_list_cid", &actor_list_cid)?;
        map.end()
    }
}

struct BuiltinActorManifestVisitor;

impl<'de> serde::de::Visitor<'de> for BuiltinActorManifestVisitor {
    type Value = BuiltinActorManifest;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a map with 'actor_list_cid' and 'actors' keys")
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut actor_list_cid: Option<Cid> = None;
        let mut builtin2cid: Option<Vec<(String, i32, String)>> = None;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "actor_list_cid" => {
                    if actor_list_cid.is_some() {
                        return Err(serde::de::Error::custom("duplicate actor_list_cid key"));
                    }
                    actor_list_cid = Some(
                        map.next_value::<String>()?
                            .try_into()
                            .map_err(serde::de::Error::custom)?,
                    );
                }
                "actors" => {
                    if builtin2cid.is_some() {
                        return Err(serde::de::Error::custom("duplicate actors key"));
                    }
                    builtin2cid = Some(map.next_value()?);
                }
                _ => return Err(serde::de::Error::custom(format!("unexpected key: {key}"))),
            }
        }
        let actor_list_cid =
            actor_list_cid.ok_or_else(|| serde::de::Error::custom("missing actor_list_cid key"))?;
        let builtin2cid: BTreeMap<BuiltinActor, Cid> = builtin2cid
            .ok_or_else(|| serde::de::Error::custom("missing actors key"))?
            .into_iter()
            .map(|(_, k, v)| {
                Ok((
                    BuiltinActor::from_i32(k)
                        .ok_or_else(|| serde::de::Error::custom("invalid builtin actor"))?,
                    v.try_into().map_err(serde::de::Error::custom)?,
                ))
            })
            .collect::<Result<_, _>>()?;

        // Assert that all mandatory actors are present
        if !BuiltinActorManifest::MANDATORY_BUILTINS
            .iter()
            .all(|builtin| builtin2cid.iter().any(|(id, _)| id == builtin))
        {
            return Err(serde::de::Error::custom("missing mandatory actor"));
        }

        Ok(BuiltinActorManifest {
            builtin2cid,
            actor_list_cid,
        })
    }
}

impl<'de> Deserialize<'de> for BuiltinActorManifest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(BuiltinActorManifestVisitor)
    }
}

// Manifest2.builtin2cid must be a BTreeMap
static_assertions::assert_not_impl_all!(BuiltinActor: std::hash::Hash);

impl BuiltinActorManifest {
    const MANDATORY_BUILTINS: &'static [BuiltinActor] = &[BuiltinActor::Init, BuiltinActor::System];
    pub fn load_manifest(b: &impl Blockstore, manifest_cid: &Cid) -> anyhow::Result<Self> {
        let (manifest_version, actor_list_cid) = b.get_cbor_required::<(u32, Cid)>(manifest_cid)?;
        ensure!(
            manifest_version == 1,
            "unsupported manifest version {}",
            manifest_version
        );
        Self::load_v1_actor_list(b, &actor_list_cid)
    }
    pub fn load_v1_actor_list(b: &impl Blockstore, actor_list_cid: &Cid) -> anyhow::Result<Self> {
        let mut actor_list = b.get_cbor_required::<Vec<(String, Cid)>>(actor_list_cid)?;
        actor_list.sort();
        ensure!(
            actor_list.iter().map(|(name, _cid)| name).all_unique(),
            "duplicate actor name in actor list"
        );
        let mut name2cid = BTreeMap::from_iter(actor_list);
        let mut builtin2cid = BTreeMap::new();
        for builtin in ALL_BUILTINS {
            if let Some(cid) = name2cid.remove(builtin.name()) {
                builtin2cid.insert(*builtin, cid);
            }
        }
        for mandatory_builtin in Self::MANDATORY_BUILTINS {
            ensure!(
                builtin2cid.contains_key(mandatory_builtin),
                "actor list does not contain mandatory actor {}",
                mandatory_builtin.name()
            )
        }
        if !name2cid.is_empty() {
            tracing::warn!("unknown actors in list: [{}]", name2cid.keys().join(", "))
        }
        Ok(Self {
            builtin2cid,
            actor_list_cid: *actor_list_cid,
        })
    }
    // Return anyhow::Result instead of Option because we know our users are all at the root of an error chain
    pub fn get(&self, builtin: BuiltinActor) -> anyhow::Result<Cid> {
        self.builtin2cid
            .get(&builtin)
            .copied()
            .with_context(|| format!("builtin actor {} is not in the manifest", builtin.name()))
    }
    pub fn get_system(&self) -> Cid {
        static_assert_contains_matching!(
            BuiltinActorManifest::MANDATORY_BUILTINS,
            BuiltinActor::System
        );
        self.get(BuiltinActor::System).unwrap()
    }
    pub fn get_init(&self) -> Cid {
        static_assert_contains_matching!(
            BuiltinActorManifest::MANDATORY_BUILTINS,
            BuiltinActor::Init
        );
        self.get(BuiltinActor::Init).unwrap()
    }
    /// The CID that this manifest was built from, also known as the `actors CID`
    #[doc(alias = "actors_cid")]
    pub fn source_cid(&self) -> Cid {
        self.actor_list_cid
    }
    pub fn builtin_actors(&self) -> impl ExactSizeIterator<Item = (BuiltinActor, Cid)> + '_ {
        self.builtin2cid.iter().map(|(k, v)| (*k, *v)) // std::iter::Copied doesn't play well with the tuple here
    }
    /// Get the actor bundle metadata
    pub fn metadata(&self) -> anyhow::Result<&ActorBundleMetadata> {
        ACTOR_BUNDLES_METADATA
            .values()
            .find(|v| &v.manifest == self)
            .with_context(|| {
                format!(
                    "actor bundle not found for system actor {}",
                    self.get_system()
                )
            })
    }
}

macro_rules! exhaustive {
    ($vis:vis const $ident:ident: &[$ty:ty] = &[$($variant:path),* $(,)?];) => {
        $vis const $ident: &[$ty] = &[$($variant,)*];
        const _: () = {
            #[allow(dead_code)]
            fn check_exhaustive(it: $ty) {
                match it {
                    $(
                        $variant => {},
                    )*
                }
            }
        };

    }
}

exhaustive! {
    const ALL_BUILTINS: &[BuiltinActor] = &[
        BuiltinActor::System,
        BuiltinActor::Init,
        BuiltinActor::Cron,
        BuiltinActor::Account,
        BuiltinActor::Power,
        BuiltinActor::Miner,
        BuiltinActor::Market,
        BuiltinActor::PaymentChannel,
        BuiltinActor::Multisig,
        BuiltinActor::Reward,
        BuiltinActor::VerifiedRegistry,
        BuiltinActor::DataCap,
        BuiltinActor::Placeholder,
        BuiltinActor::EVM,
        BuiltinActor::EAM,
        BuiltinActor::EthAccount,
    ];
}

macro_rules! static_assert_contains_matching {
    ($slice:expr, $must_match:pat) => {
        const _: () = {
            let slice = $slice;
            let mut cur_ix = slice.len();
            'ok: {
                while let Some(new_ix) = cur_ix.checked_sub(1) {
                    cur_ix = new_ix;
                    #[allow(clippy::indexing_slicing)]
                    match slice[cur_ix] {
                        $must_match => break 'ok,
                        _ => continue,
                    }
                }
                panic!("slice did not contain a match")
            }
        };
    };
}
pub(crate) use static_assert_contains_matching;

#[cfg(test)]
mod test {
    use crate::utils::multihash::prelude::*;
    use fvm_ipld_encoding::DAG_CBOR;

    use super::*;

    fn create_random_cid() -> Cid {
        let data = rand::random::<[u8; 32]>();
        Cid::new_v1(DAG_CBOR, MultihashCode::Blake2b256.digest(&data))
    }

    fn create_manifest() -> BuiltinActorManifest {
        let builtin2cid = ALL_BUILTINS
            .iter()
            .map(|&builtin| (builtin, create_random_cid()))
            .collect();
        BuiltinActorManifest {
            builtin2cid,
            actor_list_cid: create_random_cid(),
        }
    }

    #[test]
    fn test_manifest_serde_roundtrip() {
        let manifest = create_manifest();

        let serialized = serde_json::to_string(&manifest).unwrap();
        let deserialized: BuiltinActorManifest = serde_json::from_str(&serialized).unwrap();

        assert_eq!(manifest, deserialized);
    }

    #[test]
    fn test_manifest_serde_roundtrip_missing_trailing() {
        let mut manifest = create_manifest();
        // No last actor is okay and is required for backwards compatibility (networks without,
        // e.g., the EAM actor)
        manifest.builtin2cid.pop_last();

        let serialized = serde_json::to_string(&manifest).unwrap();
        let deserialized: BuiltinActorManifest = serde_json::from_str(&serialized).unwrap();

        assert_eq!(manifest, deserialized);
    }

    #[test]
    fn test_manifest_serde_roundtrip_with_missing_mandatory_actor() {
        let mut manifest = create_manifest();
        manifest.builtin2cid.remove(&BuiltinActor::System);

        let serialized = serde_json::to_string(&manifest).unwrap();
        let deserialized: Result<BuiltinActorManifest, serde_json::Error> =
            serde_json::from_str(&serialized);
        assert!(deserialized.is_err());
    }

    #[test]
    fn test_manifest_metadata() {
        for metadata in ACTOR_BUNDLES_METADATA.values() {
            let manifest = &metadata.manifest;
            assert_eq!(metadata, manifest.metadata().unwrap());
        }
    }
}
