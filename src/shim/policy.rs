// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_actors_shared::v10::runtime::Policy as PolicyV10;
use fil_actors_shared::v11::runtime::Policy as PolicyV11;
use fil_actors_shared::v11::runtime::ProofSet as ProofSetV11;
use fil_actors_shared::v12::runtime::Policy as PolicyV12;
use fil_actors_shared::v12::runtime::ProofSet as ProofSetV12;
use fil_actors_shared::v13::runtime::Policy as PolicyV13;
use fil_actors_shared::v9::runtime::Policy as PolicyV9;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, PartialEq, Eq, Clone, Serialize, Deserialize, derive_more::From, derive_more::Into,
)]
pub struct Policy(pub PolicyV13);

impl From<&Policy> for PolicyV12 {
    fn from(Policy(policy): &Policy) -> Self {
        let mut valid_post_proof_type = ProofSetV12::default();
        policy
            .valid_post_proof_type
            .clone()
            .into_inner()
            .iter()
            .for_each(|proof| valid_post_proof_type.insert(*proof));

        let mut valid_pre_commit_proof_type = ProofSetV12::default();
        policy
            .valid_pre_commit_proof_type
            .clone()
            .into_inner()
            .iter()
            .for_each(|proof| valid_pre_commit_proof_type.insert(*proof));

        PolicyV12 {
            max_aggregated_sectors: policy.max_aggregated_sectors,
            min_aggregated_sectors: policy.min_aggregated_sectors,
            max_aggregated_proof_size: policy.max_aggregated_proof_size,
            max_replica_update_proof_size: policy.max_replica_update_proof_size,
            pre_commit_sector_batch_max_size: policy.pre_commit_sector_batch_max_size,
            prove_replica_updates_max_size: policy.prove_replica_updates_max_size,
            expired_pre_commit_clean_up_delay: policy.expired_pre_commit_clean_up_delay,
            wpost_proving_period: policy.wpost_proving_period,
            wpost_challenge_window: policy.wpost_challenge_window,
            wpost_period_deadlines: policy.wpost_period_deadlines,
            wpost_max_chain_commit_age: policy.wpost_max_chain_commit_age,
            wpost_dispute_window: policy.wpost_dispute_window,
            sectors_max: policy.sectors_max,
            max_partitions_per_deadline: policy.max_partitions_per_deadline,
            max_control_addresses: policy.max_control_addresses,
            max_peer_id_length: policy.max_peer_id_length,
            max_multiaddr_data: policy.max_multiaddr_data,
            addressed_partitions_max: policy.addressed_partitions_max,
            declarations_max: policy.declarations_max,
            addressed_sectors_max: policy.addressed_sectors_max,
            max_pre_commit_randomness_lookback: policy.max_pre_commit_randomness_lookback,
            pre_commit_challenge_delay: policy.pre_commit_challenge_delay,
            wpost_challenge_lookback: policy.wpost_challenge_lookback,
            fault_declaration_cutoff: policy.fault_declaration_cutoff,
            fault_max_age: policy.fault_max_age,
            worker_key_change_delay: policy.worker_key_change_delay,
            min_sector_expiration: policy.min_sector_expiration,
            max_sector_expiration_extension: policy.max_sector_expiration_extension,
            deal_limit_denominator: policy.deal_limit_denominator,
            consensus_fault_ineligibility_duration: policy.consensus_fault_ineligibility_duration,
            new_sectors_per_period_max: policy.new_sectors_per_period_max,
            chain_finality: policy.chain_finality,
            valid_post_proof_type,
            valid_pre_commit_proof_type,
            minimum_verified_allocation_size: policy.minimum_verified_allocation_size.clone(),
            minimum_verified_allocation_term: policy.minimum_verified_allocation_term,
            maximum_verified_allocation_term: policy.maximum_verified_allocation_term,
            maximum_verified_allocation_expiration: policy.maximum_verified_allocation_expiration,
            end_of_life_claim_drop_period: policy.end_of_life_claim_drop_period,
            deal_updates_interval: policy.deal_updates_interval,
            prov_collateral_percent_supply_num: policy.prov_collateral_percent_supply_num,
            prov_collateral_percent_supply_denom: policy.prov_collateral_percent_supply_denom,
            market_default_allocation_term_buffer: policy.market_default_allocation_term_buffer,
            minimum_consensus_power: policy.minimum_consensus_power.clone(),
            posted_partitions_max: policy.posted_partitions_max,
        }
    }
}

