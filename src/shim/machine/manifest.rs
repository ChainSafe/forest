// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
// Copyright 2021-2023 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(deprecated, dead_code)]

use std::collections::BTreeMap;

use ahash::HashMap;
use anyhow::{anyhow, ensure, Context as _};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore as _;
use itertools::Itertools as _;

/// This should be the latest enumeration of all builtin actors
pub use fil_actors_shared::v11::runtime::builtins::Type as BuiltinActor;

/// A list of [`BuiltinActor`]s to their CIDs
// Theoretically, this struct could just have fields for all the actors,
// acting as a kind of perfect hash map, but performance will be fine as-is
pub struct BuiltinActorManifest {
    builtin2cid: BTreeMap<BuiltinActor, Cid>,
    /// The CID that this manifest was built from
    actor_list_cid: Cid,
}

// Manifest2.builtin2cid must be a BTreeMap
static_assertions::assert_not_impl_all!(BuiltinActor: std::hash::Hash);

impl BuiltinActorManifest {
    const MANDATORY_BUILTINS: &[BuiltinActor] = &[BuiltinActor::Init, BuiltinActor::System];
    pub fn load_manifest(b: impl Blockstore, manifest_cid: &Cid) -> anyhow::Result<Self> {
        let (manifest_version, actor_list_cid) = b
            .get_cbor::<(u32, Cid)>(manifest_cid)?
            .context("failed to load manifest")?;
        ensure!(
            manifest_version == 1,
            "unsupported manifest version {}",
            manifest_version
        );
        Self::load_v1_actor_list(b, &actor_list_cid)
    }
    pub fn load_v1_actor_list(b: impl Blockstore, actor_list_cid: &Cid) -> anyhow::Result<Self> {
        let mut actor_list = b
            .get_cbor::<Vec<(String, Cid)>>(actor_list_cid)?
            .context("failed to load actor list")?;
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
        assert!(Self::MANDATORY_BUILTINS.contains(&BuiltinActor::System));
        self.get(BuiltinActor::System).unwrap()
    }
    pub fn get_init(&self) -> Cid {
        assert!(Self::MANDATORY_BUILTINS.contains(&BuiltinActor::Init));
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
}

// For details on actor name and version, see <https://github.com/filecoin-project/go-state-types/blob/1e6cf0d47cdda75383ef036fc2725d1cf51dbde8/manifest/manifest.go#L36>

macro_rules! name {
    ($($ident:ident = $lit:literal),* $(,)?) => {
        $(
            #[allow(unused)] // included for completeness
            pub const $ident: &str = $lit;
        )*
    }
}
name!(
    ACCOUNT_ACTOR_NAME = "account",
    CRON_ACTOR_NAME = "cron",
    INIT_ACTOR_NAME = "init",
    MARKET_ACTOR_NAME = "storagemarket",
    MINER_ACTOR_NAME = "storageminer",
    MULTISIG_ACTOR_NAME = "multisig",
    PAYCH_ACTOR_NAME = "paymentchannel",
    POWER_ACTOR_NAME = "storagepower",
    REWARD_ACTOR_NAME = "reward",
    SYSTEM_ACTOR_NAME = "system",
    VERIFREG_ACTOR_NAME = "verifiedregistry",
    // actor version >= 9
    DATACAP_ACTOR_NAME = "datacap",
    // actor version >= 10
    EVM_ACTOR_NAME = "evm",
    EAM_ACTOR_NAME = "eam",
    PLACEHOLDER_ACTOR_NAME = "placeholder",
    ETH_ACCOUNT_ACTOR_NAME = "ethaccount",
);

/// Manifest is serialized as a tuple of version and manifest actors CID
type ManifestCbor = (u32, Cid);

/// Manifest data is serialized as a vector of name-to-actor-CID pair
type ManifestActorsCbor = Vec<(String, Cid)>;

/// A mapping of builtin actor CIDs to their respective types.
#[deprecated]
pub struct Manifest {
    by_name: HashMap<String, Cid>,

    actors_cid: Cid,

    init_code: Cid,
    system_code: Cid,
}

impl Manifest {
    /// Load a manifest from the block store with manifest CID.
    pub fn load<B: Blockstore>(bs: &B, manifest_cid: &Cid) -> anyhow::Result<Self> {
        let (version, actors_cid): ManifestCbor = bs.get_cbor(manifest_cid)?.ok_or_else(|| {
            anyhow::anyhow!("Failed to retrieve manifest with manifest cid {manifest_cid}")
        })?;

        Self::load_with_actors(bs, &actors_cid, version)
    }

    /// Load a manifest from the block store with actors CID and version.
    /// Note that only version 1 is supported.
    pub fn load_with_actors<B: Blockstore>(
        bs: &B,
        actors_cid: &Cid,
        version: u32,
    ) -> anyhow::Result<Self> {
        if version != 1 {
            anyhow::bail!("unsupported manifest version {version}");
        }

        let actors: ManifestActorsCbor = bs.get_cbor(actors_cid)?.ok_or_else(|| {
            anyhow::anyhow!("Failed to retrieve manifest actors with actors cid {actors_cid}")
        })?;

        Self::new(actors, *actors_cid)
    }

    /// Construct a new manifest from actor name/CID tuples.
    fn new(iter: impl IntoIterator<Item = (String, Cid)>, actors_cid: Cid) -> anyhow::Result<Self> {
        let by_name = HashMap::from_iter(iter);

        let init_code = *by_name
            .get(INIT_ACTOR_NAME)
            .context("manifest missing init actor")?;

        let system_code = *by_name
            .get(SYSTEM_ACTOR_NAME)
            .context("manifest missing system actor")?;

        Ok(Self {
            by_name,
            actors_cid,
            init_code,
            system_code,
        })
    }

    pub fn actors_cid(&self) -> Cid {
        self.actors_cid
    }

    /// Returns the code CID for a builtin actor, given the actor's name.
    pub fn code_by_name(&self, name: &str) -> anyhow::Result<&Cid> {
        self.by_name
            .get(name)
            .ok_or_else(|| anyhow!("Failed to retrieve actor code by name {name}"))
    }

    pub fn actors_count(&self) -> usize {
        self.by_name.len()
    }

    pub fn builtin_actors(&self) -> impl Iterator<Item = (&String, &Cid)> {
        self.by_name.iter()
    }

    /// Returns the code CID for the init actor.
    pub fn init_code(&self) -> &Cid {
        &self.init_code
    }

    /// Returns the code CID for the system actor.
    pub fn system_code(&self) -> &Cid {
        &self.system_code
    }
}

// https://github.com/ChainSafe/fil-actor-states/issues/171
macro_rules! exhaustive {
    ($vis:vis const $ident:ident: &[$ty:ty] = &[$($variant:path),* $(,)?];) => {
        $vis const $ident: &[$ty] = &[$($variant,)*];
        const _: () = {
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
