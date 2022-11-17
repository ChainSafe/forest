use std::convert::TryFrom;

use anyhow::Result;
use cid::multihash::Code;
use cid::Cid;
use fvm_ipld_blockstore::Block;
use fvm_sdk as fvm;

use crate::actor_error;

/// A blockstore suitable for use within actors.
///
/// Cloning simply clones a reference and does not copy the underlying blocks.
#[derive(Debug, Clone)]
pub struct ActorBlockstore;

/// Implements a blockstore delegating to IPLD syscalls.
impl fvm_ipld_blockstore::Blockstore for ActorBlockstore {
    fn get(&self, cid: &Cid) -> Result<Option<Vec<u8>>> {
        // If this fails, the _CID_ is invalid. I.e., we have a bug.
        fvm::ipld::get(cid).map(Some).map_err(|c| {
            actor_error!(illegal_state; "get failed with {:?} on CID '{}'", c, cid).into()
        })
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> Result<()> {
        let code = Code::try_from(k.hash().code())
            .map_err(|e| actor_error!(serialization, e.to_string()))?;
        let k2 = self.put(code, &Block::new(k.codec(), block))?;
        if k != &k2 {
            Err(actor_error!(serialization; "put block with cid {} but has cid {}", k, k2).into())
        } else {
            Ok(())
        }
    }

    fn put<D>(&self, code: Code, block: &Block<D>) -> Result<Cid>
    where
        D: AsRef<[u8]>,
    {
        // TODO: Don't hard-code the size. Unfortunately, there's no good way to get it from the
        //  codec at the moment.
        const SIZE: u32 = 32;
        let k = fvm::ipld::put(code.into(), SIZE, block.codec, block.data.as_ref())
            .map_err(|c| actor_error!(illegal_state; "put failed with {:?}", c))?;
        Ok(k)
    }
}