impl From<&Policy> for PolicyV11 {
    fn from(Policy(policy): &Policy) -> Self {
        let mut valid_post_proof_type = ProofSetV11::default();
        policy
            .valid_post_proof_type
            .clone()
            .into_inner()
            .iter()
            .for_each(|proof| valid_post_proof_type.insert(*proof));

        let mut valid_pre_commit_proof_type = ProofSetV11::default();
        policy
            .valid_pre_commit_proof_type
            .clone()
            .into_inner()
            .iter()
            .for_each(|proof| valid_pre_commit_proof_type.insert(*proof));

        PolicyV11 {
            max_aggregated_sectors: policy.max_aggregated_sectors,
            min_aggregated_sectors: policy.min_aggregated_sectors,
            max_aggregated_proof_size: policy.max_aggregated_proof_size,
            max_replica_update_proof_size: policy.max_replica_update_proof_size,
            pre_commit_sector_batch_max_size: policy.pre_commit_sector_batch_max_size,
            prove_replica_updates_max_size: policy.prove_replica_updates_max_size,
            expired_pre_commit_clean_up_delay: policy.expired_pre_commit_clean_up_delay,
            wpost_proving_period: policy.wpost_proving_period,
            wpost_challenge_window: policy.wpost_challenge_window,
            wpost_period_deadlines: policy.wpost_period_deadlines,
            wpost_max_chain_commit_age: policy.wpost_max_chain_commit_age,
            wpost_dispute_window: policy.wpost_dispute_window,
            sectors_max: policy.sectors_max,
            max_partitions_per_deadline: policy.max_partitions_per_deadline,
            max_control_addresses: policy.max_control_addresses,
            max_peer_id_length: policy.max_peer_id_length,
            max_multiaddr_data: policy.max_multiaddr_data,
            addressed_partitions_max: policy.addressed_partitions_max,
            declarations_max: policy.declarations_max,
            addressed_sectors_max: policy.addressed_sectors_max,
            max_pre_commit_randomness_lookback: policy.max_pre_commit_randomness_lookback,
            pre_commit_challenge_delay: policy.pre_commit_challenge_delay,
            wpost_challenge_lookback: policy.wpost_challenge_lookback,
            fault_declaration_cutoff: policy.fault_declaration_cutoff,
            fault_max_age: policy.fault_max_age,
            worker_key_change_delay: policy.worker_key_change_delay,
            min_sector_expiration: policy.min_sector_expiration,
            max_sector_expiration_extension: policy.max_sector_expiration_extension,
            deal_limit_denominator: policy.deal_limit_denominator,
            consensus_fault_ineligibility_duration: policy.consensus_fault_ineligibility_duration,
            new_sectors_per_period_max: policy.new_sectors_per_period_max,
            chain_finality: policy.chain_finality,
            valid_post_proof_type,
            valid_pre_commit_proof_type,
            minimum_verified_allocation_size: policy.minimum_verified_allocation_size.clone(),
            minimum_verified_allocation_term: policy.minimum_verified_allocation_term,
            maximum_verified_allocation_term: policy.maximum_verified_allocation_term,
            maximum_verified_allocation_expiration: policy.maximum_verified_allocation_expiration,
            end_of_life_claim_drop_period: policy.end_of_life_claim_drop_period,
            deal_updates_interval: policy.deal_updates_interval,
            prov_collateral_percent_supply_num: policy.prov_collateral_percent_supply_num,
            prov_collateral_percent_supply_denom: policy.prov_collateral_percent_supply_denom,
            market_default_allocation_term_buffer: policy.market_default_allocation_term_buffer,
            minimum_consensus_power: policy.minimum_consensus_power.clone(),
        }
    }
}

