// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_actor_datacap_state::v9::State as DataCapStateV9;
use forest_shim::{bigint::StoragePowerV2, econ::TokenAmount_v2};
use fvm_ipld_hamt::BytesKey;

use crate::common::{TypeMigration, TypeMigrator};

const TOKEN_PRECISION: u64 = 1_000_000_000_000_000_000;
const DATA_CAP_GRANULARITY: u64 = TOKEN_PRECISION;
lazy_static::lazy_static! {
    static ref INFINITE_ALLOWANCE: StoragePowerV2 = StoragePowerV2::from_str("1000000000000000000000").expect("Failed to parse INFINITE_ALLOWANCE") * TOKEN_PRECISION;
}

impl TypeMigration<(), DataCapStateV9> for TypeMigrator {
    fn migrate_type(_: (), store: &impl Blockstore) -> anyhow::Result<MarketStateV9> {
        // The DataCap actor -- needs to be created, and loading the verified registry
        // state
        let verifreg_actor = actors_out
            .get_actor(&Address::new_id(
                fil_actors_shared::v8::VERIFIED_REGISTRY_ACTOR_ADDR.id()?,
            ))?
            .context("Failed to load verifreg actor v8")?;
        let verifreg_state: fil_actor_verifreg_state::v8::State = store
            .get_cbor(&verifreg_actor.state)?
            .context("Failed to load verifreg state v8")?;
        let verified_clients = fil_actors_shared::v8::make_map_with_root::<_, StoragePowerV2>(
            &verifreg_state.verified_clients,
            store,
        )?;

        // load market proposals
        let market_actor = actors_out
            .get_actor(&Address::new_id(
                fil_actors_shared::v8::STORAGE_MARKET_ACTOR_ADDR.id()?,
            ))?
            .context("Failed to load market actor v8")?;
        let market_state: fil_actor_market_state::v8::State = store
            .get_cbor(&market_actor.state)?
            .context("Failed to load market state v8")?;

        let (pending_verified_deals, pending_verified_deal_size) =
            get_pending_verified_deals_and_total_size(store, market_state)?;

        let mut token_supply = StoragePowerV2::default();

        let mut balances_map = fil_actors_shared::v9::make_empty_map::<_, StoragePowerV2>(
            store,
            fil_actors_shared::v9::builtin::HAMT_BIT_WIDTH,
        );

        let mut allowances_map = fil_actors_shared::v9::make_empty_map(
            store,
            fil_actors_shared::v9::builtin::HAMT_BIT_WIDTH,
        );

        verified_clients.for_each(|key, value| {
            let key2 = BytesKey(key[1..].to_vec());
            let token_amount = &value * DATA_CAP_GRANULARITY;
            token_supply = &token_supply + &token_amount;
            balances_map.set(key2.clone(), token_amount)?;

            let mut allowances_map_entry = fil_actors_shared::v9::make_empty_map::<_, StoragePowerV2>(
                store,
                fil_actors_shared::v9::builtin::HAMT_BIT_WIDTH,
            );
            allowances_map_entry.set(
                BytesKey(fil_actors_shared::v9::builtin::STORAGE_MARKET_ACTOR_ADDR.payload_bytes()),
                INFINITE_ALLOWANCE.clone(),
            )?;
            allowances_map.set(key2, allowances_map_entry)?;
            Ok(())
        })?;

        let verifreg_balance =
            StoragePowerV2::from(pending_verified_deal_size) * DATA_CAP_GRANULARITY;
        token_supply = &token_supply + verifreg_balance;
        balances_map.set(
            BytesKey(fil_actors_shared::v9::builtin::VERIFIED_REGISTRY_ACTOR_ADDR.payload_bytes()),
            verifreg_balance,
        )?;

        let out_state = fil_actor_datacap_state::v9::State {
            governor: fil_actors_shared::v9::builtin::VERIFIED_REGISTRY_ACTOR_ADDR,
            token: frc46_token::token::state::TokenState {
                supply: TokenAmount_v2::from_atto(token_supply),
                balances: balances_map.flush()?,
                allowances: allowances_map.flush()?,
                hamt_bit_width: fil_actors_shared::v9::builtin::HAMT_BIT_WIDTH,
            },
        };

        Ok(out_state)
    }
}
