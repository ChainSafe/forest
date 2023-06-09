// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
// Copyright 2021-2023 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use ahash::{HashMap, HashMapExt};
use anyhow::{anyhow, Context};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding3::CborStore;

const ACCOUNT_ACTOR_NAME: &str = "account";
const INIT_ACTOR_NAME: &str = "init";
const SYSTEM_ACTOR_NAME: &str = "system";
const PLACEHOLDER_ACTOR_NAME: &str = "placeholder";
const EAM_ACTOR_NAME: &str = "eam";
const ETHACCOUNT_ACTOR_NAME: &str = "ethaccount";
const MINER_ACTOR_NAME: &str = "storageminer";
const POWER_ACTOR_NAME: &str = "storagepower";

/// A mapping of builtin actor CIDs to their respective types.
pub struct ManifestV3 {
    account_code: Cid,
    placeholder_code: Cid,
    system_code: Cid,
    init_code: Cid,
    eam_code: Cid,
    ethaccount_code: Cid,
    miner_code: Cid,
    power_code: Cid,

    by_id: HashMap<u32, Cid>,
    by_code: HashMap<Cid, u32>,
}

impl ManifestV3 {
    /// Load a manifest from the block store.
    pub fn load<B: Blockstore>(bs: &B, root_cid: &Cid, ver: u32) -> anyhow::Result<Self> {
        if ver != 1 {
            return Err(anyhow!("unsupported manifest version {}", ver));
        }

        let vec: Vec<(String, Cid)> = match bs.get_cbor(root_cid)? {
            Some(vec) => vec,
            None => {
                return Err(anyhow!("cannot find manifest root cid {}", root_cid));
            }
        };

        Self::new(vec)
    }

    /// Construct a new manifest from actor name/CID tuples.
    pub fn new(iter: impl IntoIterator<Item = (impl Into<String>, Cid)>) -> anyhow::Result<Self> {
        let mut by_name = HashMap::new();
        let mut by_id = HashMap::new();
        let mut by_code = HashMap::new();

        // Actors are indexed sequentially, starting at 1, in the order in which they
        // appear in the manifest. 0 is reserved for "everything else" (i.e.,
        // not a builtin actor).
        for ((name, code_cid), id) in iter.into_iter().zip(1u32..) {
            let name = name.into();
            by_id.insert(id, code_cid);
            by_code.insert(code_cid, id);
            by_name.insert(name, code_cid);
        }

        let account_code = *by_name
            .get(ACCOUNT_ACTOR_NAME)
            .context("manifest missing account actor")?;

        let system_code = *by_name
            .get(SYSTEM_ACTOR_NAME)
            .context("manifest missing system actor")?;

        let init_code = *by_name
            .get(INIT_ACTOR_NAME)
            .context("manifest missing init actor")?;

        let placeholder_code = *by_name
            .get(PLACEHOLDER_ACTOR_NAME)
            .context("manifest missing placeholder actor")?;

        let eam_code = *by_name
            .get(EAM_ACTOR_NAME)
            .context("manifest missing eam actor")?;

        let ethaccount_code = *by_name
            .get(ETHACCOUNT_ACTOR_NAME)
            .context("manifest missing ethaccount actor")?;

        let miner_code = *by_name
            .get(MINER_ACTOR_NAME)
            .context("manifest missing miner actor")?;

        let power_code = *by_name
            .get(POWER_ACTOR_NAME)
            .context("manifest missing power actor")?;

        Ok(Self {
            account_code,
            system_code,
            init_code,
            placeholder_code,
            eam_code,
            ethaccount_code,
            miner_code,
            power_code,
            by_id,
            by_code,
        })
    }

    /// Returns the code CID for a builtin actor, given the actor's ID.
    pub fn code_by_id(&self, id: u32) -> Option<&Cid> {
        self.by_id.get(&id)
    }

    /// Returns the the actor code's "id" if it's a builtin actor. Otherwise,
    /// returns 0.
    pub fn id_by_code(&self, code: &Cid) -> u32 {
        self.by_code.get(code).copied().unwrap_or(0)
    }

    /// Returns true id the passed code CID is the account actor.
    pub fn is_account_actor(&self, cid: &Cid) -> bool {
        &self.account_code == cid
    }

    /// Returns true id the passed code CID is the placeholder actor.
    pub fn is_placeholder_actor(&self, cid: &Cid) -> bool {
        &self.placeholder_code == cid
    }

    /// Returns true id the passed code CID is the `EthAccount` actor.
    pub fn is_ethaccount_actor(&self, cid: &Cid) -> bool {
        &self.ethaccount_code == cid
    }

    pub fn builtin_actor_codes(&self) -> impl Iterator<Item = &Cid> {
        self.by_id.values()
    }

    /// Returns the code CID for the account actor.
    pub fn get_account_code(&self) -> &Cid {
        &self.account_code
    }

    /// Returns the code CID for the init actor.
    pub fn get_init_code(&self) -> &Cid {
        &self.init_code
    }

    /// Returns the code CID for the system actor.
    pub fn get_system_code(&self) -> &Cid {
        &self.system_code
    }

    /// Returns the code CID for the `eam` actor.
    pub fn get_eam_code(&self) -> &Cid {
        &self.eam_code
    }

    /// Returns the code CID for the system actor.
    pub fn get_placeholder_code(&self) -> &Cid {
        &self.placeholder_code
    }

    /// Returns the code CID for the Ethereum Account actor.
    pub fn get_ethaccount_code(&self) -> &Cid {
        &self.ethaccount_code
    }

    /// Returns the code CID for the miner actor.
    pub fn get_miner_code(&self) -> &Cid {
        &self.miner_code
    }

    /// Returns the code CID for the power actor.
    pub fn get_power_code(&self) -> &Cid {
        &self.power_code
    }
}
