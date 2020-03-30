// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use vm::{TokenAmount, MethodNum, PieceInfo, RegisteredProof, SealVerifyInfo, PoStVerifyInfo};
trait Pricelist {
    fn on_chain_message(msg_size: i64) -> i64;
    fn on_chain_return_value (data_size: i64) -> i64;

    fn on_method_invocation(value: TokenAmount, method_num: MethodNum) -> i64;

    fn on_ipld_get(data_size: i64) -> i64;
    fn on_ipld_put(data_size: i64) -> i64;

    fn on_create_actor() -> i64;
    fn on_delete_actor() -> i64;

    fn on_verify_signature(sig_type: crypto::SignatureType, plan_text_size: i64 ) -> i64;
    fn on_hashing(data_size: i64) -> i64;
    fn on_compute_unsealed_sector_cid(proof_type: RegisteredProof, pieces: &[PieceInfo]) -> i64;
    fn on_verify_seal(info: SealVerifyInfo) -> i64;
    fn on_verify_post(info: PoStVerifyInfo) -> i64;
    fn on_verify_consensus_fault() -> i64;
}