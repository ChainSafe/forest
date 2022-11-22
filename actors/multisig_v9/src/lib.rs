// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_shared::METHOD_CONSTRUCTOR;
use num_derive::FromPrimitive;

use fil_actors_runtime_v9::cbor::serialize_vec;
use fil_actors_runtime_v9::make_map_with_root;
use fil_actors_runtime_v9::runtime::Primitives;

pub use self::state::*;
pub use self::types::*;

#[cfg(feature = "fil-actor")]
fil_actors_runtime::wasm_trampoline!(Actor);

mod state;
pub mod testing;
mod types;

/// Multisig actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    Propose = 2,
    Approve = 3,
    Cancel = 4,
    AddSigner = 5,
    RemoveSigner = 6,
    SwapSigner = 7,
    ChangeNumApprovalsThreshold = 8,
    LockBalance = 9,
    UniversalReceiverHook = frc42_dispatch::method_hash!("Receive"),
}

/// Computes a digest of a proposed transaction. This digest is used to confirm identity
/// of the transaction associated with an ID, which might change under chain re-orgs.
pub fn compute_proposal_hash(txn: &Transaction, sys: &dyn Primitives) -> anyhow::Result<[u8; 32]> {
    let proposal_hash = ProposalHashData {
        requester: txn.approved.get(0),
        to: &txn.to,
        value: &txn.value,
        method: &txn.method,
        params: &txn.params,
    };
    let data = serialize_vec(&proposal_hash, "proposal hash")?;
    Ok(sys.hash_blake2b(&data))
}
