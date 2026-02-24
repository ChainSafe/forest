// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod types;
use futures::stream::FuturesOrdered;
pub use types::*;

use super::chain::ChainGetTipSetV2;
use crate::blocks::{Tipset, TipsetKey};
use crate::chain::index::ResolveNullTipset;
use crate::cid_collections::CidHashSet;
use crate::eth::EthChainId;
use crate::interpreter::{MessageCallbackCtx, VMTrace};
use crate::libp2p::NetworkMessage;
use crate::lotus_json::{LotusJson, lotus_json_with_self};
use crate::networks::ChainConfig;
use crate::rpc::registry::actors_reg::load_and_serialize_actor_state;
use crate::shim::actors::market::DealState;
use crate::shim::actors::market::ext::MarketStateExt as _;
use crate::shim::actors::miner::ext::DeadlineExt;
use crate::shim::actors::state_load::*;
use crate::shim::actors::verifreg::ext::VerifiedRegistryStateExt as _;
use crate::shim::actors::verifreg::{Allocation, AllocationID, Claim};
use crate::shim::actors::{init, system};
use crate::shim::actors::{
    market, miner,
    miner::{MinerInfo, MinerPower},
    power, reward, verifreg,
};
use crate::shim::actors::{
    market::ext::BalanceTableExt as _, miner::ext::MinerStateExt as _,
    power::ext::PowerStateExt as _,
};
use crate::shim::address::Payload;
use crate::shim::machine::BuiltinActorManifest;
use crate::shim::message::{Message, MethodNum};
use crate::shim::sector::{SectorNumber, SectorSize};
use crate::shim::state_tree::{ActorID, StateTree};
use crate::shim::{
    address::Address, clock::ChainEpoch, deal::DealID, econ::TokenAmount, executor::Receipt,
    state_tree::ActorState, version::NetworkVersion,
};
use crate::state_manager::{
    MarketBalance, StateManager, StateOutput, circulating_supply::GenesisInfo, utils::structured,
};
use crate::utils::db::car_stream::{CarBlock, CarWriter};
use crate::{
    beacon::BeaconEntry,
    rpc::{ApiPaths, Ctx, Permission, RpcMethod, ServerError, types::*},
};
use ahash::{HashMap, HashMapExt, HashSet};
use anyhow::Context;
use anyhow::Result;
use cid::Cid;
use enumflags2::{BitFlags, make_bitflags};
use fil_actor_miner_state::v10::{qa_power_for_weight, qa_power_max};
use fil_actor_verifreg_state::v13::ClaimID;
use fil_actors_shared::fvm_ipld_amt::Amt;
use fil_actors_shared::fvm_ipld_bitfield::BitField;
use futures::{StreamExt as _, TryStreamExt as _};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{CborStore, DAG_CBOR};
pub use fvm_shared3::sector::StoragePower;
use ipld_core::ipld::Ipld;
use jsonrpsee::types::error::ErrorObject;
use num_bigint::BigInt;
use num_traits::Euclid;
use nunny::vec as nonempty;
use parking_lot::Mutex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use std::ops::Mul;
use std::path::PathBuf;
use std::{sync::Arc, time::Duration};
use tokio::task::JoinSet;

const INITIAL_PLEDGE_NUM: u64 = 110;
const INITIAL_PLEDGE_DEN: u64 = 100;

pub enum StateCall {}

impl StateCall {
    pub fn run<DB: Blockstore + Send + Sync + 'static>(
        state_manager: &StateManager<DB>,
        message: &Message,
        tsk: Option<TipsetKey>,
    ) -> anyhow::Result<ApiInvocResult> {
        let tipset = state_manager
            .chain_store()
            .load_required_tipset_or_heaviest(&tsk)?;
        Ok(state_manager.call(message, Some(tipset))?)
    }
}

impl RpcMethod<2> for StateCall {
    const NAME: &'static str = "Filecoin.StateCall";
    const PARAM_NAMES: [&'static str; 2] = ["message", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Runs the given message and returns its result without persisting changes. The message is applied to the tipset's parent state.",
    );

    type Params = (Message, ApiTipsetKey);
    type Ok = ApiInvocResult;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (message, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(Self::run(&ctx.state_manager, &message, tsk)?)
    }
}

pub enum StateReplay {}
impl RpcMethod<2> for StateReplay {
    const NAME: &'static str = "Filecoin.StateReplay";
    const PARAM_NAMES: [&'static str; 2] = ["tipsetKey", "messageCid"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Replays a given message, assuming it was included in a block in the specified tipset.",
    );

    type Params = (ApiTipsetKey, Cid);
    type Ok = ApiInvocResult;

    /// returns the result of executing the indicated message, assuming it was
    /// executed in the indicated tipset.
    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (ApiTipsetKey(tsk), message_cid): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let tipset = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        Ok(ctx.state_manager.replay(tipset, message_cid).await?)
    }
}

pub enum StateNetworkName {}
impl RpcMethod<0> for StateNetworkName {
    const NAME: &'static str = "Filecoin.StateNetworkName";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = String;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let heaviest_tipset = ctx.chain_store().heaviest_tipset();
        Ok(ctx
            .state_manager
            .get_network_state_name(*heaviest_tipset.parent_state())?
            .into())
    }
}

pub enum StateNetworkVersion {}
impl RpcMethod<1> for StateNetworkVersion {
    const NAME: &'static str = "Filecoin.StateNetworkVersion";
    const PARAM_NAMES: [&'static str; 1] = ["tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the network version at the given tipset.");

    type Params = (ApiTipsetKey,);
    type Ok = NetworkVersion;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (ApiTipsetKey(tsk),): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        Ok(ctx.state_manager.get_network_version(ts.epoch()))
    }
}

/// gets the public key address of the given ID address
/// See <https://github.com/filecoin-project/lotus/blob/master/documentation/en/api-methods-v0-deprecated.md#StateAccountKey>
pub enum StateAccountKey {}

impl RpcMethod<2> for StateAccountKey {
    const NAME: &'static str = "Filecoin.StateAccountKey";
    const PARAM_NAMES: [&'static str; 2] = ["address", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the public key address for the given ID address (secp and bls accounts).");

    type Params = (Address, ApiTipsetKey);
    type Ok = Address;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        Ok(ctx
            .state_manager
            .resolve_to_deterministic_address(address, &ts)
            .await?)
    }
}

/// retrieves the ID address of the given address
/// See <https://github.com/filecoin-project/lotus/blob/master/documentation/en/api-methods-v0-deprecated.md#StateLookupID>
pub enum StateLookupID {}

impl RpcMethod<2> for StateLookupID {
    const NAME: &'static str = "Filecoin.StateLookupID";
    const PARAM_NAMES: [&'static str; 2] = ["address", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Retrieves the ID address of the given address.");

    type Params = (Address, ApiTipsetKey);
    type Ok = Address;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        Ok(ctx.state_manager.lookup_required_id(&address, &ts)?)
    }
}

/// `StateVerifiedRegistryRootKey` returns the address of the Verified Registry's root key
pub enum StateVerifiedRegistryRootKey {}

impl RpcMethod<1> for StateVerifiedRegistryRootKey {
    const NAME: &'static str = "Filecoin.StateVerifiedRegistryRootKey";
    const PARAM_NAMES: [&'static str; 1] = ["tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the address of the Verified Registry's root key.");

    type Params = (ApiTipsetKey,);
    type Ok = Address;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (ApiTipsetKey(tsk),): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let state: verifreg::State = ctx.state_manager.get_actor_state(&ts)?;
        Ok(state.root_key())
    }
}

// StateVerifiedClientStatus returns the data cap for the given address.
// Returns zero if there is no entry in the data cap table for the address.
pub enum StateVerifierStatus {}

impl RpcMethod<2> for StateVerifierStatus {
    const NAME: &'static str = "Filecoin.StateVerifierStatus";
    const PARAM_NAMES: [&'static str; 2] = ["address", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns the data cap for the given address.");

    type Params = (Address, ApiTipsetKey);
    type Ok = Option<StoragePower>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let aid = ctx.state_manager.lookup_required_id(&address, &ts)?;
        let verifreg_state: verifreg::State = ctx.state_manager.get_actor_state(&ts)?;
        Ok(verifreg_state.verifier_data_cap(ctx.store(), aid)?)
    }
}

pub enum StateGetActor {}

impl RpcMethod<2> for StateGetActor {
    const NAME: &'static str = "Filecoin.StateGetActor";
    const PARAM_NAMES: [&'static str; 2] = ["address", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the nonce and balance for the specified actor.");

    type Params = (Address, ApiTipsetKey);
    type Ok = Option<ActorState>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let state = ctx.state_manager.get_actor(&address, *ts.parent_state())?;
        Ok(state)
    }
}

pub enum StateGetActorV2 {}

impl RpcMethod<2> for StateGetActorV2 {
    const NAME: &'static str = "Filecoin.StateGetActor";
    const PARAM_NAMES: [&'static str; 2] = ["address", "tipsetSelector"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::{ V2 });
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the nonce and balance for the specified actor.");

    type Params = (Address, TipsetSelector);
    type Ok = Option<ActorState>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, selector): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ChainGetTipSetV2::get_required_tipset(&ctx, &selector).await?;
        Ok(ctx.state_manager.get_actor(&address, *ts.parent_state())?)
    }
}

pub enum StateGetID {}

impl RpcMethod<2> for StateGetID {
    const NAME: &'static str = "Filecoin.StateGetID";
    const PARAM_NAMES: [&'static str; 2] = ["address", "tipsetSelector"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::{ V2 });
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Retrieves the ID address for the specified address at the selected tipset.");

    type Params = (Address, TipsetSelector);
    type Ok = Address;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, selector): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ChainGetTipSetV2::get_required_tipset(&ctx, &selector).await?;
        Ok(ctx.state_manager.lookup_required_id(&address, &ts)?)
    }
}

pub enum StateLookupRobustAddress {}

macro_rules! get_robust_address {
    ($store:expr, $id_addr_decoded:expr, $state:expr, $make_map_with_root:path, $robust_addr:expr) => {{
        let map = $make_map_with_root(&$state.address_map, &$store)?;
        map.for_each(|addr, v| {
            if *v == $id_addr_decoded {
                $robust_addr = Address::from_bytes(addr)?;
                return Ok(());
            }
            Ok(())
        })?;
        Ok($robust_addr)
    }};
}

