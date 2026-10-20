// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::TipsetKey;
use crate::lotus_json::{LotusJson, lotus_json_with_self};
use crate::message::MessageRead as _;
use crate::shim::{
    address::Address,
    clock::ChainEpoch,
    econ::TokenAmount,
    error::ExitCode,
    executor::{ApplyRet, Receipt},
    fvm_latest::trace::IpldOperation,
    message::Message,
    state_tree::{ActorID, ActorState},
};
use crate::utils::get_size::raw_bytes_heap_size_helper;
use cid::Cid;
use fvm_ipld_encoding::RawBytes;
use get_size2::GetSize;
use num::Zero as _;
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ComputeStateOutput {
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub root: Cid,
    #[schemars(with = "LotusJson<ApiInvocResult>")]
    #[serde(with = "crate::lotus_json")]
    pub trace: Vec<ApiInvocResult>,
}

lotus_json_with_self!(ComputeStateOutput);

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ForestComputeStateOutput {
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub state_root: Cid,
    pub epoch: ChainEpoch,
    #[schemars(with = "LotusJson<TipsetKey>")]
    #[serde(with = "crate::lotus_json")]
    pub tipset_key: TipsetKey,
}

lotus_json_with_self!(ForestComputeStateOutput);

#[derive(Debug, Default, Serialize, Deserialize, Clone, JsonSchema, GetSize)]
#[serde(rename_all = "PascalCase")]
pub struct ApiInvocResult {
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Cid>")]
    #[get_size(ignore)]
    pub msg_cid: Cid,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Message>")]
    pub msg: Message,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Option<Receipt>>")]
    pub msg_rct: Option<Receipt>,
    pub error: String,
    pub duration: u64,
    pub gas_cost: MessageGasCost,
    pub execution_trace: Option<ExecutionTrace>,
}

lotus_json_with_self!(ApiInvocResult);

impl PartialEq for ApiInvocResult {
    /// Ignore [`Self::duration`] as it is implementation-dependent
    fn eq(&self, other: &Self) -> bool {
        self.msg == other.msg
            && self.msg_cid == other.msg_cid
            && self.msg_rct == other.msg_rct
            && self.error == other.error
            && self.gas_cost == other.gas_cost
            && self.execution_trace == other.execution_trace
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, JsonSchema, GetSize)]
#[serde(rename_all = "PascalCase")]
pub struct MessageGasCost {
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Option<Cid>>")]
    #[get_size(ignore)]
    pub message: Option<Cid>,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub gas_used: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub base_fee_burn: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub over_estimation_burn: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub miner_penalty: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub miner_tip: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub refund: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub total_cost: TokenAmount,
}

lotus_json_with_self!(MessageGasCost);

impl MessageGasCost {
    fn is_zero_cost(&self) -> bool {
        self.base_fee_burn.is_zero()
            && self.over_estimation_burn.is_zero()
            && self.miner_penalty.is_zero()
            && self.miner_tip.is_zero()
            && self.refund.is_zero()
            && self.total_cost.is_zero()
    }

    pub fn new(message: &Message, apply_ret: &ApplyRet) -> anyhow::Result<Self> {
        let mut cost = Self {
            message: None,
            gas_used: TokenAmount::zero(),
            base_fee_burn: apply_ret.base_fee_burn(),
            over_estimation_burn: apply_ret.over_estimation_burn(),
            miner_penalty: apply_ret.penalty(),
            miner_tip: apply_ret.miner_tip(),
            refund: apply_ret.refund(),
            total_cost: message.required_funds() - &apply_ret.refund(),
        };
        if !cost.is_zero_cost() {
            cost.message = Some(message.cid());
            cost.gas_used = TokenAmount::from_atto(apply_ret.gas_used());
        }
        Ok(cost)
    }
}

/// IPLD operation kind for [`TraceIpld`].
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    SerializeDisplay,
    DeserializeFromStr,
    strum::Display,
    strum::EnumString,
    GetSize,
)]
#[strum(serialize_all = "PascalCase")]
pub enum TraceIpldOp {
    Get,
    Put,
    #[strum(to_string = "Unknown", default)]
    Unknown(String),
}

impl JsonSchema for TraceIpldOp {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TraceIpldOp".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({
            "type": "string",
            "enum": ["Get", "Put", "Unknown"],
        })
    }
}

impl From<IpldOperation> for TraceIpldOp {
    fn from(op: IpldOperation) -> Self {
        match op {
            IpldOperation::Get => Self::Get,
            IpldOperation::Put => Self::Put,
        }
    }
}

/// IPLD operation details attached to an [`ExecutionTrace`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, GetSize)]
#[serde(rename_all = "PascalCase")]
pub struct TraceIpld {
    pub op: TraceIpldOp,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Cid>")]
    #[get_size(ignore)]
    pub cid: Cid,
    pub size: u64,
}