impl From<&Policy> for PolicyV10 {
    fn from(Policy(policy): &Policy) -> Self {
        let valid_post_proof_type = policy
            .valid_post_proof_type
            .clone()
            .into_inner()
            .iter()
            .enumerate()
            .filter_map(|(i, &p)| if p { Some((i as i64).into()) } else { None })
            .collect();

        let valid_pre_commit_proof_type = policy
            .valid_pre_commit_proof_type
            .clone()
            .into_inner()
            .iter()
            .enumerate()
            .filter_map(|(i, &p)| if p { Some((i as i64).into()) } else { None })
            .collect();

        PolicyV10 {
            max_aggregated_sectors: policy.max_aggregated_sectors,
            min_aggregated_sectors: policy.min_aggregated_sectors,
            max_aggregated_proof_size: policy.max_aggregated_proof_size,
            max_replica_update_proof_size: policy.max_replica_update_proof_size,
            pre_commit_sector_batch_max_size: policy.pre_commit_sector_batch_max_size,
            prove_replica_updates_max_size: policy.prove_replica_updates_max_size,
            expired_pre_commit_clean_up_delay: policy.expired_pre_commit_clean_up_delay,
            wpost_proving_period: policy.wpost_proving_period,
            wpost_challenge_window: policy.wpost_challenge_window,
            wpost_period_deadlines: policy.wpost_period_deadlines,
            wpost_max_chain_commit_age: policy.wpost_max_chain_commit_age,
            wpost_dispute_window: policy.wpost_dispute_window,
            sectors_max: policy.sectors_max,
            max_partitions_per_deadline: policy.max_partitions_per_deadline,
            max_control_addresses: policy.max_control_addresses,
            max_peer_id_length: policy.max_peer_id_length,
            max_multiaddr_data: policy.max_multiaddr_data,
            addressed_partitions_max: policy.addressed_partitions_max,
            declarations_max: policy.declarations_max,
            addressed_sectors_max: policy.addressed_sectors_max,
            max_pre_commit_randomness_lookback: policy.max_pre_commit_randomness_lookback,
            pre_commit_challenge_delay: policy.pre_commit_challenge_delay,
            wpost_challenge_lookback: policy.wpost_challenge_lookback,
            fault_declaration_cutoff: policy.fault_declaration_cutoff,
            fault_max_age: policy.fault_max_age,
            worker_key_change_delay: policy.worker_key_change_delay,
            min_sector_expiration: policy.min_sector_expiration,
            max_sector_expiration_extension: policy.max_sector_expiration_extension,
            deal_limit_denominator: policy.deal_limit_denominator,
            consensus_fault_ineligibility_duration: policy.consensus_fault_ineligibility_duration,
            new_sectors_per_period_max: policy.new_sectors_per_period_max,
            chain_finality: policy.chain_finality,
            valid_post_proof_type,
            valid_pre_commit_proof_type,
            minimum_verified_allocation_size: policy.minimum_verified_allocation_size.clone(),
            minimum_verified_allocation_term: policy.minimum_verified_allocation_term,
            maximum_verified_allocation_term: policy.maximum_verified_allocation_term,
            maximum_verified_allocation_expiration: policy.maximum_verified_allocation_expiration,
            end_of_life_claim_drop_period: policy.end_of_life_claim_drop_period,
            deal_updates_interval: policy.deal_updates_interval,
            prov_collateral_percent_supply_num: policy.prov_collateral_percent_supply_num,
            prov_collateral_percent_supply_denom: policy.prov_collateral_percent_supply_denom,
            market_default_allocation_term_buffer: policy.market_default_allocation_term_buffer,
            minimum_consensus_power: policy.minimum_consensus_power.clone(),
        }
    }
}