impl RpcMethod<2> for StateLookupRobustAddress {
    const NAME: &'static str = "Filecoin.StateLookupRobustAddress";
    const PARAM_NAMES: [&'static str; 2] = ["address", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the public key address for non-account addresses (e.g., multisig, miners).");

    type Params = (Address, ApiTipsetKey);
    type Ok = Address;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (addr, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let store = ctx.store();
        let state_tree = StateTree::new_from_root(ctx.store_owned(), ts.parent_state())?;
        if let &Payload::ID(id_addr_decoded) = addr.payload() {
            let init_state: init::State = state_tree.get_actor_state()?;
            let mut robust_addr = Address::default();
            match init_state {
                init::State::V0(_) => Err(ServerError::internal_error(
                    "StateLookupRobustAddress is not implemented for init state v0",
                    None,
                )),
                init::State::V8(state) => get_robust_address!(
                    store,
                    id_addr_decoded,
                    state,
                    fil_actors_shared::v8::make_map_with_root::<_, ActorID>,
                    robust_addr
                ),
                init::State::V9(state) => get_robust_address!(
                    store,
                    id_addr_decoded,
                    state,
                    fil_actors_shared::v9::make_map_with_root::<_, ActorID>,
                    robust_addr
                ),
                init::State::V10(state) => get_robust_address!(
                    store,
                    id_addr_decoded,
                    state,
                    fil_actors_shared::v10::make_map_with_root::<_, ActorID>,
                    robust_addr
                ),
                init::State::V11(state) => get_robust_address!(
                    store,
                    id_addr_decoded,
                    state,
                    fil_actors_shared::v11::make_map_with_root::<_, ActorID>,
                    robust_addr
                ),
                init::State::V12(state) => get_robust_address!(
                    store,
                    id_addr_decoded,
                    state,
                    fil_actors_shared::v12::make_map_with_root::<_, ActorID>,
                    robust_addr
                ),
                init::State::V13(state) => get_robust_address!(
                    store,
                    id_addr_decoded,
                    state,
                    fil_actors_shared::v13::make_map_with_root::<_, ActorID>,
                    robust_addr
                ),
                init::State::V14(state) => {
                    let map = fil_actor_init_state::v14::AddressMap::load(
                        &store,
                        &state.address_map,
                        fil_actors_shared::v14::DEFAULT_HAMT_CONFIG,
                        "address_map",
                    )
                    .context("Failed to load address map")?;
                    map.for_each(|addr, v| {
                        if *v == id_addr_decoded {
                            robust_addr = addr.into();
                            return Ok(());
                        }
                        Ok(())
                    })
                    .context("Robust address not found")?;
                    Ok(robust_addr)
                }
                init::State::V15(state) => {
                    let map = fil_actor_init_state::v15::AddressMap::load(
                        &store,
                        &state.address_map,
                        fil_actors_shared::v15::DEFAULT_HAMT_CONFIG,
                        "address_map",
                    )
                    .context("Failed to load address map")?;
                    map.for_each(|addr, v| {
                        if *v == id_addr_decoded {
                            robust_addr = addr.into();
                            return Ok(());
                        }
                        Ok(())
                    })
                    .context("Robust address not found")?;
                    Ok(robust_addr)
                }
                init::State::V16(state) => {
                    let map = fil_actor_init_state::v16::AddressMap::load(
                        &store,
                        &state.address_map,
                        fil_actors_shared::v16::DEFAULT_HAMT_CONFIG,
                        "address_map",
                    )
                    .context("Failed to load address map")?;
                    map.for_each(|addr, v| {
                        if *v == id_addr_decoded {
                            robust_addr = addr.into();
                            return Ok(());
                        }
                        Ok(())
                    })
                    .context("Robust address not found")?;
                    Ok(robust_addr)
                }
                init::State::V17(state) => {
                    let map = fil_actor_init_state::v17::AddressMap::load(
                        &store,
                        &state.address_map,
                        fil_actors_shared::v17::DEFAULT_HAMT_CONFIG,
                        "address_map",
                    )
                    .context("Failed to load address map")?;
                    map.for_each(|addr, v| {
                        if *v == id_addr_decoded {
                            robust_addr = addr.into();
                            return Ok(());
                        }
                        Ok(())
                    })
                    .context("Robust address not found")?;
                    Ok(robust_addr)
                }
            }
        } else {
            Ok(Address::default())
        }
    }
}

/// looks up the Escrow and Locked balances of the given address in the Storage
/// Market
pub enum StateMarketBalance {}

impl RpcMethod<2> for StateMarketBalance {
    const NAME: &'static str = "Filecoin.StateMarketBalance";
    const PARAM_NAMES: [&'static str; 2] = ["address", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Returns the Escrow and Locked balances of the specified address in the Storage Market.",
    );

    type Params = (Address, ApiTipsetKey);
    type Ok = MarketBalance;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        ctx.state_manager
            .market_balance(&address, &ts)
            .map_err(From::from)
    }
}

pub enum StateMarketDeals {}

impl RpcMethod<1> for StateMarketDeals {
    const NAME: &'static str = "Filecoin.StateMarketDeals";
    const PARAM_NAMES: [&'static str; 1] = ["tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns information about every deal in the Storage Market.");

    type Params = (ApiTipsetKey,);
    type Ok = HashMap<String, ApiMarketDeal>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (ApiTipsetKey(tsk),): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let market_state: market::State = ctx.state_manager.get_actor_state(&ts)?;

        let da = market_state.proposals(ctx.store())?;
        let sa = market_state.states(ctx.store())?;

        let mut out = HashMap::new();
        da.for_each(|deal_id, d| {
            let s = sa.get(deal_id)?.unwrap_or(market::DealState {
                sector_start_epoch: -1,
                last_updated_epoch: -1,
                slash_epoch: -1,
                verified_claim: 0,
                sector_number: 0,
            });
            out.insert(
                deal_id.to_string(),
                MarketDeal {
                    proposal: d?,
                    state: s,
                }
                .into(),
            );
            Ok(())
        })?;
        Ok(out)
    }
}

/// looks up the miner info of the given address.
pub enum StateMinerInfo {}

impl RpcMethod<2> for StateMinerInfo {
    const NAME: &'static str = "Filecoin.StateMinerInfo";
    const PARAM_NAMES: [&'static str; 2] = ["minerAddress", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns information about the specified miner.");

    type Params = (Address, ApiTipsetKey);
    type Ok = MinerInfo;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        Ok(ctx.state_manager.miner_info(&address, &ts)?)
    }
}

pub enum StateMinerActiveSectors {}

impl RpcMethod<2> for StateMinerActiveSectors {
    const NAME: &'static str = "Filecoin.StateMinerActiveSectors";
    const PARAM_NAMES: [&'static str; 2] = ["minerAddress", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns information about sectors actively proven by a given miner.");

    type Params = (Address, ApiTipsetKey);
    type Ok = Vec<SectorOnChainInfo>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let policy = &ctx.chain_config().policy;
        let miner_state: miner::State = ctx
            .state_manager
            .get_actor_state_from_address(&ts, &address)?;
        // Collect active sectors from each partition in each deadline.
        let mut active_sectors = vec![];
        miner_state.for_each_deadline(policy, ctx.store(), |_dlidx, deadline| {
            deadline.for_each(ctx.store(), |_partidx, partition| {
                active_sectors.push(partition.active_sectors());
                Ok(())
            })
        })?;
        let sectors =
            miner_state.load_sectors_ext(ctx.store(), Some(&BitField::union(&active_sectors)))?;
        Ok(sectors)
    }
}

/// Returns a bitfield containing all sector numbers marked as allocated in miner state
pub enum StateMinerAllocated {}

impl RpcMethod<2> for StateMinerAllocated {
    const NAME: &'static str = "Filecoin.StateMinerAllocated";
    const PARAM_NAMES: [&'static str; 2] = ["minerAddress", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Returns a bitfield containing all sector numbers marked as allocated to the provided miner ID.",
    );

    type Params = (Address, ApiTipsetKey);
    type Ok = BitField;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let miner_state: miner::State = ctx
            .state_manager
            .get_actor_state_from_address(&ts, &address)?;
        Ok(miner_state.load_allocated_sector_numbers(ctx.store())?)
    }
}

/// Return all partitions in the specified deadline
pub enum StateMinerPartitions {}

impl RpcMethod<3> for StateMinerPartitions {
    const NAME: &'static str = "Filecoin.StateMinerPartitions";
    const PARAM_NAMES: [&'static str; 3] = ["minerAddress", "deadlineIndex", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns all partitions in the specified deadline.");

    type Params = (Address, u64, ApiTipsetKey);
    type Ok = Vec<MinerPartitions>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, dl_idx, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let policy = &ctx.chain_config().policy;
        let miner_state: miner::State = ctx
            .state_manager
            .get_actor_state_from_address(&ts, &address)?;
        let deadline = miner_state.load_deadline(policy, ctx.store(), dl_idx)?;
        let mut all_partitions = Vec::new();
        deadline.for_each(ctx.store(), |_partidx, partition| {
            all_partitions.push(MinerPartitions::new(
                partition.all_sectors(),
                partition.faulty_sectors(),
                partition.recovering_sectors(),
                partition.live_sectors(),
                partition.active_sectors(),
            ));
            Ok(())
        })?;
        Ok(all_partitions)
    }
}

pub enum StateMinerSectors {}

impl RpcMethod<3> for StateMinerSectors {
    const NAME: &'static str = "Filecoin.StateMinerSectors";
    const PARAM_NAMES: [&'static str; 3] = ["minerAddress", "sectors", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Returns information about the given miner's sectors. If no filter is provided, all sectors are included.",
    );

    type Params = (Address, Option<BitField>, ApiTipsetKey);
    type Ok = Vec<SectorOnChainInfo>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, sectors, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let miner_state: miner::State = ctx
            .state_manager
            .get_actor_state_from_address(&ts, &address)?;
        Ok(miner_state.load_sectors_ext(ctx.store(), sectors.as_ref())?)
    }
}

/// Returns the number of sectors in a miner's sector set and proving set
pub enum StateMinerSectorCount {}

impl RpcMethod<2> for StateMinerSectorCount {
    const NAME: &'static str = "Filecoin.StateMinerSectorCount";
    const PARAM_NAMES: [&'static str; 2] = ["minerAddress", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the number of sectors in a miner's sector and proving sets.");

    type Params = (Address, ApiTipsetKey);
    type Ok = MinerSectors;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let policy = &ctx.chain_config().policy;
        let miner_state: miner::State = ctx
            .state_manager
            .get_actor_state_from_address(&ts, &address)?;
        // Collect live, active and faulty sectors count from each partition in each deadline.
        let mut live_count = 0;
        let mut active_count = 0;
        let mut faulty_count = 0;
        miner_state.for_each_deadline(policy, ctx.store(), |_dlidx, deadline| {
            deadline.for_each(ctx.store(), |_partidx, partition| {
                live_count += partition.live_sectors().len();
                active_count += partition.active_sectors().len();
                faulty_count += partition.faulty_sectors().len();
                Ok(())
            })
        })?;
        Ok(MinerSectors::new(live_count, active_count, faulty_count))
    }
}

/// Checks if a sector is allocated
pub enum StateMinerSectorAllocated {}

impl RpcMethod<3> for StateMinerSectorAllocated {
    const NAME: &'static str = "Filecoin.StateMinerSectorAllocated";
    const PARAM_NAMES: [&'static str; 3] = ["minerAddress", "sectorNumber", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Checks if a sector number is marked as allocated.");

    type Params = (Address, SectorNumber, ApiTipsetKey);
    type Ok = bool;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (miner_address, sector_number, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let miner_state: miner::State = ctx
            .state_manager
            .get_actor_state_from_address(&ts, &miner_address)?;
        let allocated_sector_numbers: BitField =
            miner_state.load_allocated_sector_numbers(ctx.store())?;
        Ok(allocated_sector_numbers.get(sector_number))
    }
}

/// looks up the miner power of the given address.
pub enum StateMinerPower {}

impl RpcMethod<2> for StateMinerPower {
    const NAME: &'static str = "Filecoin.StateMinerPower";
    const PARAM_NAMES: [&'static str; 2] = ["minerAddress", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns the power of the specified miner.");

    type Params = (Address, ApiTipsetKey);
    type Ok = MinerPower;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        ctx.state_manager
            .miner_power(&address, &ts)
            .map_err(From::from)
    }
}

pub enum StateMinerDeadlines {}

impl RpcMethod<2> for StateMinerDeadlines {
    const NAME: &'static str = "Filecoin.StateMinerDeadlines";
    const PARAM_NAMES: [&'static str; 2] = ["minerAddress", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns all proving deadlines for the given miner.");

    type Params = (Address, ApiTipsetKey);
    type Ok = Vec<ApiDeadline>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let policy = &ctx.chain_config().policy;
        let state: miner::State = ctx
            .state_manager
            .get_actor_state_from_address(&ts, &address)?;
        let mut res = Vec::new();
        state.for_each_deadline(policy, ctx.store(), |_idx, deadline| {
            res.push(ApiDeadline {
                post_submissions: deadline.partitions_posted(),
                disputable_proof_count: deadline.disputable_proof_count(ctx.store())?,
                daily_fee: deadline.daily_fee(),
            });
            Ok(())
        })?;
        Ok(res)
    }
}

pub enum StateMinerProvingDeadline {}

impl RpcMethod<2> for StateMinerProvingDeadline {
    const NAME: &'static str = "Filecoin.StateMinerProvingDeadline";
    const PARAM_NAMES: [&'static str; 2] = ["minerAddress", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Calculates the deadline and related details for a given epoch during a proving period.",
    );

    type Params = (Address, ApiTipsetKey);
    type Ok = ApiDeadlineInfo;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let policy = &ctx.chain_config().policy;
        let state: miner::State = ctx
            .state_manager
            .get_actor_state_from_address(&ts, &address)?;
        Ok(ApiDeadlineInfo(
            state
                .recorded_deadline_info(policy, ts.epoch())
                .next_not_elapsed(),
        ))
    }
}

/// looks up the miner power of the given address.
pub enum StateMinerFaults {}

impl RpcMethod<2> for StateMinerFaults {
    const NAME: &'static str = "Filecoin.StateMinerFaults";
    const PARAM_NAMES: [&'static str; 2] = ["minerAddress", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns a bitfield of the faulty sectors for the given miner.");

    type Params = (Address, ApiTipsetKey);
    type Ok = BitField;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        ctx.state_manager
            .miner_faults(&address, &ts)
            .map_err(From::from)
    }
}

pub enum StateMinerRecoveries {}

