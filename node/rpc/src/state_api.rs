pub(crate) async fn state_miner_sector<DB: BlockStore + Send + Sync + 'static>(
    data: Data<State<DB>>,
    Params(params): Params<(AddressJson,BitFieldJson,bool,TipsetKeysJson)>,
) -> Result<Vec<ChainSectorInfoJson>, JsonRpcError> {
    let (address,bitfield,filter,tipset_keys) = params;
    let state_manager = &data.state_manager;
    state_api::state_miner_sector(state_manager,&address,&bitfield,&filter,&key).map(|vec|vec.iter().map(|s|ChainSectorInfoJson(s)).collect())
}

pub(crate) async fn state_call<DB: BlockStore + Send + Sync + 'static>(
    data: Data<State<DB>>,
    Params(params): Params<(UnisignedMessageJson,TipsetKeysJson)>,
) -> Result<InvocResultJson<UnsignedMessageJson>, JsonRpcError> {
    let (message,key) = params;
    state_api::state_call(&data.state_manage,&message,&key).map(|s|s.into())
}

pub(crate) async fn state_miner_deadlines<DB: BlockStore + Send + Sync + 'static>(
    data: Data<State<DB>>,
    Params(params): Params<(AddressJson,TipsetKeysJson)>,
) -> Result<DeadlineJson, JsonRpcError> {
    let (address,key) = params;
    state_api::state_miner_deadlines(&data.state_manage,&address,&key).map(|s|s.into())
}


pub(crate) async fn state_miner_precommit_info<DB: BlockStore + Send + Sync + 'static>(
    data: Data<State<DB>>,
    Params(params): Params<(AddressJson,TipsetKeysJson)>,
) -> Result<SectorPreCommitOnChainInfoJson, JsonRpcError> {
    let (address,key) = params;
    state_api::state_miner_deadlines(&data.state_manager,&address,&key).map(|s|s.into())
}

pub (crate) async fn state_miner_proving_set<DB>(
    data: Data<State<DB>>,
    Params(params): Params<(AddressJson,TipsetKeysJson)>,
) -> Result<Vec<SectorOnChainInfoJson>, BoxError>
where
    DB: BlockStore,
{
    let (address,key) = params;
    state_api::state_miner_proving_set(&data.state_manager, &address,&key)
}

/// StateMinerInfo returns info about the indicated miner
pub fn state_miner_info<DB>(
    data: Data<State<DB>>,
    Params(params): Params<(AddressJson,TipsetKeysJson)>,
) -> Result<MinerInfoJson, BoxError>
where
    DB: BlockStore,
{
    let (address,key) = params;
    state_api::state_miner_info(&data.state_manager, &address, key)
}

/// returns the on-chain info for the specified miner's sector
pub fn state_sector_info<DB>(
    data: Data<State<DB>>,
    Params(params): Params<(AddressJson,SectorNumberJson,TipsetKeysJson)>
) -> Result<Option<SectorOnChainInfoJson>, BoxError>
where
    DB: BlockStore,
{
    let (address,sector_number,key) = params;
    state_api::state_sector_info(&data.state_manager, &key, &key, &key)
   
}



/// calculates the deadline at some epoch for a proving period
/// and returns the deadline-related calculations.
pub fn state_miner_proving_deadline<DB>(
    data: Data<State<DB>>,
    Params(params): Params<(AddressJson,TipsetKeysJson)>,
) -> Result<DeadlineInfoJson, BoxError>
where
    DB: BlockStore,
{
    let (address,key) = params;
    state_api::state_miner_proving_deadline(&data.state_manager, &key)
   
}

/// returns a single non-expired Faults that occur within lookback epochs of the given tipset
pub fn state_miner_faults<DB>(
    data: Data<State<DB>>,
    Params(params): Params<(AddressJson,TipsetKeysJson)>,
) -> Result<BitFieldJson, BoxError>
where
    DB: BlockStore,
{
    let (address,key) = params;
    state_api::state_miner_faults(&data.state_manager, &address,&key)
}

