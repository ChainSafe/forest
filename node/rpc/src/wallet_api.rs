use wallet::*;
use state_manager::*;
use blockstore::BlockStore;

pub struct WalletApi<DB, T> {
    state_manager: StateManager<DB>,
    wallet: Wallet<T>
}

impl<DB, T>  WalletApi<DB, T>
    where
        DB: BlockStore,
        T: KeyStore
{
    pub fn new(state_manager: StateManager<DB>, wallet: Wallet<T>) -> Self
        where
            DB: BlockStore,
            T: KeyStore
    {
        WalletApi {
            state_manager,
            wallet
        }
    }
}