use ipld_blockstore::BlockStore;
use vm::{GasTracker, PriceList};

pub(crate) struct GasBlockStore<'bs>
where
    BS: BlockStore,
{
    pub price_list: PriceList,
    pub gas: GasTracker,
    pub store: &'bs BS,
}
