// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::blocks::{CachingBlockHeader, Chain4U, HeaderBuilder, RawBlockHeader};
use crate::chain::ChainStore;
use crate::networks::{ACTOR_BUNDLES_METADATA, ActorBundleMetadata, Height, NetworkChain};
use crate::rpc::test_utils::chain_store_with_config;
use crate::rpc::{DbImpl, RPCState};
use crate::shim::machine::BuiltinActor;
use crate::shim::sector::RegisteredSealProofV4;
use crate::shim::state_tree::{ActorState, StateTree, StateTreeVersion};
use crate::state_manager::circulating_supply::GenesisInfo;
use crate::utils::db::CborStoreExt as _;
use fil_actors_shared::v18::builtin::reward::smooth::FilterEstimate;
use num_traits::Zero as _;
use quickcheck_macros::quickcheck;
use rstest::rstest;

#[rstest]
#[case(1_000, Some(600))]
#[case(401, Some(1))]
#[case(400, None)]
#[case(399, None)]
#[case(i64::MIN, None)]
fn sector_duration_from_expiration_requires_positive(
    #[case] expiration: ChainEpoch,
    #[case] expected: Option<ChainEpoch>,
) {
    assert_eq!(
        sector_duration_from_expiration(expiration, 400).ok(),
        expected
    );
}

#[quickcheck]
fn sector_duration_from_expiration_no_panic(expiration: ChainEpoch, epoch: ChainEpoch) {
    let _ = sector_duration_from_expiration(expiration, epoch);
}

/// Below calibnet's Shark, so the supply calculation takes its pre-nv23 branch and reads the
/// market actor; above Assembly, so the reserve term is live.
const FIXTURE_EPOCH: ChainEpoch = 10_000;

fn qa_sector_power() -> StoragePower {
    StoragePower::from(1u64 << 36)
}

fn fixture_bundle() -> &'static ActorBundleMetadata {
    ACTOR_BUNDLES_METADATA
        .values()
        .find(|bundle| {
            bundle.network == NetworkChain::Calibnet
                && bundle.actor_major_version().ok() == Some(18)
        })
        .expect("v18 actor bundle is embedded")
}

/// Named after the arguments of the upstream FIP-0081 vectors, so a case there transcribes here
/// one for one: <https://github.com/filecoin-project/builtin-actors/blob/v18.0.0/actors/miner/tests/fip0081_initial_pledge.rs>
///
/// [`compute_initial_pledge_for_power`] derives the ramp arguments from these fields and the tipset
/// epoch, so an upstream `epochs_since_ramp_start` becomes a ramp start that far before the epoch
/// under test, and a `ramp_start_epoch` of zero is a ramp that never started.
struct PledgeInputs {
    baseline_power: StoragePower,
    reward_smoothed: FilterEstimate,
    network_qa_power_smoothed: FilterEstimate,
    ramp_start_epoch: ChainEpoch,
    ramp_duration_epochs: u64,
}

impl Default for PledgeInputs {
    fn default() -> Self {
        Self {
            baseline_power: StoragePower::from(1u64 << 37),
            reward_smoothed: FilterEstimate::new(0.into(), 0.into()),
            network_qa_power_smoothed: FilterEstimate::new(
                (1u64 << 10).into(),
                (1u64 << 10).into(),
            ),
            ramp_start_epoch: 0,
            ramp_duration_epochs: 0,
        }
    }
}