/// returns all non-expired Faults that occur within lookback epochs of the given tipset
pub fn state_all_miner_faults<DB>(
    data: Data<State<DB>>,
    Params(params): Params<(ChainEpochJson,TipsetKeysJson)>,
) -> Result<Vec<FaultJson>, BoxError>
where
    DB: BlockStore,
{
    let (chain_epoch,key) = params;
    state_api::state_all_miner_faults(&data.state_manager, &chain_epoch,&key)
}
/// returns a bitfield indicating the recovering sectors of the given miner
pub fn state_miner_recoveries<DB>(
    data: Data<State<DB>>,
    Params(params): Params<(AddressJson,TipsetKeysJson)>,
) -> Result<BitFieldJson, BoxError>
where
    DB: BlockStore,
{
    let (address,key) = params;
    state_api::state_miner_recoveries(&data.state_manager, &address,&key)
}

/// returns the power of the indicated miner
pub fn state_miner_power<DB>(
    data: Data<State<DB>>,
    Params(params): Params<(AddressJson,TipsetKeysJson)>,
) -> Result<BitFieldJson, BoxError>
where
    DB: BlockStore,
{
    let (address,key) = params;
    state_api::state_miner_power(&data.state_manager, &address,&key)
}



/// runs the given message and returns its result without any persisted changes.
pub fn state_call<DB>(
    data: Data<State<DB>>,
    Params(params): Params<(UnsignedMessageJson,TipsetKeysJson)>,
) -> Result<InvocResultJson<UnsignedMessage>, BoxError>
where
    DB: BlockStore,
{
    let (unsigned_msg,key) = params;
    state_api::state_call(&data.state_manager, &unsigned_msg,&key)
}

/// returns the result of executing the indicated message, assuming it was executed in the indicated tipset.
pub fn state_reply<DB>(
    data: Data<State<DB>>,
    Params(params): Params<(CidJson,TipsetKeysJson)>,
) -> Result<InvocResultJson<UnsignedMessage>, BoxError>
where
    DB: BlockStore,
{
    let (cid,key) = params;
    state_api::state_reply(&data.state_manager, &cid,&key)
}

/// returns the indicated actor's nonce and balance.
pub fn state_get_actor<DB>(
    data: Data<State<DB>>,
    Params(params): Params<(AddressJson,TipsetKeysJson)>,
) -> Result<Option<actor::ActorStateJson>, BoxError>
where
    DB: BlockStore,
{
    let (address,key) = params;
    state_api::state_get_actor(&data.state_manager, &address,&key)
}

/// returns the public key address of the given ID address
pub fn state_account_key<DB>(
    data: Data<State<DB>>,
    Params(params): Params<(AddressJson,TipsetKeysJson)>,
) -> Result<Option<actor::ActorStateJson>, BoxError>
where
    DB: BlockStore,
{
    let (address,key) = params;
    state_api::state_account_key(&data.state_manager, &address,&key)
}

pub fn state_lookup_id<DB>(
    data: Data<State<DB>>,
    Params(params): Params<(AddressJson,TipsetKeysJson)>,
) -> Result<Option<actor::ActorStateJson>, BoxError>
where
    DB: BlockStore,
{
    let (address,key) = params;
    state_api::state_lookup_id(&data.state_manager, &address,&key)
}

/// returns the addresses of every actor in the state
pub fn state_list_actors<DB>(
    data: Data<State<DB>>,
    Params(params): Params<(TipsetKeysJson)>,
) -> Result<Vec<AddressJson>, BoxError>
where
    DB: BlockStore,
{
    let (tipset) = params;
    state_api::state_list_actors(&data.state_manager, &tipset)
}

pub fn state_market_balance<DB>(
    data: Data<State<DB>>,
    Params(params): Params<(AddressJson,TipsetKeysJson)>,
) -> Result<Option<actor::ActorStateJson>, BoxError>
where
    DB: BlockStore,
{
    let (address,key) = params;
    state_api::state_market_balance(&data.state_manager, &address,&key)
}

/// returns the message receipt for the given message
pub fn state_get_receipt<DB>(
    data: Data<State<DB>>,
    Params(params): Params<(CidJson,TipsetKeysJson)>,
) -> Result<Option<actor::ActorStateJson>, BoxError>
where
    DB: BlockStore,
{
    let (cid,key) = params;
    state_api::state_get_receipt(&data.state_manager, &cid,&key)
}
/// looks back in the chain for a message. If not found, it blocks until the
/// message arrives on chain, and gets to the indicated confidence depth.
pub fn state_wait_msg<DB>(
    data: Data<State<DB>>,
    Params(params): Params<(CidJson,u64)>,
) -> Result<MessageLookupJson, BoxError>
where
    DB: BlockStore,
{
    let (cid,confidence) = params;
    state_api::state_wait_msg(&data.state_manager, &cid,&key)
}