impl From<&Policy> for PolicyV9 {
    fn from(Policy(policy): &Policy) -> Self {
        let valid_post_proof_type = policy
            .valid_post_proof_type
            .clone()
            .into_inner()
            .iter()
            .enumerate()
            .filter_map(|(i, &p)| if p { Some((i as i64).into()) } else { None })
            .collect();

        let valid_pre_commit_proof_type = policy
            .valid_pre_commit_proof_type
            .clone()
            .into_inner()
            .iter()
            .enumerate()
            .filter_map(|(i, &p)| if p { Some((i as i64).into()) } else { None })
            .collect();

        PolicyV9 {
            max_aggregated_sectors: policy.max_aggregated_sectors,
            min_aggregated_sectors: policy.min_aggregated_sectors,
            max_aggregated_proof_size: policy.max_aggregated_proof_size,
            max_replica_update_proof_size: policy.max_replica_update_proof_size,
            pre_commit_sector_batch_max_size: policy.pre_commit_sector_batch_max_size,
            prove_replica_updates_max_size: policy.prove_replica_updates_max_size,
            expired_pre_commit_clean_up_delay: policy.expired_pre_commit_clean_up_delay,
            wpost_proving_period: policy.wpost_proving_period,
            wpost_challenge_window: policy.wpost_challenge_window,
            wpost_period_deadlines: policy.wpost_period_deadlines,
            wpost_max_chain_commit_age: policy.wpost_max_chain_commit_age,
            wpost_dispute_window: policy.wpost_dispute_window,
            sectors_max: policy.sectors_max,
            max_partitions_per_deadline: policy.max_partitions_per_deadline,
            max_control_addresses: policy.max_control_addresses,
            max_peer_id_length: policy.max_peer_id_length,
            max_multiaddr_data: policy.max_multiaddr_data,
            addressed_partitions_max: policy.addressed_partitions_max,
            declarations_max: policy.declarations_max,
            addressed_sectors_max: policy.addressed_sectors_max,
            max_pre_commit_randomness_lookback: policy.max_pre_commit_randomness_lookback,
            pre_commit_challenge_delay: policy.pre_commit_challenge_delay,
            wpost_challenge_lookback: policy.wpost_challenge_lookback,
            fault_declaration_cutoff: policy.fault_declaration_cutoff,
            fault_max_age: policy.fault_max_age,
            worker_key_change_delay: policy.worker_key_change_delay,
            min_sector_expiration: policy.min_sector_expiration,
            max_sector_expiration_extension: policy.max_sector_expiration_extension,
            deal_limit_denominator: policy.deal_limit_denominator,
            consensus_fault_ineligibility_duration: policy.consensus_fault_ineligibility_duration,
            new_sectors_per_period_max: policy.new_sectors_per_period_max,
            chain_finality: policy.chain_finality,
            valid_post_proof_type,
            valid_pre_commit_proof_type,
            minimum_verified_allocation_size: policy.minimum_verified_allocation_size.clone(),
            minimum_verified_allocation_term: policy.minimum_verified_allocation_term,
            maximum_verified_allocation_term: policy.maximum_verified_allocation_term,
            maximum_verified_allocation_expiration: policy.maximum_verified_allocation_expiration,
            end_of_life_claim_drop_period: policy.end_of_life_claim_drop_period,
            deal_updates_interval: policy.deal_updates_interval,
            prov_collateral_percent_supply_num: policy.prov_collateral_percent_supply_num,
            prov_collateral_percent_supply_denom: policy.prov_collateral_percent_supply_denom,
            market_default_allocation_term_buffer: policy.market_default_allocation_term_buffer,
            minimum_consensus_power: policy.minimum_consensus_power.clone(),
        }
    }
}