impl RpcMethod<2> for StateMinerRecoveries {
    const NAME: &'static str = "Filecoin.StateMinerRecoveries";
    const PARAM_NAMES: [&'static str; 2] = ["minerAddress", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns a bitfield of recovering sectors for the given miner.");

    type Params = (Address, ApiTipsetKey);
    type Ok = BitField;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        ctx.state_manager
            .miner_recoveries(&address, &ts)
            .map_err(From::from)
    }
}

pub enum StateMinerAvailableBalance {}

impl RpcMethod<2> for StateMinerAvailableBalance {
    const NAME: &'static str = "Filecoin.StateMinerAvailableBalance";
    const PARAM_NAMES: [&'static str; 2] = ["minerAddress", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the portion of a miner's balance available for withdrawal or spending.");

    type Params = (Address, ApiTipsetKey);
    type Ok = TokenAmount;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let actor = ctx
            .state_manager
            .get_required_actor(&address, *ts.parent_state())?;
        let state = miner::State::load(ctx.store(), actor.code, actor.state)?;
        let actor_balance: TokenAmount = actor.balance.clone().into();
        let (vested, available): (TokenAmount, TokenAmount) = match &state {
            miner::State::V17(s) => (
                s.check_vested_funds(ctx.store(), ts.epoch())?.into(),
                s.get_available_balance(&actor_balance.into())?.into(),
            ),
            miner::State::V16(s) => (
                s.check_vested_funds(ctx.store(), ts.epoch())?.into(),
                s.get_available_balance(&actor_balance.into())?.into(),
            ),
            miner::State::V15(s) => (
                s.check_vested_funds(ctx.store(), ts.epoch())?.into(),
                s.get_available_balance(&actor_balance.into())?.into(),
            ),
            miner::State::V14(s) => (
                s.check_vested_funds(ctx.store(), ts.epoch())?.into(),
                s.get_available_balance(&actor_balance.into())?.into(),
            ),
            miner::State::V13(s) => (
                s.check_vested_funds(ctx.store(), ts.epoch())?.into(),
                s.get_available_balance(&actor_balance.into())?.into(),
            ),
            miner::State::V12(s) => (
                s.check_vested_funds(ctx.store(), ts.epoch())?.into(),
                s.get_available_balance(&actor_balance.into())?.into(),
            ),
            miner::State::V11(s) => (
                s.check_vested_funds(ctx.store(), ts.epoch())?.into(),
                s.get_available_balance(&actor_balance.into())?.into(),
            ),
            miner::State::V10(s) => (
                s.check_vested_funds(ctx.store(), ts.epoch())?.into(),
                s.get_available_balance(&actor_balance.into())?.into(),
            ),
            miner::State::V9(s) => (
                s.check_vested_funds(ctx.store(), ts.epoch())?.into(),
                s.get_available_balance(&actor_balance.into())?.into(),
            ),
            miner::State::V8(s) => (
                s.check_vested_funds(ctx.store(), ts.epoch())?.into(),
                s.get_available_balance(&actor_balance.into())?.into(),
            ),
        };

        Ok(vested + available)
    }
}

pub enum StateMinerInitialPledgeCollateral {}

impl RpcMethod<3> for StateMinerInitialPledgeCollateral {
    const NAME: &'static str = "Filecoin.StateMinerInitialPledgeCollateral";
    const PARAM_NAMES: [&'static str; 3] = ["minerAddress", "sectorPreCommitInfo", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the initial pledge collateral for the specified miner's sector.");

    type Params = (Address, SectorPreCommitInfo, ApiTipsetKey);
    type Ok = TokenAmount;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, pci, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;

        let sector_size = pci
            .seal_proof
            .sector_size()
            .map_err(|e| anyhow::anyhow!("failed to get resolve size: {e}"))?;

        let market_state: market::State = ctx.state_manager.get_actor_state(&ts)?;
        let (w, vw) = market_state.verify_deals_for_activation(
            ctx.store(),
            address,
            pci.deal_ids,
            ts.epoch(),
            pci.expiration,
        )?;
        let duration = pci.expiration - ts.epoch();
        let sector_weight =
            qa_power_for_weight(SectorSize::from(sector_size).into(), duration, &w, &vw);

        let power_state: power::State = ctx.state_manager.get_actor_state(&ts)?;
        let power_smoothed = power_state.total_power_smoothed();
        let pledge_collateral = power_state.total_locked();

        let reward_state: reward::State = ctx.state_manager.get_actor_state(&ts)?;
        let genesis_info = GenesisInfo::from_chain_config(ctx.chain_config().clone());
        let circ_supply = genesis_info.get_vm_circulating_supply_detailed(
            ts.epoch(),
            &Arc::new(ctx.store()),
            ts.parent_state(),
        )?;
        let initial_pledge = reward_state.initial_pledge_for_power(
            &sector_weight,
            pledge_collateral,
            power_smoothed,
            &circ_supply.fil_circulating,
            power_state.ramp_start_epoch(),
            power_state.ramp_duration_epochs(),
        )?;

        let (q, _) = (initial_pledge * INITIAL_PLEDGE_NUM).div_rem(INITIAL_PLEDGE_DEN);
        Ok(q)
    }
}

pub enum StateMinerPreCommitDepositForPower {}

impl RpcMethod<3> for StateMinerPreCommitDepositForPower {
    const NAME: &'static str = "Filecoin.StateMinerPreCommitDepositForPower";
    const PARAM_NAMES: [&'static str; 3] = ["minerAddress", "sectorPreCommitInfo", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the sector precommit deposit for the specified miner.");

    type Params = (Address, SectorPreCommitInfo, ApiTipsetKey);
    type Ok = TokenAmount;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, pci, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;

        let sector_size = pci
            .seal_proof
            .sector_size()
            .map_err(|e| anyhow::anyhow!("failed to get resolve size: {e}"))?;

        let market_state: market::State = ctx.state_manager.get_actor_state(&ts)?;
        let (w, vw) = market_state.verify_deals_for_activation(
            ctx.store(),
            address,
            pci.deal_ids,
            ts.epoch(),
            pci.expiration,
        )?;
        let duration = pci.expiration - ts.epoch();
        let sector_size = SectorSize::from(sector_size).into();
        let sector_weight =
            if ctx.state_manager.get_network_version(ts.epoch()) < NetworkVersion::V16 {
                qa_power_for_weight(sector_size, duration, &w, &vw)
            } else {
                qa_power_max(sector_size)
            };

        let power_state: power::State = ctx.state_manager.get_actor_state(&ts)?;
        let power_smoothed = power_state.total_power_smoothed();

        let reward_state: reward::State = ctx.state_manager.get_actor_state(&ts)?;
        let deposit: TokenAmount =
            reward_state.pre_commit_deposit_for_power(power_smoothed, sector_weight)?;
        let (value, _) = (deposit * INITIAL_PLEDGE_NUM).div_rem(INITIAL_PLEDGE_DEN);
        Ok(value)
    }
}

/// returns the message receipt for the given message
pub enum StateGetReceipt {}

impl RpcMethod<2> for StateGetReceipt {
    const NAME: &'static str = "Filecoin.StateGetReceipt";
    const PARAM_NAMES: [&'static str; 2] = ["cid", "tipset_key"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::V0); // deprecated in V1
    const PERMISSION: Permission = Permission::Read;

    type Params = (Cid, ApiTipsetKey);
    type Ok = Receipt;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (cid, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let tipset = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        ctx.state_manager
            .get_receipt(tipset, cid)
            .map_err(From::from)
    }
}

/// looks back in the chain for a message. If not found, it blocks until the
/// message arrives on chain, and gets to the indicated confidence depth.
pub enum StateWaitMsgV0 {}

impl RpcMethod<2> for StateWaitMsgV0 {
    const NAME: &'static str = "Filecoin.StateWaitMsg";
    const PARAM_NAMES: [&'static str; 2] = ["messageCid", "confidence"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::V0); // Changed in V1
    const PERMISSION: Permission = Permission::Read;

    type Params = (Cid, i64);
    type Ok = MessageLookup;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (message_cid, confidence): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let (tipset, receipt) = ctx
            .state_manager
            .wait_for_message(message_cid, confidence, None, None)
            .await?;
        let tipset = tipset.context("wait for msg returned empty tuple")?;
        let receipt = receipt.context("wait for msg returned empty receipt")?;
        let ipld = receipt.return_data().deserialize().unwrap_or(Ipld::Null);
        Ok(MessageLookup {
            receipt,
            tipset: tipset.key().clone(),
            height: tipset.epoch(),
            message: message_cid,
            return_dec: ipld,
        })
    }
}

/// looks back in the chain for a message. If not found, it blocks until the
/// message arrives on chain, and gets to the indicated confidence depth.
pub enum StateWaitMsg {}

impl RpcMethod<4> for StateWaitMsg {
    const NAME: &'static str = "Filecoin.StateWaitMsg";
    const PARAM_NAMES: [&'static str; 4] =
        ["messageCid", "confidence", "lookbackLimit", "allowReplaced"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::V1); // Changed in V1
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "StateWaitMsg searches up to limit epochs for a message in the chain. If not found, it blocks until the message appears on-chain and reaches the required confidence depth.",
    );

    type Params = (Cid, i64, ChainEpoch, bool);
    type Ok = MessageLookup;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (message_cid, confidence, look_back_limit, allow_replaced): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let (tipset, receipt) = ctx
            .state_manager
            .wait_for_message(
                message_cid,
                confidence,
                Some(look_back_limit),
                Some(allow_replaced),
            )
            .await?;
        let tipset = tipset.context("wait for msg returned empty tuple")?;
        let receipt = receipt.context("wait for msg returned empty receipt")?;
        let ipld = receipt.return_data().deserialize().unwrap_or(Ipld::Null);
        Ok(MessageLookup {
            receipt,
            tipset: tipset.key().clone(),
            height: tipset.epoch(),
            message: message_cid,
            return_dec: ipld,
        })
    }
}

/// Searches for a message in the chain, and returns its receipt and the tipset where it was executed.
/// See <https://github.com/filecoin-project/lotus/blob/master/documentation/en/api-methods-v1-stable.md#StateSearchMsg>
pub enum StateSearchMsg {}

impl RpcMethod<4> for StateSearchMsg {
    const NAME: &'static str = "Filecoin.StateSearchMsg";
    const PARAM_NAMES: [&'static str; 4] =
        ["tipsetKey", "messageCid", "lookBackLimit", "allowReplaced"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the receipt and tipset the specified message was included in.");

    type Params = (ApiTipsetKey, Cid, i64, bool);
    type Ok = MessageLookup;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (ApiTipsetKey(tsk), message_cid, look_back_limit, allow_replaced): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let from = tsk
            .map(|k| ctx.chain_index().load_required_tipset(&k))
            .transpose()?;
        let (tipset, receipt) = ctx
            .state_manager
            .search_for_message(
                from,
                message_cid,
                Some(look_back_limit),
                Some(allow_replaced),
            )
            .await?
            .with_context(|| format!("message {message_cid} not found."))?;
        let ipld = receipt.return_data().deserialize().unwrap_or(Ipld::Null);
        Ok(MessageLookup {
            receipt,
            tipset: tipset.key().clone(),
            height: tipset.epoch(),
            message: message_cid,
            return_dec: ipld,
        })
    }
}

/// See <https://github.com/filecoin-project/lotus/blob/master/documentation/en/api-methods-v0-deprecated.md#StateSearchMsgLimited>
pub enum StateSearchMsgLimited {}

impl RpcMethod<2> for StateSearchMsgLimited {
    const NAME: &'static str = "Filecoin.StateSearchMsgLimited";
    const PARAM_NAMES: [&'static str; 2] = ["message_cid", "look_back_limit"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::V0); // Not supported in V1
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Looks back up to limit epochs in the chain for a message, and returns its receipt and the tipset where it was executed.",
    );
    type Params = (Cid, i64);
    type Ok = MessageLookup;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (message_cid, look_back_limit): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let (tipset, receipt) = ctx
            .state_manager
            .search_for_message(None, message_cid, Some(look_back_limit), None)
            .await?
            .with_context(|| {
                format!("message {message_cid} not found within the last {look_back_limit} epochs")
            })?;
        let ipld = receipt.return_data().deserialize().unwrap_or(Ipld::Null);
        Ok(MessageLookup {
            receipt,
            tipset: tipset.key().clone(),
            height: tipset.epoch(),
            message: message_cid,
            return_dec: ipld,
        })
    }
}