impl PledgeInputs {
    /// Mined rewards and burnt funds are solved so the realised circulating supply is exactly
    /// [`fixture_circulating_supply`], which the vectors take as an input. `omit` drops one actor,
    /// for the failure paths.
    fn write_state_tree(
        &self,
        chain_store: &ChainStore,
        epoch: ChainEpoch,
        omit: Option<Address>,
    ) -> Cid {
        let store = chain_store.db();
        let config = chain_store.chain_config();
        let manifest = &fixture_bundle().manifest;

        let power_state = power::State::V18(fil_actor_power_state::v18::State {
            this_epoch_qa_power_smoothed: self.network_qa_power_smoothed.clone(),
            ramp_start_epoch: self.ramp_start_epoch,
            ramp_duration_epochs: self.ramp_duration_epochs,
            ..Default::default()
        });
        let reward_cid = |mined: &TokenAmount| {
            let state = reward::State::V18(fil_actor_reward_state::v18::State {
                this_epoch_reward_smoothed: self.reward_smoothed.clone(),
                this_epoch_baseline_power: self.baseline_power.clone(),
                epoch,
                total_storage_power_reward: mined.into(),
                ..Default::default()
            });
            store.put_cbor_default(&state).unwrap()
        };
        // Read by the circulating-supply calculation below network version 23.
        let market_state = market::State::V18(
            fil_actor_market_state::v18::State::new(store).expect("empty market state"),
        );

        let put = |tree: &mut StateTree<DbImpl>, address: &Address, code, state, balance| {
            if omit.as_ref() != Some(address) {
                tree.set_actor(address, ActorState::new(code, state, balance, 0, None))
                    .unwrap()
            }
        };
        let builtin = |tree: &mut _, actor, address: &Address, state| {
            put(
                tree,
                address,
                manifest.get(actor).unwrap(),
                state,
                TokenAmount::zero(),
            )
        };
        // Only the balance of a funded actor is read, so its code and state may be anything.
        let funded = |tree: &mut _, address: &Address, balance| {
            put(tree, address, Cid::default(), Cid::default(), balance)
        };

        let mut tree = StateTree::new(store, StateTreeVersion::V5).unwrap();
        let power_cid = store.put_cbor_default(&power_state).unwrap();
        let market_cid = store.put_cbor_default(&market_state).unwrap();
        builtin(
            &mut tree,
            BuiltinActor::Power,
            &Address::POWER_ACTOR,
            power_cid,
        );
        builtin(
            &mut tree,
            BuiltinActor::Market,
            &Address::MARKET_ACTOR,
            market_cid,
        );
        builtin(
            &mut tree,
            BuiltinActor::Reward,
            &Address::REWARD_ACTOR,
            reward_cid(&TokenAmount::zero()),
        );
        funded(
            &mut tree,
            &Address::RESERVE_ACTOR,
            config.initial_fil_reserved_at_height(epoch).clone(),
        );
        funded(&mut tree, &Address::BURNT_FUNDS_ACTOR, TokenAmount::zero());

        // Solving needs every actor the supply calculation reads.
        if omit.is_some() {
            return tree.flush().unwrap();
        }

        // Solved with the same function the handler reads, so the supply path is fixed here, not
        // verified; it is covered differentially against Lotus by the API compare suite.
        let genesis_info = GenesisInfo::from_chain_config(config.clone());
        let supply = |tree: &StateTree<DbImpl>| {
            genesis_info
                .get_vm_circulating_supply_detailed_with_state_tree(epoch, tree)
                .unwrap()
                .fil_circulating
        };
        let target = TokenAmount::from_whole(1);
        // Vesting lands wherever the network's schedule puts it; close the gap from either side.
        let gap = supply(&tree) - &target;
        let excess = gap.clone().max(TokenAmount::zero());
        let shortfall = (-gap).max(TokenAmount::zero());
        funded(&mut tree, &Address::BURNT_FUNDS_ACTOR, excess);
        builtin(
            &mut tree,
            BuiltinActor::Reward,
            &Address::REWARD_ACTOR,
            reward_cid(&shortfall),
        );
        assert_eq!(
            supply(&tree),
            target,
            "fixture failed to reach the requested circulating supply"
        );

        tree.flush().unwrap()
    }
}

/// Must be called from a Tokio context: the message pool spawns background tasks.
fn build_ctx(
    config: ChainConfig,
    epoch: ChainEpoch,
    inputs: &PledgeInputs,
    omit: Option<Address>,
) -> (Ctx, Tipset) {
    let chain_store = chain_store_with_config(config);
    let state_root = inputs.write_state_tree(&chain_store, epoch, omit);
    let blocks = Chain4U::with_blockstore(chain_store.db_owned());
    let mut header = HeaderBuilder::new();
    header.with_epoch(epoch).with_state_root(state_root);
    blocks.insert(&[], "head", header);
    let head = blocks.tipset(&["head"]);
    chain_store.set_heaviest_tipset(head.clone()).unwrap();
    let (ctx, _network_rx) = RPCState::for_tests(chain_store).unwrap();
    (ctx, head)
}

fn ctx_at(config: ChainConfig, epoch: ChainEpoch, inputs: &PledgeInputs) -> (Ctx, Tipset) {
    build_ctx(config, epoch, inputs, None)
}