lotus_json_with_self!(TraceIpld);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, GetSize)]
#[serde(rename_all = "PascalCase")]
pub struct ExecutionTrace {
    pub msg: MessageTrace,
    pub msg_rct: ReturnTrace,
    pub invoked_actor: Option<ActorTrace>,
    pub gas_charges: Vec<GasTrace>,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Vec<ExecutionTrace>>")]
    pub subcalls: Vec<ExecutionTrace>,
    /// FVM invocation logs (not EVM actor / `eth_getLogs` event logs).
    // See <https://github.com/filecoin-project/lotus/blob/a0ecb8687f1c60d5e66040b6de364dbc9cc4d253/chain/types/execresult.go#L115>
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub logs: Vec<String>,
    // See <https://github.com/filecoin-project/lotus/blob/a0ecb8687f1c60d5e66040b6de364dbc9cc4d253/chain/types/execresult.go#L116>
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ipld_ops: Vec<TraceIpld>,
}

impl ExecutionTrace {
    pub fn sum_gas(&self) -> GasTrace {
        let mut out: GasTrace = GasTrace::default();
        for gc in self.gas_charges.iter() {
            out.total_gas += gc.total_gas;
            out.compute_gas += gc.compute_gas;
            out.storage_gas += gc.storage_gas;
        }
        out
    }
}

lotus_json_with_self!(ExecutionTrace);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, GetSize)]
#[serde(rename_all = "PascalCase")]
pub struct MessageTrace {
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Address>")]
    pub from: Address,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Address>")]
    pub to: Address,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub value: TokenAmount,
    pub method: u64,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<RawBytes>")]
    #[get_size(size_fn = raw_bytes_heap_size_helper)]
    pub params: RawBytes,
    pub params_codec: u64,
    pub gas_limit: Option<u64>,
    pub read_only: Option<bool>,
}

lotus_json_with_self!(MessageTrace);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, GetSize)]
#[serde(rename_all = "PascalCase")]
pub struct ActorTrace {
    pub id: ActorID,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<ActorState>")]
    pub state: ActorState,
}

lotus_json_with_self!(ActorTrace);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, GetSize)]
#[serde(rename_all = "PascalCase")]
pub struct ReturnTrace {
    #[get_size(ignore)]
    pub exit_code: ExitCode,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<RawBytes>")]
    #[get_size(size_fn = raw_bytes_heap_size_helper)]
    pub r#return: RawBytes,
    pub return_codec: u64,
}

lotus_json_with_self!(ReturnTrace);

#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, GetSize)]
#[serde(rename_all = "PascalCase")]
pub struct GasTrace {
    pub name: String,
    #[serde(rename = "tg")]
    pub total_gas: u64,
    #[serde(rename = "cg")]
    pub compute_gas: u64,
    #[serde(rename = "sg")]
    pub storage_gas: u64,
    #[serde(rename = "tt")]
    pub time_taken: u64,
}

lotus_json_with_self!(GasTrace);

impl PartialEq for GasTrace {
    /// Ignore [`Self::time_taken`] as it is implementation-dependent
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.total_gas == other.total_gas
            && self.compute_gas == other.compute_gas
            && self.storage_gas == other.storage_gas
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct SectorExpiration {
    pub on_time: ChainEpoch,
    pub early: ChainEpoch,
}
lotus_json_with_self!(SectorExpiration);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct SectorLocation {
    pub deadline: u64,
    pub partition: u64,
}
lotus_json_with_self!(SectorLocation);

/// Block rewards allocated while executing one tipset.
///
/// All token amounts are in attoFIL.
// See <https://github.com/filecoin-project/lotus/blob/v1.37.0-rc2/api/v2api/types.go#L12-L100>
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, GetSize)]
#[serde(rename_all = "PascalCase")]
pub struct RewardDistribution {
    #[serde(rename = "TipSetKey", with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TipsetKey>")]
    pub tipset_key: TipsetKey,
    pub height: ChainEpoch,
    /// Common denominator of stream weights and recipient shares.
    #[serde(with = "crate::lotus_json::stringify")]
    #[schemars(with = "String")]
    pub denom: u64,
    /// Sums of the corresponding `Amounts` fields across `Blocks`.
    pub totals: RewardAmounts,
    /// One entry per block, in execution order.
    pub blocks: Vec<BlockReward>,
}

lotus_json_with_self!(RewardDistribution);

/// Reward award of one block and the distribution it used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, GetSize)]
#[serde(rename_all = "PascalCase")]
pub struct BlockReward {
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Cid>")]
    #[get_size(ignore)]
    pub block: Cid,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Address>")]
    pub miner: Address,
    pub win_count: i64,
    pub amounts: RewardAmounts,
    /// Fraction assigned to no stream. `BurnAllocation` also includes integer rounding and the
    /// burn shares of explicit streams.
    #[serde(with = "crate::lotus_json::stringify")]
    #[schemars(with = "String")]
    pub burn_weight: u64,
    pub streams: Vec<StreamReward>,
}