// Sample CIDs (useful for testing):
//   Mainnet:
//     1,594,681 bafy2bzaceaclaz3jvmbjg3piazaq5dcesoyv26cdpoozlkzdiwnsvdvm2qoqm OhSnap upgrade
//     1_960_320 bafy2bzacec43okhmihmnwmgqspyrkuivqtxv75rpymsdbulq6lgsdq2vkwkcg Skyr upgrade
//     2,833,266 bafy2bzacecaydufxqo5vtouuysmg3tqik6onyuezm6lyviycriohgfnzfslm2
//     2,933,266 bafy2bzacebyp6cmbshtzzuogzk7icf24pt6s5veyq5zkkqbn3sbbvswtptuuu
//   Calibnet:
//     242,150 bafy2bzaceb522vvt3wo7xhleo2dvb7wb7pyydmzlahc4aqd7lmvg3afreejiw
//     630,932 bafy2bzacedidwdsd7ds73t3z76hcjfsaisoxrangkxsqlzih67ulqgtxnypqk
//
/// Traverse an IPLD directed acyclic graph and use libp2p-bitswap to request any missing nodes.
/// This function has two primary uses: (1) Downloading specific state-roots when Forest deviates
/// from the mainline blockchain, (2) fetching historical state-trees to verify past versions of the
/// consensus rules.
pub enum StateFetchRoot {}

impl RpcMethod<2> for StateFetchRoot {
    const NAME: &'static str = "Forest.StateFetchRoot";
    const PARAM_NAMES: [&'static str; 2] = ["root_cid", "save_to_file"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (Cid, Option<PathBuf>);
    type Ok = String;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (root_cid, save_to_file): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let network_send = ctx.network_send().clone();
        let db = ctx.store_owned();

        let (car_tx, car_handle) = if let Some(save_to_file) = save_to_file {
            let (car_tx, car_rx) = flume::bounded(100);
            let roots = nonempty![root_cid];
            let file = tokio::fs::File::create(save_to_file).await?;

            let car_handle = tokio::spawn(async move {
                car_rx
                    .stream()
                    .map(Ok)
                    .forward(CarWriter::new_carv1(roots, file)?)
                    .await
            });

            (Some(car_tx), Some(car_handle))
        } else {
            (None, None)
        };

        const MAX_CONCURRENT_REQUESTS: usize = 64;
        const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

        let mut seen: CidHashSet = CidHashSet::default();
        let mut counter: usize = 0;
        let mut fetched: usize = 0;
        let mut failures: usize = 0;
        let mut task_set = JoinSet::new();

        fn handle_worker(fetched: &mut usize, failures: &mut usize, ret: anyhow::Result<()>) {
            match ret {
                Ok(()) => *fetched += 1,
                Err(msg) => {
                    *failures += 1;
                    tracing::debug!("Request failed: {msg}");
                }
            }
        }

        // When walking an Ipld graph, we're only interested in the DAG_CBOR encoded nodes.
        let mut get_ipld_link = |ipld: &Ipld| match ipld {
            &Ipld::Link(cid) if cid.codec() == DAG_CBOR && seen.insert(cid) => Some(cid),
            _ => None,
        };

        // Do a depth-first-search of the IPLD graph (DAG). Nodes that are _not_ present in our database
        // are fetched in background tasks. If the number of tasks reaches MAX_CONCURRENT_REQUESTS, the
        // depth-first-search pauses until one of the work tasks returns. The memory usage of this
        // algorithm is dominated by the set of seen CIDs and the 'dfs' stack is not expected to grow to
        // more than 1000 elements (even when walking tens of millions of nodes).
        let dfs = Arc::new(Mutex::new(vec![Ipld::Link(root_cid)]));
        let mut to_be_fetched = vec![];

        // Loop until: No more items in `dfs` AND no running worker tasks.
        loop {
            while let Some(ipld) = lock_pop(&dfs) {
                {
                    let mut dfs_guard = dfs.lock();
                    // Scan for unseen CIDs. Available IPLD nodes are pushed to the depth-first-search
                    // stack, unavailable nodes will be requested in worker tasks.
                    for new_cid in ipld.iter().filter_map(&mut get_ipld_link) {
                        counter += 1;
                        if counter.is_multiple_of(1_000) {
                            // set RUST_LOG=forest::rpc::state_api=debug to enable these printouts.
                            tracing::debug!(
                                "Graph walk: CIDs: {counter}, Fetched: {fetched}, Failures: {failures}, dfs: {}, Concurrent: {}",
                                dfs_guard.len(),
                                task_set.len()
                            );
                        }

                        if let Some(next_ipld) = db.get_cbor(&new_cid)? {
                            dfs_guard.push(next_ipld);
                            if let Some(car_tx) = &car_tx {
                                car_tx.send(CarBlock {
                                    cid: new_cid,
                                    data: db.get(&new_cid)?.with_context(|| {
                                        format!("Failed to get cid {new_cid} from block store")
                                    })?,
                                })?;
                            }
                        } else {
                            to_be_fetched.push(new_cid);
                        }
                    }
                }

                while let Some(cid) = to_be_fetched.pop() {
                    if task_set.len() == MAX_CONCURRENT_REQUESTS
                        && let Some(ret) = task_set.join_next().await
                    {
                        handle_worker(&mut fetched, &mut failures, ret?)
                    }
                    task_set.spawn_blocking({
                        let network_send = network_send.clone();
                        let db = db.clone();
                        let dfs_vec = Arc::clone(&dfs);
                        let car_tx = car_tx.clone();
                        move || {
                            let (tx, rx) = flume::bounded(1);
                            network_send.send(NetworkMessage::BitswapRequest {
                                cid,
                                response_channel: tx,
                            })?;
                            // Bitswap requests do not fail. They are just ignored if no-one has
                            // the requested data. Here we arbitrary decide to only wait for
                            // REQUEST_TIMEOUT before judging that the data is unavailable.
                            let _ignore = rx.recv_timeout(REQUEST_TIMEOUT);

                            let new_ipld = db
                                .get_cbor::<Ipld>(&cid)?
                                .with_context(|| format!("Request failed: {cid}"))?;
                            dfs_vec.lock().push(new_ipld);
                            if let Some(car_tx) = &car_tx {
                                car_tx.send(CarBlock {
                                    cid,
                                    data: db.get(&cid)?.with_context(|| {
                                        format!("Failed to get cid {cid} from block store")
                                    })?,
                                })?;
                            }

                            Ok(())
                        }
                    });
                }
                tokio::task::yield_now().await;
            }
            if let Some(ret) = task_set.join_next().await {
                handle_worker(&mut fetched, &mut failures, ret?)
            } else {
                // We are out of work items (dfs) and all worker threads have finished, this means
                // the entire graph has been walked and fetched.
                break;
            }
        }

        drop(car_tx);
        if let Some(car_handle) = car_handle {
            car_handle.await??;
        }

        Ok(format!(
            "IPLD graph traversed! CIDs: {counter}, fetched: {fetched}, failures: {failures}."
        ))
    }
}

pub enum ForestStateCompute {}

impl RpcMethod<2> for ForestStateCompute {
    const NAME: &'static str = "Forest.StateCompute";
    const N_REQUIRED_PARAMS: usize = 1;
    const PARAM_NAMES: [&'static str; 2] = ["epoch", "n_epochs"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (ChainEpoch, Option<NonZeroUsize>);
    type Ok = Vec<ForestComputeStateOutput>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (from_epoch, n_epochs): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let n_epochs = n_epochs.map(|n| n.get()).unwrap_or(1) as ChainEpoch;
        let to_epoch = from_epoch + n_epochs - 1;
        let to_ts = ctx.chain_index().tipset_by_height(
            to_epoch,
            ctx.chain_store().heaviest_tipset(),
            ResolveNullTipset::TakeOlder,
        )?;
        let from_ts = if from_epoch >= to_ts.epoch() {
            // When `from_epoch` is a null epoch or `n_epochs` is 1,
            // `to_ts.epoch()` could be less than or equal to `from_epoch`
            to_ts.clone()
        } else {
            ctx.chain_index().tipset_by_height(
                from_epoch,
                to_ts.clone(),
                ResolveNullTipset::TakeOlder,
            )?
        };

        let mut futures = FuturesOrdered::new();
        for ts in to_ts
            .chain(ctx.store())
            .take_while(|ts| ts.epoch() >= from_ts.epoch())
        {
            let chain_store = ctx.chain_store().clone();
            let network_context = ctx.sync_network_context.clone();
            futures.push_front(async move {
                if crate::chain_sync::load_full_tipset(&chain_store, ts.key()).is_err() {
                    // Backfill full tipset from the network
                    const MAX_RETRIES: usize = 5;
                    let fts = 'retry_loop: {
                        for i in 1..=MAX_RETRIES {
                            match network_context.chain_exchange_messages(None, &ts).await {
                                Ok(fts) => break 'retry_loop Ok(fts),
                                Err(e) if i >= MAX_RETRIES => break 'retry_loop Err(e),
                                Err(_) => continue,
                            }
                        }
                        Err("unreachable chain exchange error in ForestStateCompute".into())
                    }
                    .map_err(|e| {
                        anyhow::anyhow!("failed to download messages@{}: {e}", ts.epoch())
                    })?;
                    fts.persist(chain_store.blockstore())?;
                }
                anyhow::Ok(ts)
            });
        }

        let mut results = Vec::with_capacity(n_epochs as _);
        while let Some(ts) = futures.try_next().await? {
            let epoch = ts.epoch();
            let tipset_key = ts.key().clone();
            let StateOutput { state_root, .. } = ctx
                .state_manager
                .compute_tipset_state(ts, crate::state_manager::NO_CALLBACK, VMTrace::NotTraced)
                .await?;
            results.push(ForestComputeStateOutput {
                state_root,
                epoch,
                tipset_key,
            });
        }
        Ok(results)
    }
}

pub enum StateCompute {}

impl RpcMethod<3> for StateCompute {
    const NAME: &'static str = "Filecoin.StateCompute";
    const PARAM_NAMES: [&'static str; 3] = ["height", "messages", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Applies the given messages on the given tipset");

    type Params = (ChainEpoch, Vec<Message>, ApiTipsetKey);
    type Ok = ComputeStateOutput;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (height, messages, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let (tx, rx) = flume::unbounded();
        let callback = move |ctx: MessageCallbackCtx<'_>| {
            tx.send(ApiInvocResult {
                msg_cid: ctx.message.cid(),
                msg: ctx.message.message().clone(),
                msg_rct: Some(ctx.apply_ret.msg_receipt()),
                error: ctx.apply_ret.failure_info().unwrap_or_default(),
                duration: ctx.duration.as_nanos().clamp(0, u64::MAX as u128) as u64,
                gas_cost: MessageGasCost::new(ctx.message.message(), ctx.apply_ret)?,
                execution_trace: structured::parse_events(ctx.apply_ret.exec_trace())
                    .unwrap_or_default(),
            })?;
            Ok(())
        };
        let StateOutput { state_root, .. } = ctx
            .state_manager
            .compute_state(height, messages, ts, Some(callback), VMTrace::Traced)
            .await?;
        let mut trace = vec![];
        while let Ok(v) = rx.try_recv() {
            trace.push(v);
        }
        Ok(ComputeStateOutput {
            root: state_root,
            trace,
        })
    }
}

// Convenience function for locking and popping a value out of a vector. If this function is
// inlined, the mutex guard isn't dropped early enough.
fn lock_pop<T>(mutex: &Mutex<Vec<T>>) -> Option<T> {
    mutex.lock().pop()
}

/// Get randomness from tickets
pub enum StateGetRandomnessFromTickets {}

impl RpcMethod<4> for StateGetRandomnessFromTickets {
    const NAME: &'static str = "Filecoin.StateGetRandomnessFromTickets";
    const PARAM_NAMES: [&'static str; 4] = ["personalization", "randEpoch", "entropy", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Samples the chain for randomness.");

    type Params = (i64, ChainEpoch, Vec<u8>, ApiTipsetKey);
    type Ok = Vec<u8>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (personalization, rand_epoch, entropy, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let tipset = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let chain_rand = ctx.state_manager.chain_rand(tipset);
        let digest = chain_rand.get_chain_randomness(rand_epoch, false)?;
        let value = crate::state_manager::chain_rand::draw_randomness_from_digest(
            &digest,
            personalization,
            rand_epoch,
            &entropy,
        )?;
        Ok(value.to_vec())
    }
}

pub enum StateGetRandomnessDigestFromTickets {}

impl RpcMethod<2> for StateGetRandomnessDigestFromTickets {
    const NAME: &'static str = "Filecoin.StateGetRandomnessDigestFromTickets";
    const PARAM_NAMES: [&'static str; 2] = ["randEpoch", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Samples the chain for randomness.");

    type Params = (ChainEpoch, ApiTipsetKey);
    type Ok = Vec<u8>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (rand_epoch, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let tipset = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let chain_rand = ctx.state_manager.chain_rand(tipset);
        let digest = chain_rand.get_chain_randomness(rand_epoch, false)?;
        Ok(digest.to_vec())
    }
}

/// Get randomness from beacon
pub enum StateGetRandomnessFromBeacon {}

impl RpcMethod<4> for StateGetRandomnessFromBeacon {
    const NAME: &'static str = "Filecoin.StateGetRandomnessFromBeacon";
    const PARAM_NAMES: [&'static str; 4] = ["personalization", "randEpoch", "entropy", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Returns the beacon entry for the specified Filecoin epoch. If unavailable, the call blocks until it becomes available.",
    );