fn pledge_for(inputs: &PledgeInputs) -> Result<TokenAmount, ServerError> {
    let (ctx, ts) = ctx_at(ChainConfig::calibnet(), FIXTURE_EPOCH, inputs);
    compute_initial_pledge_for_power(&ctx, &ts, &qa_sector_power())
}

/// Upgrades take effect the epoch *after* the configured height.
fn first_epoch_of(config: &ChainConfig, height: Height) -> ChainEpoch {
    config.epoch(height) + 1
}

/// Puts nv27 at [`FIXTURE_EPOCH`] so the boundary is exercised independently of any shipped height.
/// Mirrors what Lotus does in `itests/migration_test.go`.
fn goldenweek_at_fixture_epoch() -> ChainConfig {
    let mut config = ChainConfig::calibnet();
    config
        .height_infos
        .get_mut(&Height::GoldenWeek)
        .expect("calibnet schedules GoldenWeek")
        .epoch = FIXTURE_EPOCH;
    // `ChainConfig::network_height` scans in map order, so the schedule must stay sorted by epoch.
    config
        .height_infos
        .sort_by(|_, a, _, b| a.epoch.cmp(&b.epoch));
    config
}

async fn creation_deposit(config: ChainConfig, epoch: ChainEpoch) -> TokenAmount {
    let (ctx, _) = ctx_at(config, epoch, &Default::default());
    StateMinerCreationDeposit::handle(ctx, (ApiTipsetKey(None),), &Default::default())
        .await
        .unwrap()
}

/// The vectors from `builtin-actors/actors/miner/tests/fip0081_initial_pledge.rs`, reached through
/// Forest's plumbing rather than by calling the actor function directly. Each expected amount is
/// the FIP-0081 share of supply plus one atto, the reward term clamped at a zero reward estimate.
#[rstest]
#[case::before_ramp_start(-100, 100, TokenAmount::from_micro(150_000))]
#[case::ramp_edge_before(-1, 10, TokenAmount::from_micro(150_000))]
#[case::no_ramp(0, 0, TokenAmount::from_micro(195_000))]
#[case::no_ramp_after_start(10, 0, TokenAmount::from_micro(195_000))]
#[case::at_ramp_start(0, 100, TokenAmount::from_micro(150_000))]
#[case::ramp_edge_at_start(0, 10, TokenAmount::from_micro(150_000))]
#[case::ramp_first_step(1, 10, TokenAmount::from_micro(154_500))]
#[case::ramp_early(10, 100, TokenAmount::from_micro(154_500))]
#[case::ramp_mid(50, 100, TokenAmount::from_micro(172_500))]
#[case::after_ramp(150, 100, TokenAmount::from_micro(195_000))]
#[case::long_after_ramp(500, 100, TokenAmount::from_micro(195_000))]
// A ramp start of zero is dormant, so the duration must not be consulted.
#[case::dormant_ramp(FIXTURE_EPOCH, 100_000, TokenAmount::from_micro(195_000))]
#[tokio::test]
async fn initial_pledge_matches_upstream_fip0081_vectors(
    #[case] epochs_since_ramp_start: ChainEpoch,
    #[case] ramp_duration_epochs: u64,
    #[case] expected_share: TokenAmount,
) {
    let inputs = PledgeInputs {
        ramp_start_epoch: FIXTURE_EPOCH - epochs_since_ramp_start,
        ramp_duration_epochs,
        ..Default::default()
    };

    assert_eq!(
        pledge_for(&inputs).unwrap(),
        TokenAmount::from_atto(1) + expected_share
    );
}

/// The upstream vectors keep this estimate below the sector's own power, where it cannot affect
/// the result. Push it above, so dropping it changes the pledge.
#[tokio::test]
async fn network_power_estimate_is_wired_through() {
    let inputs = PledgeInputs {
        network_qa_power_smoothed: FilterEstimate::new((1u64 << 40).into(), 0.into()),
        ..Default::default()
    };

    // Both FIP-0081 denominators are now the network estimate, so the two halves of the convex
    // combination collapse into `circulating_supply * 3/10 * qa_power / network_power`.
    assert_eq!(
        pledge_for(&inputs).unwrap(),
        TokenAmount::from_atto(1) + TokenAmount::from_micro(18_750)
    );
}

