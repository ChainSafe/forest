// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::{
    address::{Address, Payload},
    randomness::Randomness,
    sector::{RegisteredPoStProof, RegisteredSealProof, SectorInfo},
    state_tree::ActorState,
    version::NetworkVersion,
};
use crate::utils::encoding::prover_id_from_u64;
use cid::Cid;
use fil_actor_interface::{is_account_actor, is_eth_account_actor, is_placeholder_actor, miner};
use filecoin_proofs_api::post;
use fvm_ipld_bitfield::BitField;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::bytes_32;

use crate::state_manager::{errors::*, StateManager};

impl<DB> StateManager<DB>
where
    DB: Blockstore,
{
    /// Retrieves and generates a vector of sector info for the winning `PoSt`
    /// verification.
    pub fn get_sectors_for_winning_post(
        &self,
        st: &Cid,
        nv: NetworkVersion,
        miner_address: &Address,
        rand: Randomness,
    ) -> Result<Vec<SectorInfo>, anyhow::Error> {
        let store = self.blockstore();

        let actor = self
            .get_actor(miner_address, *st)?
            .ok_or_else(|| Error::State("Miner actor address could not be resolved".to_string()))?;
        let mas = miner::State::load(self.blockstore(), actor.code, actor.state)?;

        let proving_sectors = {
            let mut proving_sectors = BitField::new();

            if nv < NetworkVersion::V7 {
                mas.for_each_deadline(&self.chain_config.policy, store, |_, deadline| {
                    let mut fault_sectors = BitField::new();
                    deadline.for_each(store, |_, partition: miner::Partition| {
                        proving_sectors |= partition.all_sectors();
                        fault_sectors |= partition.faulty_sectors();
                        Ok(())
                    })?;

                    proving_sectors -= &fault_sectors;
                    Ok(())
                })?;
            } else {
                mas.for_each_deadline(&self.chain_config.policy, store, |_, deadline| {
                    deadline.for_each(store, |_, partition: miner::Partition| {
                        proving_sectors |= &partition.active_sectors();
                        Ok(())
                    })?;
                    Ok(())
                })?;
            }
            proving_sectors
        };

        let num_prov_sect = proving_sectors.len();

        if num_prov_sect == 0 {
            return Ok(Vec::new());
        }

        let info = mas.info(store)?;
        let spt = RegisteredSealProof::from_sector_size(info.sector_size().into(), nv);

        let wpt = spt
            .registered_winning_post_proof()
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let m_id = miner_address.id()?;

        let ids = generate_winning_post_sector_challenge(wpt.into(), m_id, rand, num_prov_sect)?;

        let mut iter = proving_sectors.iter();

        let mut selected_sectors = BitField::new();
        for n in ids {
            let sno = iter.nth(n as usize).ok_or_else(|| {
                anyhow::anyhow!(
                    "Error iterating over proving sectors, id {} does not exist",
                    n
                )
            })?;
            selected_sectors.set(sno);
        }

        let sectors = mas.load_sectors(store, Some(&selected_sectors))?;

        let out = sectors
            .into_iter()
            .map(|s_info| SectorInfo::new(*spt, s_info.sector_number, s_info.sealed_cid))
            .collect();

        Ok(out)
    }
}

pub fn is_valid_for_sending(network_version: NetworkVersion, actor: &ActorState) -> bool {
    // Comments from Lotus:
    // Before nv18 (Hygge), we only supported built-in account actors as senders.
    //
    // Note: this gate is probably superfluous, since:
    // 1. Placeholder actors cannot be created before nv18.
    // 2. EthAccount actors cannot be created before nv18.
    // 3. Delegated addresses cannot be created before nv18.
    //
    // But it's a safeguard.
    //
    // Note 2: ad-hoc checks for network versions like this across the codebase
    // will be problematic with networks with diverging version lineages
    // (e.g. Hyperspace). We need to revisit this strategy entirely.
    if network_version < NetworkVersion::V18 {
        return is_account_actor(&actor.code);
    }

    // After nv18, we also support other kinds of senders.
    if is_account_actor(&actor.code)
        || is_eth_account_actor(&actor.code)
        // XXX: Remove this once 'is_eth_account_actor' is fixed
        || fil_actor_interface::ethaccount::is_v11_ethaccount_cid(&actor.code)
    {
        return true;
    }

    // Allow placeholder actors with a delegated address and nonce 0 to send a
    // message. These will be converted to an EthAccount actor on first send.
    if !is_placeholder_actor(&actor.code)
        || actor.sequence != 0
        || actor.delegated_address.is_none()
    {
        return false;
    }

    // Only allow such actors to send if their delegated address is in the EAM's
    // namespace.
    return if let Payload::Delegated(address) = actor
        .delegated_address
        .as_ref()
        .expect("unfallible")
        .payload()
    {
        address.namespace() == Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR.id().unwrap()
    } else {
        false
    };
}