    type Params = (i64, ChainEpoch, Vec<u8>, ApiTipsetKey);
    type Ok = Vec<u8>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (personalization, rand_epoch, entropy, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let tipset = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let chain_rand = ctx.state_manager.chain_rand(tipset);
        let digest = chain_rand.get_beacon_randomness_v3(rand_epoch)?;
        let value = crate::state_manager::chain_rand::draw_randomness_from_digest(
            &digest,
            personalization,
            rand_epoch,
            &entropy,
        )?;
        Ok(value.to_vec())
    }
}

pub enum StateGetRandomnessDigestFromBeacon {}

impl RpcMethod<2> for StateGetRandomnessDigestFromBeacon {
    const NAME: &'static str = "Filecoin.StateGetRandomnessDigestFromBeacon";
    const PARAM_NAMES: [&'static str; 2] = ["randEpoch", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Samples the beacon for randomness.");

    type Params = (ChainEpoch, ApiTipsetKey);
    type Ok = Vec<u8>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (rand_epoch, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let tipset = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let chain_rand = ctx.state_manager.chain_rand(tipset);
        let digest = chain_rand.get_beacon_randomness_v3(rand_epoch)?;
        Ok(digest.to_vec())
    }
}

/// Get read state
pub enum StateReadState {}

impl RpcMethod<2> for StateReadState {
    const NAME: &'static str = "Filecoin.StateReadState";
    const PARAM_NAMES: [&'static str; 2] = ["address", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns the state of the specified actor.");

    type Params = (Address, ApiTipsetKey);
    type Ok = ApiActorState;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (address, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let actor = ctx
            .state_manager
            .get_required_actor(&address, *ts.parent_state())?;
        let state_json = load_and_serialize_actor_state(ctx.store(), &actor.code, &actor.state)
            .map_err(|e| anyhow::anyhow!("Failed to load actor state: {}", e))?;
        Ok(ApiActorState {
            balance: actor.balance.clone().into(),
            code: actor.code,
            state: state_json,
        })
    }
}

pub enum StateDecodeParams {}
impl RpcMethod<4> for StateDecodeParams {
    const NAME: &'static str = "Filecoin.StateDecodeParams";
    const PARAM_NAMES: [&'static str; 4] = ["address", "method", "params", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Decode the provided method params.");

    type Params = (Address, MethodNum, Vec<u8>, ApiTipsetKey);
    type Ok = serde_json::Value;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (address, method, params, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let actor = ctx
            .state_manager
            .get_required_actor(&address, *ts.parent_state())?;

        let res = crate::rpc::registry::methods_reg::deserialize_params(
            &actor.code,
            method,
            params.as_slice(),
        )?;
        Ok(res.into())
    }
}

pub enum StateCirculatingSupply {}

impl RpcMethod<1> for StateCirculatingSupply {
    const NAME: &'static str = "Filecoin.StateCirculatingSupply";
    const PARAM_NAMES: [&'static str; 1] = ["tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the exact circulating supply of Filecoin at the given tipset.");

    type Params = (ApiTipsetKey,);
    type Ok = TokenAmount;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (ApiTipsetKey(tsk),): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let height = ts.epoch();
        let root = ts.parent_state();
        let genesis_info = GenesisInfo::from_chain_config(ctx.chain_config().clone());
        let supply =
            genesis_info.get_state_circulating_supply(height - 1, &ctx.store_owned(), root)?;
        Ok(supply)
    }
}

pub enum StateVerifiedClientStatus {}

impl RpcMethod<2> for StateVerifiedClientStatus {
    const NAME: &'static str = "Filecoin.StateVerifiedClientStatus";
    const PARAM_NAMES: [&'static str; 2] = ["address", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Returns the data cap for the given address. Returns null if no entry exists in the data cap table.",
    );

    type Params = (Address, ApiTipsetKey);
    type Ok = Option<BigInt>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let status = ctx.state_manager.verified_client_status(&address, &ts)?;
        Ok(status)
    }
}

pub enum StateVMCirculatingSupplyInternal {}

impl RpcMethod<1> for StateVMCirculatingSupplyInternal {
    const NAME: &'static str = "Filecoin.StateVMCirculatingSupplyInternal";
    const PARAM_NAMES: [&'static str; 1] = ["tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns an approximation of Filecoin's circulating supply at the given tipset.");

    type Params = (ApiTipsetKey,);
    type Ok = CirculatingSupply;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (ApiTipsetKey(tsk),): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let genesis_info = GenesisInfo::from_chain_config(ctx.chain_config().clone());
        Ok(genesis_info.get_vm_circulating_supply_detailed(
            ts.epoch(),
            &ctx.store_owned(),
            ts.parent_state(),
        )?)
    }
}

pub enum StateListMiners {}

impl RpcMethod<1> for StateListMiners {
    const NAME: &'static str = "Filecoin.StateListMiners";
    const PARAM_NAMES: [&'static str; 1] = ["tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the addresses of every miner with claimed power in the Power Actor.");

    type Params = (ApiTipsetKey,);
    type Ok = Vec<Address>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (ApiTipsetKey(tsk),): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let state: power::State = ctx.state_manager.get_actor_state(&ts)?;
        let miners = state.list_all_miners(ctx.store())?;
        Ok(miners)
    }
}

pub enum StateListActors {}

impl RpcMethod<1> for StateListActors {
    const NAME: &'static str = "Filecoin.StateListActors";
    const PARAM_NAMES: [&'static str; 1] = ["tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the addresses of every actor in the state.");

    type Params = (ApiTipsetKey,);
    type Ok = Vec<Address>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (ApiTipsetKey(tsk),): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let mut actors = vec![];
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let state_tree = ctx.state_manager.get_state_tree(ts.parent_state())?;
        state_tree.for_each(|addr, _state| {
            actors.push(addr);
            Ok(())
        })?;
        Ok(actors)
    }
}

pub enum StateMarketStorageDeal {}

impl RpcMethod<2> for StateMarketStorageDeal {
    const NAME: &'static str = "Filecoin.StateMarketStorageDeal";
    const PARAM_NAMES: [&'static str; 2] = ["dealId", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns information about the specified deal.");

    type Params = (DealID, ApiTipsetKey);
    type Ok = ApiMarketDeal;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (deal_id, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let store = ctx.store();
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let market_state: market::State = ctx.state_manager.get_actor_state(&ts)?;
        let proposals = market_state.proposals(store)?;
        let proposal = proposals.get(deal_id)?.ok_or_else(|| anyhow::anyhow!("deal {deal_id} not found - deal may not have completed sealing before deal proposal start epoch, or deal may have been slashed"))?;

        let states = market_state.states(store)?;
        let state = states.get(deal_id)?.unwrap_or_else(DealState::empty);

        Ok(MarketDeal { proposal, state }.into())
    }
}

pub enum StateMarketParticipants {}

impl RpcMethod<1> for StateMarketParticipants {
    const NAME: &'static str = "Filecoin.StateMarketParticipants";
    const PARAM_NAMES: [&'static str; 1] = ["tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the Escrow and Locked balances of all participants in the Storage Market.");

    type Params = (ApiTipsetKey,);
    type Ok = HashMap<String, MarketBalance>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (ApiTipsetKey(tsk),): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let market_state = ctx.state_manager.market_state(&ts)?;
        let escrow_table = market_state.escrow_table(ctx.store())?;
        let locked_table = market_state.locked_table(ctx.store())?;
        let mut result = HashMap::new();
        escrow_table.for_each(|address, escrow| {
            let locked = locked_table.get(address)?;
            result.insert(
                address.to_string(),
                MarketBalance {
                    escrow: escrow.clone(),
                    locked,
                },
            );
            Ok(())
        })?;
        Ok(result)
    }
}

pub enum StateDealProviderCollateralBounds {}

impl RpcMethod<3> for StateDealProviderCollateralBounds {
    const NAME: &'static str = "Filecoin.StateDealProviderCollateralBounds";
    const PARAM_NAMES: [&'static str; 3] = ["size", "verified", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Returns the minimum and maximum collateral a storage provider can issue, based on deal size and verified status.",
    );

    type Params = (u64, bool, ApiTipsetKey);
    type Ok = DealCollateralBounds;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (size, verified, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let deal_provider_collateral_num = BigInt::from(110);
        let deal_provider_collateral_denom = BigInt::from(100);

        // This is more eloquent than giving the whole match pattern a type.
        let _: bool = verified;

        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;

        let power_state: power::State = ctx.state_manager.get_actor_state(&ts)?;
        let reward_state: reward::State = ctx.state_manager.get_actor_state(&ts)?;

        let genesis_info = GenesisInfo::from_chain_config(ctx.chain_config().clone());

        let supply = genesis_info.get_vm_circulating_supply(
            ts.epoch(),
            &ctx.store_owned(),
            ts.parent_state(),
        )?;

        let power_claim = power_state.total_power();

        let policy = &ctx.chain_config().policy;

        let baseline_power = reward_state.this_epoch_baseline_power();

        let (min, max) = reward_state.deal_provider_collateral_bounds(
            policy,
            size.into(),
            &power_claim.raw_byte_power,
            baseline_power,
            &supply,
        );

        let min = min
            .atto()
            .mul(deal_provider_collateral_num)
            .div_euclid(&deal_provider_collateral_denom);

        Ok(DealCollateralBounds {
            max,
            min: TokenAmount::from_atto(min),
        })
    }
}

pub enum StateGetBeaconEntry {}

impl RpcMethod<1> for StateGetBeaconEntry {
    const NAME: &'static str = "Filecoin.StateGetBeaconEntry";
    const PARAM_NAMES: [&'static str; 1] = ["epoch"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the beacon entries for the specified epoch.");

    type Params = (ChainEpoch,);
    type Ok = BeaconEntry;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (epoch,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        {
            let genesis_timestamp = ctx.chain_store().genesis_block_header().timestamp as i64;
            let block_delay = ctx.chain_config().block_delay_secs as i64;
            // Give it a 1s clock drift buffer
            let epoch_timestamp = genesis_timestamp + block_delay * epoch + 1;
            let now_timestamp = chrono::Utc::now().timestamp();
            match epoch_timestamp.saturating_sub(now_timestamp) {
                diff if diff > 0 => {
                    tokio::time::sleep(Duration::from_secs(diff as u64)).await;
                }
                _ => {}
            };
        }

        let (_, beacon) = ctx.beacon().beacon_for_epoch(epoch)?;
        let network_version = ctx.state_manager.get_network_version(epoch);
        let round = beacon.max_beacon_round_for_epoch(network_version, epoch);
        let entry = beacon.entry(round).await?;
        Ok(entry)
    }
}

pub enum StateSectorPreCommitInfoV0 {}

impl RpcMethod<3> for StateSectorPreCommitInfoV0 {
    const NAME: &'static str = "Filecoin.StateSectorPreCommitInfo";
    const PARAM_NAMES: [&'static str; 3] = ["minerAddress", "sectorNumber", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::V0); // Changed in V1
    const PERMISSION: Permission = Permission::Read;

    type Params = (Address, u64, ApiTipsetKey);
    type Ok = SectorPreCommitOnChainInfo;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (miner_address, sector_number, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let state: miner::State = ctx
            .state_manager
            .get_actor_state_from_address(&ts, &miner_address)?;
        Ok(state
            .load_precommit_on_chain_info(ctx.store(), sector_number)?
            .context("precommit info does not exist")?)
    }
}

pub enum StateSectorPreCommitInfo {}

impl RpcMethod<3> for StateSectorPreCommitInfo {
    const NAME: &'static str = "Filecoin.StateSectorPreCommitInfo";
    const PARAM_NAMES: [&'static str; 3] = ["minerAddress", "sectorNumber", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::V1); // Changed in V1
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Returns the PreCommit information for the specified miner's sector. Returns null if not precommitted.",
    );