/// The upstream vectors zero this estimate, leaving the reward term clamped whether or not it is
/// wired up. Rising twice also shows the pledge is not merely pinned at the per-byte cap.
#[tokio::test]
async fn reward_estimate_is_wired_through() {
    let with_reward = |reward: u64| {
        pledge_for(&PledgeInputs {
            reward_smoothed: FilterEstimate::new(reward.into(), 0.into()),
            ..Default::default()
        })
        .unwrap()
    };

    assert!(with_reward(1_000) > with_reward(0));
    assert!(with_reward(2_000) > with_reward(1_000));
}

#[rstest]
#[case::mainnet(ChainConfig::mainnet())]
#[case::calibnet(ChainConfig::calibnet())]
#[case::synthetic(goldenweek_at_fixture_epoch())]
#[tokio::test]
async fn creation_deposit_is_positive_from_goldenweek(#[case] config: ChainConfig) {
    let activation = first_epoch_of(&config, Height::GoldenWeek);

    assert!(creation_deposit(config, activation).await.is_positive());
}

/// Butterflynet is absent because its genesis is already past the `goldenweek`: it has no epoch to be inactive at.
#[rstest]
#[case::mainnet(ChainConfig::mainnet())]
#[case::calibnet(ChainConfig::calibnet())]
#[case::synthetic(goldenweek_at_fixture_epoch())]
#[tokio::test]
async fn creation_deposit_is_zero_before_goldenweek(#[case] config: ChainConfig) {
    let activation = first_epoch_of(&config, Height::GoldenWeek);

    assert!(creation_deposit(config, activation - 1).await.is_zero());
}

#[rstest]
#[case::mainnet(ChainConfig::mainnet())]
#[case::calibnet(ChainConfig::calibnet())]
#[tokio::test]
async fn creation_deposit_is_the_pledge_for_a_tenth_of_minimum_consensus_power(
    #[case] config: ChainConfig,
) {
    let epoch = first_epoch_of(&config, Height::GoldenWeek);
    let inputs = PledgeInputs {
        // Above the baseline the pledge flattens to a fixed share of supply, so this must clear
        // mainnet's consensus minimum (10 TiB) for the amount to still track the power asked for.
        baseline_power: StoragePower::from(1u128 << 70),
        ..Default::default()
    };
    let (ctx, ts) = ctx_at(config, epoch, &inputs);
    let minimum = ctx.chain_config().policy.minimum_consensus_power.clone();

    let deposit =
        StateMinerCreationDeposit::handle(ctx.clone(), (ApiTipsetKey(None),), &Default::default())
            .await
            .unwrap();

    // Pinned against the upstream vectors above, so as a reference it asserts only which power the
    // handler asks for.
    assert_eq!(
        deposit,
        compute_initial_pledge_for_power(&ctx, &ts, &(&minimum / 10)).unwrap()
    );
    // Guards the assertion above: past the baseline the pledge flattens to a fixed share of supply
    // and stops tracking power, which would let it hold for any divisor.
    assert_ne!(
        deposit,
        compute_initial_pledge_for_power(&ctx, &ts, &minimum).unwrap(),
        "fixture no longer discriminates between powers"
    );
}

#[tokio::test]
async fn initial_pledge_for_sector_adds_the_buffer() {
    let (ctx, ts) = ctx_at(ChainConfig::calibnet(), FIXTURE_EPOCH, &Default::default());
    let duration = 1_000;
    let sector_size = SectorSize::_32GiB;

    let pledge = StateMinerInitialPledgeForSector::handle(
        ctx.clone(),
        (duration, sector_size, 0, ApiTipsetKey(None)),
        &Default::default(),
    )
    .await
    .unwrap();

    let sector_weight = qa_power_for_weight(
        sector_size.into(),
        duration,
        &BigInt::from(0),
        &BigInt::from(0),
    );
    let unbuffered = compute_initial_pledge_for_power(&ctx, &ts, &sector_weight).unwrap();
    // Restated from Lotus `node/impl/full/state.go` so the assertion is independent of the
    // constants under test.
    assert_eq!(pledge, (unbuffered * 110u64).div_floor(100u64));
}

