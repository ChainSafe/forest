// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::error::ServerError;
use crate::rpc::mpool::MpoolPushMessage;
use crate::rpc::{ApiPaths, Ctx, Permission, RpcMethod};
use crate::shim::{address::Address, message::Message, message::MethodNum};
use cid::Cid;
use enumflags2::BitFlags;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::RawBytes;
use num_bigint::BigInt;

const METHOD_ADD_BALANCE: MethodNum = 2;

pub enum MarketAddBalance {}
impl RpcMethod<3> for MarketAddBalance {
    const NAME: &'static str = "Filecoin.MarketAddBalance";
    const PARAM_NAMES: [&'static str; 3] = ["wallet", "address", "amount"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Sign;

    type Params = (Address, Address, BigInt);
    type Ok = Cid;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (wallet, address, amount): Self::Params,
        ext: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let bytes = fvm_ipld_encoding::to_vec(&address)?;
        let params = RawBytes::new(bytes);

        let message = Message {
            to: Address::MARKET_ACTOR,
            from: wallet,
            value: amount.into(),
            method_num: METHOD_ADD_BALANCE,
            params,
            ..Default::default()
        };

        let smsg = MpoolPushMessage::handle(ctx, (message, None), ext).await?;
        Ok(smsg.cid())
    }
}