    type Params = (Address, u64, ApiTipsetKey);
    type Ok = Option<SectorPreCommitOnChainInfo>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (miner_address, sector_number, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let state: miner::State = ctx
            .state_manager
            .get_actor_state_from_address(&ts, &miner_address)?;
        Ok(state.load_precommit_on_chain_info(ctx.store(), sector_number)?)
    }
}

impl StateSectorPreCommitInfo {
    pub fn get_sectors(
        store: &Arc<impl Blockstore>,
        miner_address: &Address,
        tipset: &Tipset,
    ) -> anyhow::Result<Vec<u64>> {
        let mut sectors = vec![];
        let state_tree = StateTree::new_from_root(store.clone(), tipset.parent_state())?;
        let state: miner::State = state_tree.get_actor_state_from_address(miner_address)?;
        match &state {
            miner::State::V8(s) => {
                let precommitted = fil_actors_shared::v8::make_map_with_root::<
                    _,
                    fil_actor_miner_state::v8::SectorPreCommitOnChainInfo,
                >(&s.pre_committed_sectors, store)?;
                precommitted
                    .for_each(|_k, v| {
                        sectors.push(v.info.sector_number);
                        Ok(())
                    })
                    .context("failed to iterate over precommitted sectors")
            }
            miner::State::V9(s) => {
                let precommitted = fil_actors_shared::v9::make_map_with_root::<
                    _,
                    fil_actor_miner_state::v9::SectorPreCommitOnChainInfo,
                >(&s.pre_committed_sectors, store)?;
                precommitted
                    .for_each(|_k, v| {
                        sectors.push(v.info.sector_number);
                        Ok(())
                    })
                    .context("failed to iterate over precommitted sectors")
            }
            miner::State::V10(s) => {
                let precommitted = fil_actors_shared::v10::make_map_with_root::<
                    _,
                    fil_actor_miner_state::v10::SectorPreCommitOnChainInfo,
                >(&s.pre_committed_sectors, store)?;
                precommitted
                    .for_each(|_k, v| {
                        sectors.push(v.info.sector_number);
                        Ok(())
                    })
                    .context("failed to iterate over precommitted sectors")
            }
            miner::State::V11(s) => {
                let precommitted = fil_actors_shared::v11::make_map_with_root::<
                    _,
                    fil_actor_miner_state::v11::SectorPreCommitOnChainInfo,
                >(&s.pre_committed_sectors, store)?;
                precommitted
                    .for_each(|_k, v| {
                        sectors.push(v.info.sector_number);
                        Ok(())
                    })
                    .context("failed to iterate over precommitted sectors")
            }
            miner::State::V12(s) => {
                let precommitted = fil_actors_shared::v12::make_map_with_root::<
                    _,
                    fil_actor_miner_state::v12::SectorPreCommitOnChainInfo,
                >(&s.pre_committed_sectors, store)?;
                precommitted
                    .for_each(|_k, v| {
                        sectors.push(v.info.sector_number);
                        Ok(())
                    })
                    .context("failed to iterate over precommitted sectors")
            }
            miner::State::V13(s) => {
                let precommitted = fil_actors_shared::v13::make_map_with_root::<
                    _,
                    fil_actor_miner_state::v13::SectorPreCommitOnChainInfo,
                >(&s.pre_committed_sectors, store)?;
                precommitted
                    .for_each(|_k, v| {
                        sectors.push(v.info.sector_number);
                        Ok(())
                    })
                    .context("failed to iterate over precommitted sectors")
            }
            miner::State::V14(s) => {
                let precommitted = fil_actor_miner_state::v14::PreCommitMap::load(
                    store,
                    &s.pre_committed_sectors,
                    fil_actor_miner_state::v14::PRECOMMIT_CONFIG,
                    "precommits",
                )?;
                precommitted
                    .for_each(|_k, v| {
                        sectors.push(v.info.sector_number);
                        Ok(())
                    })
                    .context("failed to iterate over precommitted sectors")
            }
            miner::State::V15(s) => {
                let precommitted = fil_actor_miner_state::v15::PreCommitMap::load(
                    store,
                    &s.pre_committed_sectors,
                    fil_actor_miner_state::v15::PRECOMMIT_CONFIG,
                    "precommits",
                )?;
                precommitted
                    .for_each(|_k, v| {
                        sectors.push(v.info.sector_number);
                        Ok(())
                    })
                    .context("failed to iterate over precommitted sectors")
            }
            miner::State::V16(s) => {
                let precommitted = fil_actor_miner_state::v16::PreCommitMap::load(
                    store,
                    &s.pre_committed_sectors,
                    fil_actor_miner_state::v16::PRECOMMIT_CONFIG,
                    "precommits",
                )?;
                precommitted
                    .for_each(|_k, v| {
                        sectors.push(v.info.sector_number);
                        Ok(())
                    })
                    .context("failed to iterate over precommitted sectors")
            }
            miner::State::V17(s) => {
                let precommitted = fil_actor_miner_state::v17::PreCommitMap::load(
                    store,
                    &s.pre_committed_sectors,
                    fil_actor_miner_state::v17::PRECOMMIT_CONFIG,
                    "precommits",
                )?;
                precommitted
                    .for_each(|_k, v| {
                        sectors.push(v.info.sector_number);
                        Ok(())
                    })
                    .context("failed to iterate over precommitted sectors")
            }
        }?;

        Ok(sectors)
    }

    pub fn get_sector_pre_commit_infos(
        store: &Arc<impl Blockstore>,
        miner_address: &Address,
        tipset: &Tipset,
    ) -> anyhow::Result<Vec<SectorPreCommitInfo>> {
        let mut infos = vec![];
        let state_tree = StateTree::new_from_root(store.clone(), tipset.parent_state())?;
        let state: miner::State = state_tree.get_actor_state_from_address(miner_address)?;
        match &state {
            miner::State::V8(s) => {
                let precommitted = fil_actors_shared::v8::make_map_with_root::<
                    _,
                    fil_actor_miner_state::v8::SectorPreCommitOnChainInfo,
                >(&s.pre_committed_sectors, store)?;
                precommitted
                    .for_each(|_k, v| {
                        infos.push(v.info.clone().into());
                        Ok(())
                    })
                    .context("failed to iterate over precommitted sectors")
            }
            miner::State::V9(s) => {
                let precommitted = fil_actors_shared::v9::make_map_with_root::<
                    _,
                    fil_actor_miner_state::v9::SectorPreCommitOnChainInfo,
                >(&s.pre_committed_sectors, store)?;
                precommitted
                    .for_each(|_k, v| {
                        infos.push(v.info.clone().into());
                        Ok(())
                    })
                    .context("failed to iterate over precommitted sectors")
            }
            miner::State::V10(s) => {
                let precommitted = fil_actors_shared::v10::make_map_with_root::<
                    _,
                    fil_actor_miner_state::v10::SectorPreCommitOnChainInfo,
                >(&s.pre_committed_sectors, store)?;
                precommitted
                    .for_each(|_k, v| {
                        infos.push(v.info.clone().into());
                        Ok(())
                    })
                    .context("failed to iterate over precommitted sectors")
            }
            miner::State::V11(s) => {
                let precommitted = fil_actors_shared::v11::make_map_with_root::<
                    _,
                    fil_actor_miner_state::v11::SectorPreCommitOnChainInfo,
                >(&s.pre_committed_sectors, store)?;
                precommitted
                    .for_each(|_k, v| {
                        infos.push(v.info.clone().into());
                        Ok(())
                    })
                    .context("failed to iterate over precommitted sectors")
            }
            miner::State::V12(s) => {
                let precommitted = fil_actors_shared::v12::make_map_with_root::<
                    _,
                    fil_actor_miner_state::v12::SectorPreCommitOnChainInfo,
                >(&s.pre_committed_sectors, store)?;
                precommitted
                    .for_each(|_k, v| {
                        infos.push(v.info.clone().into());
                        Ok(())
                    })
                    .context("failed to iterate over precommitted sectors")
            }
            miner::State::V13(s) => {
                let precommitted = fil_actors_shared::v13::make_map_with_root::<
                    _,
                    fil_actor_miner_state::v13::SectorPreCommitOnChainInfo,
                >(&s.pre_committed_sectors, store)?;
                precommitted
                    .for_each(|_k, v| {
                        infos.push(v.info.clone().into());
                        Ok(())
                    })
                    .context("failed to iterate over precommitted sectors")
            }
            miner::State::V14(s) => {
                let precommitted = fil_actor_miner_state::v14::PreCommitMap::load(
                    store,
                    &s.pre_committed_sectors,
                    fil_actor_miner_state::v14::PRECOMMIT_CONFIG,
                    "precommits",
                )?;
                precommitted
                    .for_each(|_k, v| {
                        infos.push(v.info.clone().into());
                        Ok(())
                    })
                    .context("failed to iterate over precommitted sectors")
            }
            miner::State::V15(s) => {
                let precommitted = fil_actor_miner_state::v15::PreCommitMap::load(
                    store,
                    &s.pre_committed_sectors,
                    fil_actor_miner_state::v15::PRECOMMIT_CONFIG,
                    "precommits",
                )?;
                precommitted
                    .for_each(|_k, v| {
                        infos.push(v.info.clone().into());
                        Ok(())
                    })
                    .context("failed to iterate over precommitted sectors")
            }
            miner::State::V16(s) => {
                let precommitted = fil_actor_miner_state::v16::PreCommitMap::load(
                    store,
                    &s.pre_committed_sectors,
                    fil_actor_miner_state::v16::PRECOMMIT_CONFIG,
                    "precommits",
                )?;
                precommitted
                    .for_each(|_k, v| {
                        infos.push(v.info.clone().into());
                        Ok(())
                    })
                    .context("failed to iterate over precommitted sectors")
            }
            miner::State::V17(s) => {
                let precommitted = fil_actor_miner_state::v17::PreCommitMap::load(
                    store,
                    &s.pre_committed_sectors,
                    fil_actor_miner_state::v17::PRECOMMIT_CONFIG,
                    "precommits",
                )?;
                precommitted
                    .for_each(|_k, v| {
                        infos.push(v.info.clone().into());
                        Ok(())
                    })
                    .context("failed to iterate over precommitted sectors")
            }
        }?;

        Ok(infos)
    }
}

pub enum StateSectorGetInfo {}

impl RpcMethod<3> for StateSectorGetInfo {
    const NAME: &'static str = "Filecoin.StateSectorGetInfo";
    const PARAM_NAMES: [&'static str; 3] = ["minerAddress", "sectorNumber", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Returns on-chain information for the specified miner's sector. Returns null if not found. Use StateSectorExpiration for accurate expiration epochs.",
    );