/// Generates sector challenge indexes for use in winning PoSt verification.
fn generate_winning_post_sector_challenge(
    proof: RegisteredPoStProof,
    prover_id: u64,
    mut rand: Randomness,
    eligible_sector_count: u64,
) -> Result<Vec<u64>, anyhow::Error> {
    // Necessary to be valid bls12 381 element.
    rand.0[31] &= 0x3f;

    post::generate_winning_post_sector_challenge(
        proof.try_into()?,
        &bytes_32(&rand.0),
        eligible_sector_count,
        prover_id_from_u64(prover_id),
    )
}

#[cfg(test)]
mod test {
    use crate::shim::{address::Address, econ::TokenAmount, state_tree::ActorState};
    use cid::Cid;

    use super::*;

    #[test]
    fn is_valid_for_sending_test() {
        let create_actor = |code: &Cid, sequence: u64, delegated_address: Option<Address>| {
            ActorState::new(
                code.to_owned(),
                // changing this cid will unleash unthinkable horrors upon the world
                Cid::try_from("bafk2bzaceavfgpiw6whqigmskk74z4blm22nwjfnzxb4unlqz2e4wgcthulhu")
                    .unwrap(),
                TokenAmount::default(),
                sequence,
                delegated_address,
            )
        };

        // calibnet actor version 10
        let account_actor_cid =
            Cid::try_from("bafk2bzaceavfgpiw6whqigmskk74z4blm22nwjfnzxb4unlqz2e4wg3c5ujpw")
                .unwrap();
        let ethaccount_actor_cid =
            Cid::try_from("bafk2bzacebiyrhz32xwxi6xql67aaq5nrzeelzas472kuwjqmdmgwotpkj35e")
                .unwrap();
        let placeholder_actor_cid =
            Cid::try_from("bafk2bzacedfvut2myeleyq67fljcrw4kkmn5pb5dpyozovj7jpoez5irnc3ro")
                .unwrap();

        // happy path for account actor
        let actor = create_actor(&account_actor_cid, 0, None);
        assert!(is_valid_for_sending(NetworkVersion::V17, &actor));

        // eth account not allowed before v18, should fail
        let actor = create_actor(&ethaccount_actor_cid, 0, None);
        assert!(!is_valid_for_sending(NetworkVersion::V17, &actor));

        // happy path for eth account
        assert!(is_valid_for_sending(NetworkVersion::V18, &actor));

        // no delegated address for placeholder actor, should fail
        let actor = create_actor(&placeholder_actor_cid, 0, None);
        assert!(!is_valid_for_sending(NetworkVersion::V18, &actor));

        // happy path for the placeholder actor
        let delegated_address = Address::new_delegated(
            Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR.id().unwrap(),
            &[0; 20],
        )
        .ok();
        let actor = create_actor(&placeholder_actor_cid, 0, delegated_address);
        assert!(is_valid_for_sending(NetworkVersion::V18, &actor));

        // sequence not 0, should fail
        let actor = create_actor(&placeholder_actor_cid, 1, delegated_address);
        assert!(!is_valid_for_sending(NetworkVersion::V18, &actor));

        // delegated address not in EAM namespace, should fail
        let delegated_address =
            Address::new_delegated(Address::CHAOS_ACTOR.id().unwrap(), &[0; 20]).ok();
        let actor = create_actor(&placeholder_actor_cid, 0, delegated_address);
        assert!(!is_valid_for_sending(NetworkVersion::V18, &actor));
    }
}
