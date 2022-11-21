use crate::{Allocation, Claim};
use fil_actors_runtime_v9::{
    parse_uint_key, ActorError, AsActorError, BatchReturn, BatchReturnGen, MapMap,
};
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::error::ExitCode;
use fvm_shared::ActorID;
use log::info;
use serde::de::DeserializeOwned;
use serde::Serialize;

// Something with an expiration epoch.
pub trait Expires {
    fn expiration(&self) -> ChainEpoch;
}

impl Expires for Allocation {
    fn expiration(&self) -> ChainEpoch {
        self.expiration
    }
}

impl Expires for Claim {
    fn expiration(&self) -> ChainEpoch {
        self.term_start + self.term_max
    }
}

// Finds all items in a collection for some owner that have expired.
// Returns those items' keys.
pub fn find_expired<T, BS>(
    collection: &mut MapMap<BS, T, ActorID, u64>,
    owner: ActorID,
    curr_epoch: ChainEpoch,
) -> Result<Vec<u64>, ActorError>
where
    T: Expires + Serialize + DeserializeOwned + Clone + PartialEq,
    BS: Blockstore,
{
    let mut found_ids = Vec::<u64>::new();
    collection
        .for_each(owner, |key, record| {
            if curr_epoch >= record.expiration() {
                let id = parse_uint_key(key)
                    .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to parse uint key")?;
                found_ids.push(id);
            }
            Ok(())
        })
        .context_code(
            ExitCode::USR_ILLEGAL_STATE,
            "failed to iterate over allocations/claims",
        )?;
    Ok(found_ids)
}

// Checks each candidate item from the collection for expiration.
// Returns a batch return with OK for expired items, and FORBIDDEN for non-expired.
pub fn check_expired<T, BS>(
    collection: &mut MapMap<BS, T, ActorID, u64>,
    candidates: &Vec<u64>,
    owner: ActorID,
    curr_epoch: ChainEpoch,
) -> Result<BatchReturn, ActorError>
where
    T: Expires + Serialize + DeserializeOwned + Clone + PartialEq,
    BS: Blockstore,
{
    let mut ret_gen = BatchReturnGen::new(candidates.len());
    for id in candidates {
        // Check each specified claim is expired.
        let maybe_record = collection.get(owner, *id).context_code(
            ExitCode::USR_ILLEGAL_STATE,
            "HAMT lookup failure getting allocation/claim",
        )?;

        if let Some(record) = maybe_record {
            if curr_epoch >= record.expiration() {
                ret_gen.add_success();
            } else {
                ret_gen.add_fail(ExitCode::USR_FORBIDDEN);
                info!("cannot remove allocation/claim {} that has not expired", id);
            }
        } else {
            ret_gen.add_fail(ExitCode::USR_NOT_FOUND);
            info!(
                "allocation/claim references id {} that does not belong to {}",
                id, owner,
            );
        }
    }
    Ok(ret_gen.gen())
}