    type Params = (Address, u64, ApiTipsetKey);
    type Ok = Option<SectorOnChainInfo>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (miner_address, sector_number, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        Ok(ctx
            .state_manager
            .get_all_sectors(&miner_address, &ts)?
            .into_iter()
            .find(|info| info.sector_number == sector_number))
    }
}

impl StateSectorGetInfo {
    pub fn get_sectors(
        store: &Arc<impl Blockstore>,
        miner_address: &Address,
        tipset: &Tipset,
    ) -> anyhow::Result<Vec<u64>> {
        let state_tree = StateTree::new_from_root(store.clone(), tipset.parent_state())?;
        let state: miner::State = state_tree.get_actor_state_from_address(miner_address)?;
        Ok(state
            .load_sectors(store, None)?
            .into_iter()
            .map(|s| s.sector_number)
            .collect())
    }
}

pub enum StateSectorExpiration {}

impl RpcMethod<3> for StateSectorExpiration {
    const NAME: &'static str = "Filecoin.StateSectorExpiration";
    const PARAM_NAMES: [&'static str; 3] = ["minerAddress", "sectorNumber", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the epoch at which the specified sector will expire.");

    type Params = (Address, u64, ApiTipsetKey);
    type Ok = SectorExpiration;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (miner_address, sector_number, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let store = ctx.store();
        let policy = &ctx.chain_config().policy;
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let state: miner::State = ctx
            .state_manager
            .get_actor_state_from_address(&ts, &miner_address)?;
        let mut early = 0;
        let mut on_time = 0;
        let mut terminated = false;
        state.for_each_deadline(policy, store, |_deadline_index, deadline| {
            deadline.for_each(store, |_partition_index, partition| {
                if !terminated && partition.all_sectors().get(sector_number) {
                    if partition.terminated().get(sector_number) {
                        terminated = true;
                        early = 0;
                        on_time = 0;
                        return Ok(());
                    }
                    let expirations: Amt<fil_actor_miner_state::v16::ExpirationSet, _> =
                        Amt::load(&partition.expirations_epochs(), store)?;
                    expirations.for_each(|epoch, expiration| {
                        if expiration.early_sectors.get(sector_number) {
                            early = epoch as _;
                        }
                        if expiration.on_time_sectors.get(sector_number) {
                            on_time = epoch as _;
                        }
                        Ok(())
                    })?;
                }

                Ok(())
            })?;
            Ok(())
        })?;
        if early == 0 && on_time == 0 {
            Err(anyhow::anyhow!("failed to find sector {sector_number}").into())
        } else {
            Ok(SectorExpiration { early, on_time })
        }
    }
}

pub enum StateSectorPartition {}

impl RpcMethod<3> for StateSectorPartition {
    const NAME: &'static str = "Filecoin.StateSectorPartition";
    const PARAM_NAMES: [&'static str; 3] = ["minerAddress", "sectorNumber", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Finds the deadline/partition for the specified sector.");

    type Params = (Address, u64, ApiTipsetKey);
    type Ok = SectorLocation;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (miner_address, sector_number, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let state: miner::State = ctx
            .state_manager
            .get_actor_state_from_address(&ts, &miner_address)?;
        let (deadline, partition) =
            state.find_sector(ctx.store(), sector_number, &ctx.chain_config().policy)?;
        Ok(SectorLocation {
            deadline,
            partition,
        })
    }
}

/// Looks back and returns all messages with a matching to or from address, stopping at the given height.
pub enum StateListMessages {}

impl RpcMethod<3> for StateListMessages {
    const NAME: &'static str = "Filecoin.StateListMessages";
    const PARAM_NAMES: [&'static str; 3] = ["messageFilter", "tipsetKey", "maxHeight"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns all messages with a matching to or from address up to the given height.");

    type Params = (MessageFilter, ApiTipsetKey, i64);
    type Ok = Vec<Cid>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (from_to, ApiTipsetKey(tsk), max_height): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        if from_to.is_empty() {
            return Err(ErrorObject::owned(
                1,
                "must specify at least To or From in message filter",
                Some(from_to),
            )
            .into());
        } else if let Some(to) = from_to.to {
            // this is following lotus logic, it probably should be `if let` instead of `else if let`
            // see <https://github.com/ChainSafe/forest/pull/3827#discussion_r1462691005>
            if ctx.state_manager.lookup_id(&to, &ts)?.is_none() {
                return Ok(vec![]);
            }
        } else if let Some(from) = from_to.from
            && ctx.state_manager.lookup_id(&from, &ts)?.is_none()
        {
            return Ok(vec![]);
        }

        let mut out = Vec::new();
        let mut cur_ts = ts.clone();

        while cur_ts.epoch() >= max_height {
            let msgs = ctx.chain_store().messages_for_tipset(&cur_ts)?;

            for msg in msgs {
                if from_to.matches(msg.message()) {
                    out.push(msg.cid());
                }
            }

            if cur_ts.epoch() == 0 {
                break;
            }

            let next = ctx.chain_index().load_required_tipset(cur_ts.parents())?;
            cur_ts = next;
        }

        Ok(out)
    }
}

pub enum StateGetClaim {}

impl RpcMethod<3> for StateGetClaim {
    const NAME: &'static str = "Filecoin.StateGetClaim";
    const PARAM_NAMES: [&'static str; 3] = ["address", "claimId", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the claim for a given address and claim ID.");

    type Params = (Address, ClaimID, ApiTipsetKey);
    type Ok = Option<Claim>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, claim_id, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        Ok(ctx.state_manager.get_claim(&address, &ts, claim_id)?)
    }
}

pub enum StateGetClaims {}

impl RpcMethod<2> for StateGetClaims {
    const NAME: &'static str = "Filecoin.StateGetClaims";
    const PARAM_NAMES: [&'static str; 2] = ["address", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns all claims for a given provider.");

    type Params = (Address, ApiTipsetKey);
    type Ok = HashMap<ClaimID, Claim>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        Ok(Self::get_claims(&ctx.store_owned(), &address, &ts)?)
    }
}

impl StateGetClaims {
    pub fn get_claims(
        store: &Arc<impl Blockstore>,
        address: &Address,
        tipset: &Tipset,
    ) -> anyhow::Result<HashMap<ClaimID, Claim>> {
        let state_tree = StateTree::new_from_tipset(store.clone(), tipset)?;
        let state: verifreg::State = state_tree.get_actor_state()?;
        let actor_id = state_tree.lookup_required_id(address)?;
        let actor_id_address = Address::new_id(actor_id);
        state.get_claims(store, &actor_id_address)
    }
}

pub enum StateGetAllClaims {}

impl RpcMethod<1> for StateGetAllClaims {
    const NAME: &'static str = "Filecoin.StateGetAllClaims";
    const PARAM_NAMES: [&'static str; 1] = ["tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns all claims available in the verified registry actor.");

    type Params = (ApiTipsetKey,);
    type Ok = HashMap<ClaimID, Claim>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (ApiTipsetKey(tsk),): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        Ok(ctx.state_manager.get_all_claims(&ts)?)
    }
}

pub enum StateGetAllocation {}

impl RpcMethod<3> for StateGetAllocation {
    const NAME: &'static str = "Filecoin.StateGetAllocation";
    const PARAM_NAMES: [&'static str; 3] = ["address", "allocationId", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the allocation for a given address and allocation ID.");

    type Params = (Address, AllocationID, ApiTipsetKey);
    type Ok = Option<Allocation>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, allocation_id, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        Ok(ctx
            .state_manager
            .get_allocation(&address, &ts, allocation_id)?)
    }
}

pub enum StateGetAllocations {}

impl RpcMethod<2> for StateGetAllocations {
    const NAME: &'static str = "Filecoin.StateGetAllocations";
    const PARAM_NAMES: [&'static str; 2] = ["address", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns all allocations for a given client.");

    type Params = (Address, ApiTipsetKey);
    type Ok = HashMap<AllocationID, Allocation>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        Ok(Self::get_allocations(&ctx.store_owned(), &address, &ts)?)
    }
}

impl StateGetAllocations {
    // For testing
    pub fn get_valid_actor_addresses<'a>(
        store: &'a Arc<impl Blockstore>,
        tipset: &'a Tipset,
    ) -> anyhow::Result<impl Iterator<Item = Address> + 'a> {
        let mut addresses = HashSet::default();
        let state_tree = StateTree::new_from_tipset(store.clone(), tipset)?;
        let verifreg_state: verifreg::State = state_tree.get_actor_state()?;
        match verifreg_state {
            verifreg::State::V13(s) => {
                let map = s.load_allocs(store)?;
                map.for_each(|k, _| {
                    let actor_id = fil_actors_shared::v13::parse_uint_key(k)?;
                    addresses.insert(Address::new_id(actor_id));
                    Ok(())
                })?;
            }
            verifreg::State::V12(s) => {
                let map = s.load_allocs(store)?;
                map.for_each(|k, _| {
                    let actor_id = fil_actors_shared::v12::parse_uint_key(k)?;
                    addresses.insert(Address::new_id(actor_id));
                    Ok(())
                })?;
            }
            _ => (),
        };

        if addresses.is_empty() {
            let init_state: init::State = state_tree.get_actor_state()?;
            match init_state {
                init::State::V0(_) => {
                    anyhow::bail!("StateGetAllocations is not implemented for init state v0");
                }
                init::State::V8(s) => {
                    let map =
                        fil_actors_shared::v8::make_map_with_root::<_, u64>(&s.address_map, store)?;
                    map.for_each(|_k, v| {
                        addresses.insert(Address::new_id(*v));
                        Ok(())
                    })?;
                }
                init::State::V9(s) => {
                    let map =
                        fil_actors_shared::v9::make_map_with_root::<_, u64>(&s.address_map, store)?;
                    map.for_each(|_k, v| {
                        addresses.insert(Address::new_id(*v));
                        Ok(())
                    })?;
                }
                init::State::V10(s) => {
                    let map = fil_actors_shared::v10::make_map_with_root::<_, u64>(
                        &s.address_map,
                        store,
                    )?;
                    map.for_each(|_k, v| {
                        addresses.insert(Address::new_id(*v));
                        Ok(())
                    })?;
                }
                init::State::V11(s) => {
                    let map = fil_actors_shared::v11::make_map_with_root::<_, u64>(
                        &s.address_map,
                        store,
                    )?;
                    map.for_each(|_k, v| {
                        addresses.insert(Address::new_id(*v));
                        Ok(())
                    })?;
                }
                init::State::V12(s) => {
                    let map = fil_actors_shared::v12::make_map_with_root::<_, u64>(
                        &s.address_map,
                        store,
                    )?;
                    map.for_each(|_k, v| {
                        addresses.insert(Address::new_id(*v));
                        Ok(())
                    })?;
                }
                init::State::V13(s) => {
                    let map = fil_actors_shared::v13::make_map_with_root::<_, u64>(
                        &s.address_map,
                        store,
                    )?;
                    map.for_each(|_k, v| {
                        addresses.insert(Address::new_id(*v));
                        Ok(())
                    })?;
                }
                init::State::V14(s) => {
                    let map = fil_actor_init_state::v14::AddressMap::load(
                        store,
                        &s.address_map,
                        fil_actors_shared::v14::DEFAULT_HAMT_CONFIG,
                        "address_map",
                    )?;
                    map.for_each(|_k, v| {
                        addresses.insert(Address::new_id(*v));
                        Ok(())
                    })?;
                }
                init::State::V15(s) => {
                    let map = fil_actor_init_state::v15::AddressMap::load(
                        store,
                        &s.address_map,
                        fil_actors_shared::v15::DEFAULT_HAMT_CONFIG,
                        "address_map",
                    )?;
                    map.for_each(|_k, v| {
                        addresses.insert(Address::new_id(*v));
                        Ok(())
                    })?;
                }
                init::State::V16(s) => {
                    let map = fil_actor_init_state::v16::AddressMap::load(
                        store,
                        &s.address_map,
                        fil_actors_shared::v16::DEFAULT_HAMT_CONFIG,
                        "address_map",
                    )?;
                    map.for_each(|_k, v| {
                        addresses.insert(Address::new_id(*v));
                        Ok(())
                    })?;
                }
                init::State::V17(s) => {
                    let map = fil_actor_init_state::v17::AddressMap::load(
                        store,
                        &s.address_map,
                        fil_actors_shared::v17::DEFAULT_HAMT_CONFIG,
                        "address_map",
                    )?;
                    map.for_each(|_k, v| {
                        addresses.insert(Address::new_id(*v));
                        Ok(())
                    })?;
                }
            };
        }

        Ok(addresses
            .into_iter()
            .filter(|addr| match Self::get_allocations(store, addr, tipset) {
                Ok(r) => !r.is_empty(),
                _ => false,
            }))
    }

    pub fn get_allocations(
        store: &Arc<impl Blockstore>,
        address: &Address,
        tipset: &Tipset,
    ) -> anyhow::Result<HashMap<AllocationID, Allocation>> {
        let state_tree = StateTree::new_from_tipset(store.clone(), tipset)?;
        let state: verifreg::State = state_tree.get_actor_state()?;
        state.get_allocations(store, address)
    }
}

pub enum StateGetAllAllocations {}

impl RpcMethod<1> for crate::rpc::prelude::StateGetAllAllocations {
    const NAME: &'static str = "Filecoin.StateGetAllAllocations";
    const PARAM_NAMES: [&'static str; 1] = ["tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns all allocations available in the verified registry actor.");

    type Params = (ApiTipsetKey,);
    type Ok = HashMap<AllocationID, Allocation>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (ApiTipsetKey(tsk),): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        Ok(ctx.state_manager.get_all_allocations(&ts)?)
    }
}

pub enum StateGetAllocationIdForPendingDeal {}

impl RpcMethod<2> for StateGetAllocationIdForPendingDeal {
    const NAME: &'static str = "Filecoin.StateGetAllocationIdForPendingDeal";
    const PARAM_NAMES: [&'static str; 2] = ["dealId", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the allocation ID for the specified pending deal.");

    type Params = (DealID, ApiTipsetKey);
    type Ok = AllocationID;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (deal_id, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let state_tree = StateTree::new_from_tipset(ctx.store_owned(), &ts)?;
        let market_state: market::State = state_tree.get_actor_state()?;
        Ok(market_state.get_allocation_id_for_pending_deal(ctx.store(), &deal_id)?)
    }
}

impl StateGetAllocationIdForPendingDeal {
    pub fn get_allocations_for_pending_deals(
        store: &Arc<impl Blockstore>,
        tipset: &Tipset,
    ) -> anyhow::Result<HashMap<DealID, AllocationID>> {
        let state_tree = StateTree::new_from_tipset(store.clone(), tipset)?;
        let state: market::State = state_tree.get_actor_state()?;
        state.get_allocations_for_pending_deals(store)
    }
}

pub enum StateGetAllocationForPendingDeal {}

impl RpcMethod<2> for StateGetAllocationForPendingDeal {
    const NAME: &'static str = "Filecoin.StateGetAllocationForPendingDeal";
    const PARAM_NAMES: [&'static str; 2] = ["dealId", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Returns the allocation for the specified pending deal. Returns null if no pending allocation is found.",
    );

