// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
// Copyright 2021-2023 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::BTreeMap;

use anyhow::{ensure, Context as _};
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

macro_rules! static_assert_contains_matching {
    ($slice:expr, $must_match:pat) => {
        const _: () = {
            let slice = $slice;
            let mut cur_ix = slice.len();
            'ok: {
                while let Some(new_ix) = cur_ix.checked_sub(1) {
                    cur_ix = new_ix;
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