/// The collateral RPC reaches the same pledge through a pre-commit carrying no deals.
#[tokio::test]
async fn initial_pledge_collateral_matches_the_sector_pledge() {
    let (ctx, _) = ctx_at(ChainConfig::calibnet(), FIXTURE_EPOCH, &Default::default());
    let duration = 1_000;
    // Default seal proof is `Invalid`, so that field has to be set; the rest go unread.
    let pre_commit = SectorPreCommitInfo::from(fil_actor_miner_state::v18::SectorPreCommitInfo {
        seal_proof: RegisteredSealProofV4::StackedDRG32GiBV1P1,
        expiration: FIXTURE_EPOCH + duration,
        ..Default::default()
    });

    assert_eq!(
        StateMinerInitialPledgeCollateral::handle(
            ctx.clone(),
            (Address::new_id(1000), pre_commit, ApiTipsetKey(None)),
            &Default::default(),
        )
        .await
        .unwrap(),
        StateMinerInitialPledgeForSector::handle(
            ctx,
            (duration, SectorSize::_32GiB, 0, ApiTipsetKey(None)),
            &Default::default(),
        )
        .await
        .unwrap()
    );
}

/// Puts nv29 at [`FIXTURE_EPOCH`], because no network schedules Solstice yet.
fn solstice_at_fixture_epoch(mut config: ChainConfig) -> ChainConfig {
    config
        .height_infos
        .get_mut(&Height::Solstice)
        .expect("every network lists Solstice")
        .epoch = FIXTURE_EPOCH;
    config
}

async fn initial_pledge_collateral(
    config: ChainConfig,
    epoch: ChainEpoch,
) -> Result<TokenAmount, ServerError> {
    let (ctx, _) = ctx_at(config, epoch, &Default::default());
    // Default seal proof is `Invalid`, so that field has to be set; the rest go unread.
    let pre_commit = SectorPreCommitInfo::from(fil_actor_miner_state::v18::SectorPreCommitInfo {
        seal_proof: RegisteredSealProofV4::StackedDRG32GiBV1P1,
        expiration: epoch + 1_000,
        ..Default::default()
    });
    StateMinerInitialPledgeCollateral::handle(
        ctx,
        (Address::new_id(1000), pre_commit, ApiTipsetKey(None)),
        &Default::default(),
    )
    .await
}

/// From NV29 a pre-commit no longer describes a pledge, so the collateral RPC refuses.
#[rstest]
#[case::mainnet(ChainConfig::mainnet())]
#[case::calibnet(ChainConfig::calibnet())]
#[case::butterflynet(ChainConfig::butterflynet())]
#[case::devnet(ChainConfig::devnet())]
#[tokio::test]
async fn initial_pledge_collateral_is_retired_from_nv29(#[case] config: ChainConfig) {
    let config = solstice_at_fixture_epoch(config);
    let activation = first_epoch_of(&config, Height::Solstice);

    let error = initial_pledge_collateral(config, activation)
        .await
        .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("unsupported from network version 29"),
        "{error}"
    );
}

#[rstest]
#[case::mainnet(ChainConfig::mainnet())]
#[case::calibnet(ChainConfig::calibnet())]
#[case::butterflynet(ChainConfig::butterflynet())]
#[case::devnet(ChainConfig::devnet())]
#[tokio::test]
async fn initial_pledge_collateral_answers_until_nv29(#[case] config: ChainConfig) {
    let config = solstice_at_fixture_epoch(config);
    let activation = first_epoch_of(&config, Height::Solstice);

    let pledge = initial_pledge_collateral(config, activation - 1)
        .await
        .unwrap();

    assert!(pledge.is_positive());
}

#[rstest]
#[case::no_power_actor(Address::POWER_ACTOR)]
#[case::no_reward_actor(Address::REWARD_ACTOR)]
#[case::no_market_actor(Address::MARKET_ACTOR)]
#[case::no_reserve_actor(Address::RESERVE_ACTOR)]
#[case::no_burnt_funds_actor(Address::BURNT_FUNDS_ACTOR)]
#[tokio::test]
async fn initial_pledge_fails_without(#[case] missing: Address) {
    let (ctx, ts) = build_ctx(
        ChainConfig::calibnet(),
        FIXTURE_EPOCH,
        &Default::default(),
        Some(missing),
    );

    assert!(compute_initial_pledge_for_power(&ctx, &ts, &qa_sector_power()).is_err());
}

#[tokio::test]
async fn initial_pledge_fails_on_an_unknown_state_root() {
    let (ctx, _) = ctx_at(ChainConfig::calibnet(), FIXTURE_EPOCH, &Default::default());
    let unknown = Tipset::from(CachingBlockHeader::new(RawBlockHeader {
        state_root: Cid::default(),
        epoch: FIXTURE_EPOCH,
        ..Default::default()
    }));

    assert!(compute_initial_pledge_for_power(&ctx, &unknown, &qa_sector_power()).is_err());
}