    type Params = (DealID, ApiTipsetKey);
    type Ok = Option<Allocation>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (deal_id, tsk): Self::Params,
        ext: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let allocation_id =
            StateGetAllocationIdForPendingDeal::handle(ctx.clone(), (deal_id, tsk.clone()), ext)
                .await?;
        if allocation_id == fil_actor_market_state::v14::NO_ALLOCATION_ID {
            return Ok(None);
        }
        let deal = StateMarketStorageDeal::handle(ctx.clone(), (deal_id, tsk.clone()), ext).await?;
        StateGetAllocation::handle(ctx.clone(), (deal.proposal.client, allocation_id, tsk), ext)
            .await
    }
}

pub enum StateGetNetworkParams {}

impl RpcMethod<0> for StateGetNetworkParams {
    const NAME: &'static str = "Filecoin.StateGetNetworkParams";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns current network parameters.");

    type Params = ();
    type Ok = NetworkParams;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let config = ctx.chain_config().as_ref();
        let heaviest_tipset = ctx.chain_store().heaviest_tipset();
        let network_name = ctx
            .state_manager
            .get_network_state_name(*heaviest_tipset.parent_state())?
            .into();
        let policy = &config.policy;

        let params = NetworkParams {
            network_name,
            block_delay_secs: config.block_delay_secs as u64,
            consensus_miner_min_power: policy.minimum_consensus_power.clone(),
            pre_commit_challenge_delay: policy.pre_commit_challenge_delay,
            fork_upgrade_params: ForkUpgradeParams::try_from(config)
                .context("Failed to get fork upgrade params")?,
            eip155_chain_id: config.eth_chain_id,
        };

        Ok(params)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct NetworkParams {
    network_name: String,
    block_delay_secs: u64,
    #[schemars(with = "crate::lotus_json::LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    consensus_miner_min_power: BigInt,
    pre_commit_challenge_delay: ChainEpoch,
    fork_upgrade_params: ForkUpgradeParams,
    #[serde(rename = "Eip155ChainID")]
    eip155_chain_id: EthChainId,
}

lotus_json_with_self!(NetworkParams);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct ForkUpgradeParams {
    upgrade_smoke_height: ChainEpoch,
    upgrade_breeze_height: ChainEpoch,
    upgrade_ignition_height: ChainEpoch,
    upgrade_liftoff_height: ChainEpoch,
    upgrade_assembly_height: ChainEpoch,
    upgrade_refuel_height: ChainEpoch,
    upgrade_tape_height: ChainEpoch,
    upgrade_kumquat_height: ChainEpoch,
    breeze_gas_tamping_duration: ChainEpoch,
    upgrade_calico_height: ChainEpoch,
    upgrade_persian_height: ChainEpoch,
    upgrade_orange_height: ChainEpoch,
    upgrade_claus_height: ChainEpoch,
    upgrade_trust_height: ChainEpoch,
    upgrade_norwegian_height: ChainEpoch,
    upgrade_turbo_height: ChainEpoch,
    upgrade_hyperdrive_height: ChainEpoch,
    upgrade_chocolate_height: ChainEpoch,
    upgrade_oh_snap_height: ChainEpoch,
    upgrade_skyr_height: ChainEpoch,
    upgrade_shark_height: ChainEpoch,
    upgrade_hygge_height: ChainEpoch,
    upgrade_lightning_height: ChainEpoch,
    upgrade_thunder_height: ChainEpoch,
    upgrade_watermelon_height: ChainEpoch,
    upgrade_dragon_height: ChainEpoch,
    upgrade_phoenix_height: ChainEpoch,
    upgrade_waffle_height: ChainEpoch,
    upgrade_tuktuk_height: ChainEpoch,
    upgrade_teep_height: ChainEpoch,
    upgrade_tock_height: ChainEpoch,
    //upgrade_golden_week_height: ChainEpoch,
}

impl TryFrom<&ChainConfig> for ForkUpgradeParams {
    type Error = anyhow::Error;
    fn try_from(config: &ChainConfig) -> anyhow::Result<Self> {
        let height_infos = &config.height_infos;
        let get_height = |height| -> anyhow::Result<ChainEpoch> {
            let height = height_infos
                .get(&height)
                .context(format!("Height info for {height} not found"))?
                .epoch;
            Ok(height)
        };

        use crate::networks::Height::*;
        Ok(ForkUpgradeParams {
            upgrade_smoke_height: get_height(Smoke)?,
            upgrade_breeze_height: get_height(Breeze)?,
            upgrade_ignition_height: get_height(Ignition)?,
            upgrade_liftoff_height: get_height(Liftoff)?,
            upgrade_assembly_height: get_height(Assembly)?,
            upgrade_refuel_height: get_height(Refuel)?,
            upgrade_tape_height: get_height(Tape)?,
            upgrade_kumquat_height: get_height(Kumquat)?,
            breeze_gas_tamping_duration: config.breeze_gas_tamping_duration,
            upgrade_calico_height: get_height(Calico)?,
            upgrade_persian_height: get_height(Persian)?,
            upgrade_orange_height: get_height(Orange)?,
            upgrade_claus_height: get_height(Claus)?,
            upgrade_trust_height: get_height(Trust)?,
            upgrade_norwegian_height: get_height(Norwegian)?,
            upgrade_turbo_height: get_height(Turbo)?,
            upgrade_hyperdrive_height: get_height(Hyperdrive)?,
            upgrade_chocolate_height: get_height(Chocolate)?,
            upgrade_oh_snap_height: get_height(OhSnap)?,
            upgrade_skyr_height: get_height(Skyr)?,
            upgrade_shark_height: get_height(Shark)?,
            upgrade_hygge_height: get_height(Hygge)?,
            upgrade_lightning_height: get_height(Lightning)?,
            upgrade_thunder_height: get_height(Thunder)?,
            upgrade_watermelon_height: get_height(Watermelon)?,
            upgrade_dragon_height: get_height(Dragon)?,
            upgrade_phoenix_height: get_height(Phoenix)?,
            upgrade_waffle_height: get_height(Waffle)?,
            upgrade_tuktuk_height: get_height(TukTuk)?,
            upgrade_teep_height: get_height(Teep)?,
            upgrade_tock_height: get_height(Tock)?,
            //upgrade_golden_week_height: get_height(GoldenWeek)?,
        })
    }
}

pub enum StateMinerInitialPledgeForSector {}
impl RpcMethod<4> for StateMinerInitialPledgeForSector {
    const NAME: &'static str = "Filecoin.StateMinerInitialPledgeForSector";
    const PARAM_NAMES: [&'static str; 4] = [
        "sector_duration",
        "sector_size",
        "verified_size",
        "tipset_key",
    ];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (ChainEpoch, SectorSize, u64, ApiTipsetKey);
    type Ok = TokenAmount;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (sector_duration, sector_size, verified_size, ApiTipsetKey(tsk)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        if sector_duration <= 0 {
            return Err(anyhow::anyhow!("sector duration must be greater than 0").into());
        }
        if verified_size > sector_size as u64 {
            return Err(
                anyhow::anyhow!("verified deal size cannot be larger than sector size").into(),
            );
        }

        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;

        let power_state: power::State = ctx.state_manager.get_actor_state(&ts)?;
        let power_smoothed = power_state.total_power_smoothed();
        let pledge_collateral = power_state.total_locked();

        let reward_state: reward::State = ctx.state_manager.get_actor_state(&ts)?;

        let genesis_info = GenesisInfo::from_chain_config(ctx.chain_config().clone());
        let circ_supply = genesis_info.get_vm_circulating_supply_detailed(
            ts.epoch(),
            &ctx.store_owned(),
            ts.parent_state(),
        )?;

        let deal_weight = BigInt::from(0);
        let verified_deal_weight = BigInt::from(verified_size) * sector_duration;
        let sector_weight = qa_power_for_weight(
            sector_size.into(),
            sector_duration,
            &deal_weight,
            &verified_deal_weight,
        );

        let (epochs_since_start, duration) = get_pledge_ramp_params(&ctx, ts.epoch(), &ts)?;

        let initial_pledge = reward_state.initial_pledge_for_power(
            &sector_weight,
            pledge_collateral,
            power_smoothed,
            &circ_supply.fil_circulating,
            epochs_since_start,
            duration,
        )?;

        let (value, _) = (initial_pledge * INITIAL_PLEDGE_NUM).div_rem(INITIAL_PLEDGE_DEN);
        Ok(value)
    }
}

fn get_pledge_ramp_params(
    ctx: &Ctx<impl Blockstore + Send + Sync + 'static>,
    height: ChainEpoch,
    ts: &Tipset,
) -> Result<(ChainEpoch, u64), anyhow::Error> {
    let state_tree = ctx.state_manager.get_state_tree(ts.parent_state())?;

    let power_state: power::State = state_tree
        .get_actor_state()
        .context("loading power actor state")?;

    if power_state.ramp_start_epoch() > 0 {
        Ok((
            height - power_state.ramp_start_epoch(),
            power_state.ramp_duration_epochs(),
        ))
    } else {
        Ok((0, 0))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct StateActorCodeCidsOutput {
    pub network_version: NetworkVersion,
    pub network_version_revision: i64,
    pub actor_version: String,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Cid>")]
    pub manifest: Cid,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Cid>")]
    pub bundle: Cid,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<HashMap<String, Cid>>")]
    pub actor_cids: HashMap<String, Cid>,
}
lotus_json_with_self!(StateActorCodeCidsOutput);

impl std::fmt::Display for StateActorCodeCidsOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Network Version: {}", self.network_version)?;
        writeln!(
            f,
            "Network Version Revision: {}",
            self.network_version_revision
        )?;
        writeln!(f, "Actor Version: {}", self.actor_version)?;
        writeln!(f, "Manifest CID: {}", self.manifest)?;
        writeln!(f, "Bundle CID: {}", self.bundle)?;
        writeln!(f, "Actor CIDs:")?;
        let longest_name = self
            .actor_cids
            .keys()
            .map(|name| name.len())
            .max()
            .unwrap_or(0);
        for (name, cid) in &self.actor_cids {
            writeln!(f, "  {:width$} : {}", name, cid, width = longest_name)?;
        }
        Ok(())
    }
}

pub enum StateActorInfo {}

impl RpcMethod<0> for StateActorInfo {
    const NAME: &'static str = "Forest.StateActorInfo";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the builtin actor information for the current network.");

    type Params = ();
    type Ok = StateActorCodeCidsOutput;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(None)?;
        let state_tree = StateTree::new_from_tipset(ctx.store_owned(), &ts)?;
        let bundle = state_tree.get_actor_bundle_metadata()?;
        let system_state: system::State = state_tree.get_actor_state()?;
        let actors = system_state.builtin_actors_cid();

        let current_manifest = BuiltinActorManifest::load_v1_actor_list(ctx.store(), actors)?;

        // Sanity check: the command would normally be used only for diagnostics, so we want to be
        // sure the data is consistent.
        if current_manifest != bundle.manifest {
            return Err(anyhow::anyhow!("Actor bundle manifest does not match the manifest in the state tree. This indicates that the node is misconfigured or is running an unsupported network.")
            .into());
        }

        let network_version = ctx.chain_config().network_version(ts.epoch() - 1);
        let network_version_revision = ctx.chain_config().network_version_revision(ts.epoch() - 1);
        let result = StateActorCodeCidsOutput {
            network_version,
            network_version_revision,
            actor_version: bundle.version.to_owned(),
            manifest: current_manifest.actor_list_cid,
            bundle: bundle.bundle_cid,
            actor_cids: current_manifest
                .builtin_actors()
                .map(|(a, c)| (a.name().to_string(), c))
                .collect(),
        };

        Ok(result)
    }
}