/// Reward allocations and transfers of one block, or their sums over a tipset.
///
/// `MintedReward = MinerReward + ExplicitReward + BurnAllocation`.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, JsonSchema, GetSize)]
#[serde(rename_all = "PascalCase")]
pub struct RewardAmounts {
    /// Block subsidy released from the reward actor's reserve. Zero for a block with a positive
    /// `WinCount` means the block received only its gas reward, the reward actor's fallback when
    /// it cannot award normally.
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub minted_reward: TokenAmount,
    /// Minted reward allocated to the implicit miner stream.
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub miner_reward: TokenAmount,
    /// Reward funded by message fees and allocated to the miner.
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub message_reward: TokenAmount,
    /// Minted reward retained for explicit stream recipients.
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub explicit_reward: TokenAmount,
    /// Minted reward allocated to burn by the distribution.
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub burn_allocation: TokenAmount,
    /// Amount successfully transferred to miner actors, including message rewards.
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub miner_paid: TokenAmount,
    /// Amount successfully transferred to the burnt funds actor by reward awards, including
    /// earlier rounding dust settled during the award and a miner payment redirected to burn on
    /// failure. Excludes burns from separate messages and nested transfers from miner actors.
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub burn_paid: TokenAmount,
}

/// Fraction and allocation of one reward stream in one block's award.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, GetSize)]
#[serde(rename_all = "PascalCase")]
pub struct StreamReward {
    #[serde(rename = "ID", with = "crate::lotus_json::stringify")]
    #[schemars(with = "String")]
    pub id: u64,
    #[serde(with = "crate::lotus_json::stringify")]
    #[schemars(with = "String")]
    pub weight: u64,
    /// Gross minted reward allocated to this stream.
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub amount: TokenAmount,
    /// `null` for the implicit stream, whose recipient is the block's miner.
    pub distribution: Option<ExplicitRewardDistribution>,
}

/// Recipients of an explicit stream's award.
///
/// The stream's `Amount = sum(Recipients.EarnedAmount) + BurnAmount + RoundingAdjustment`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, GetSize)]
#[serde(rename_all = "PascalCase")]
pub struct ExplicitRewardDistribution {
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Address>")]
    pub writer: Address,
    pub recipients: Vec<RecipientReward>,
    /// Fraction of the stream assigned to no recipient.
    #[serde(with = "crate::lotus_json::stringify")]
    #[schemars(with = "String")]
    pub burn_share: u64,
    /// Stream allocation burned for this award.
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub burn_amount: TokenAmount,
    /// Reconciles the stream allocation with recipient earnings and burn. Negative when earlier
    /// rounding dust becomes earned, because earnings are differences of rounded cumulative
    /// entitlements. Not an additional payment or burn.
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub rounding_adjustment: TokenAmount,
}

/// Share and earnings of one recipient from a stream award.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, GetSize)]
#[serde(rename_all = "PascalCase")]
pub struct RecipientReward {
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Address>")]
    pub recipient: Address,
    #[serde(with = "crate::lotus_json::stringify")]
    #[schemars(with = "String")]
    pub share: u64,
    /// Increase of the recipient's entitlement from this award. Excludes earnings from earlier
    /// awards and does not subtract later claims.
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub earned_amount: TokenAmount,
}

/// Asserts the sums that hold for every award.
///
/// Minted = miner + explicit + burn. Weights + burn weight = 100%. For an explicit stream,
/// shares + burn share = 100% and portion = earned + burned + rounding adjustment.
#[cfg(test)]
pub fn assert_award_conserved(streams: &[StreamReward], burn_weight: u64, amounts: &RewardAmounts) {
    use fil_actor_reward_state::v19::DENOM;

    assert_eq!(
        amounts.minted_reward,
        &amounts.miner_reward + &amounts.explicit_reward + &amounts.burn_allocation
    );
    let weights: u64 = streams.iter().map(|stream| stream.weight).sum();
    assert_eq!(weights + burn_weight, DENOM);

    for stream in streams {
        let Some(distribution) = &stream.distribution else {
            continue;
        };
        let mut shares = 0;
        let mut earned = TokenAmount::zero();
        for recipient in &distribution.recipients {
            shares += recipient.share;
            earned += recipient.earned_amount.clone();
        }
        assert_eq!(shares + distribution.burn_share, DENOM);
        assert_eq!(
            stream.amount,
            earned + &distribution.burn_amount + &distribution.rounding_adjustment
        );
    }
}
