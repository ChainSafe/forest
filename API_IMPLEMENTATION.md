# Forest API Implementation Report

## Stats

- Forest method count: 68
- Lotus method count: 173
- API coverage: 39.31%

## Forest-only Methods

These methods exist in Forest only and cannot be compared:

- `Filecoin.AuthNew`
- `Filecoin.AuthVerify`
- `Filecoin.ChainGetTipsetByHeight`
- `Filecoin.ChainHeadSubscription`
- `Filecoin.ChainNotify`
- `Filecoin.MpoolEstimateGasPrice`
- `Filecoin.NetAddrsListen`
- `Filecoin.NetPeers`
- `Filecoin.StateGetReceipt`
- `Filecoin.StateLookupId`
- `Filecoin.StateSectorPrecommitInfo`
- `Filecoin.Version`

## Type Mismatches

Some methods contain possible inconsistencies between Forest and Lotus.

### Params Mismatches

| Method | Param Index | Forest Param | Lotus Param |
| ------ | ----------- | ------------ | ----------- |
| `Filecoin.WalletNew`                                 | `0` | `SignatureTypeJson` | `KeyType`

### Results Mismatches

| Method | Forest Result | Lotus Result |
| ------ | ------------- | ------------ |
| `Filecoin.StateNetworkName`                          | `String` | `dNetworkName`

## Method Table
| Present | Method                                               | Params | Result |
| ------- | ---------------------------------------------------- | ------ | ------
|   ✔️    | `Filecoin.BeaconGetEntry`                            | `(ChainEpoch)` | `BeaconEntryJson` |
|   ❌    | `Filecoin.ChainBlockstoreInfo`                       | `-` | `-` |
|   ❌    | `Filecoin.ChainCheckBlockstore`                      | `-` | `-` |
|   ❌    | `Filecoin.ChainDeleteObj`                            | `-` | `-` |
|   ✔️    | `Filecoin.ChainGetBlock`                             | `(CidJson)` | `BlockHeaderJson` |
|   ✔️    | `Filecoin.ChainGetBlockMessages`                     | `(CidJson)` | `BlockMessages` |
|   ✔️    | `Filecoin.ChainGetGenesis`                           | `()` | `Option<TipsetJson>` |
|   ✔️    | `Filecoin.ChainGetMessage`                           | `(CidJson)` | `UnsignedMessageJson` |
|   ❌    | `Filecoin.ChainGetMessagesInTipset`                  | `-` | `-` |
|   ❌    | `Filecoin.ChainGetNode`                              | `-` | `-` |
|   ❌    | `Filecoin.ChainGetParentMessages`                    | `-` | `-` |
|   ❌    | `Filecoin.ChainGetParentReceipts`                    | `-` | `-` |
|   ❌    | `Filecoin.ChainGetPath`                              | `-` | `-` |
|   ❌    | `Filecoin.ChainGetRandomnessFromBeacon`              | `-` | `-` |
|   ❌    | `Filecoin.ChainGetRandomnessFromTickets`             | `-` | `-` |
|   ✔️    | `Filecoin.ChainGetTipSet`                            | `(TipsetKeysJson)` | `TipsetJson` |
|   ❌    | `Filecoin.ChainGetTipSetAfterHeight`                 | `-` | `-` |
|   ❌    | `Filecoin.ChainGetTipSetByHeight`                    | `-` | `-` |
|   ✔️    | `Filecoin.ChainHasObj`                               | `(CidJson)` | `bool` |
|   ✔️    | `Filecoin.ChainHead`                                 | `()` | `TipsetJson` |
|   ✔️    | `Filecoin.ChainReadObj`                              | `(CidJson)` | `Vec<u8>` |
|   ❌    | `Filecoin.ChainSetHead`                              | `-` | `-` |
|   ❌    | `Filecoin.ChainStatObj`                              | `-` | `-` |
|   ✔️    | `Filecoin.ChainTipSetWeight`                         | `(TipsetKeysJson)` | `BigInt` |
|   ❌    | `Filecoin.ClientCalcCommP`                           | `-` | `-` |
|   ❌    | `Filecoin.ClientCancelDataTransfer`                  | `-` | `-` |
|   ❌    | `Filecoin.ClientCancelRetrievalDeal`                 | `-` | `-` |
|   ❌    | `Filecoin.ClientDealPieceCID`                        | `-` | `-` |
|   ❌    | `Filecoin.ClientDealSize`                            | `-` | `-` |
|   ❌    | `Filecoin.ClientFindData`                            | `-` | `-` |
|   ❌    | `Filecoin.ClientGenCar`                              | `-` | `-` |
|   ❌    | `Filecoin.ClientGetDealInfo`                         | `-` | `-` |
|   ❌    | `Filecoin.ClientGetDealStatus`                       | `-` | `-` |
|   ❌    | `Filecoin.ClientHasLocal`                            | `-` | `-` |
|   ❌    | `Filecoin.ClientImport`                              | `-` | `-` |
|   ❌    | `Filecoin.ClientListDataTransfers`                   | `-` | `-` |
|   ❌    | `Filecoin.ClientListDeals`                           | `-` | `-` |
|   ❌    | `Filecoin.ClientListImports`                         | `-` | `-` |
|   ❌    | `Filecoin.ClientListRetrievals`                      | `-` | `-` |
|   ❌    | `Filecoin.ClientMinerQueryOffer`                     | `-` | `-` |
|   ❌    | `Filecoin.ClientQueryAsk`                            | `-` | `-` |
|   ❌    | `Filecoin.ClientRemoveImport`                        | `-` | `-` |
|   ❌    | `Filecoin.ClientRestartDataTransfer`                 | `-` | `-` |
|   ❌    | `Filecoin.ClientRetrieve`                            | `-` | `-` |
|   ❌    | `Filecoin.ClientRetrieveTryRestartInsufficientFunds` | `-` | `-` |
|   ❌    | `Filecoin.ClientStartDeal`                           | `-` | `-` |
|   ❌    | `Filecoin.ClientStatelessDeal`                       | `-` | `-` |
|   ❌    | `Filecoin.CreateBackup`                              | `-` | `-` |
|   ✔️    | `Filecoin.GasEstimateFeeCap`                         | `(UnsignedMessageJson, i64, TipsetKeysJson)` | `BigInt` |
|   ✔️    | `Filecoin.GasEstimateGasLimit`                       | `(UnsignedMessageJson, TipsetKeysJson)` | `i64` |
|   ✔️    | `Filecoin.GasEstimateGasPremium`                     | `(u64, AddressJson, i64, TipsetKeysJson)` | `BigInt` |
|   ✔️    | `Filecoin.GasEstimateMessageGas`                     | `(UnsignedMessageJson, Option<MessageSendSpec>, TipsetKeysJson)` | `UnsignedMessageJson` |
|   ❌    | `Filecoin.MarketAddBalance`                          | `-` | `-` |
|   ❌    | `Filecoin.MarketGetReserved`                         | `-` | `-` |
|   ❌    | `Filecoin.MarketReleaseFunds`                        | `-` | `-` |
|   ❌    | `Filecoin.MarketReserveFunds`                        | `-` | `-` |
|   ❌    | `Filecoin.MarketWithdraw`                            | `-` | `-` |
|   ✔️    | `Filecoin.MinerCreateBlock`                          | `(BlockTemplate)` | `BlockMsgJson` |
|   ✔️    | `Filecoin.MinerGetBaseInfo`                          | `(AddressJson, ChainEpoch, TipsetKeysJson)` | `Option<MiningBaseInfoJson>` |
|   ❌    | `Filecoin.MpoolBatchPush`                            | `-` | `-` |
|   ❌    | `Filecoin.MpoolBatchPushMessage`                     | `-` | `-` |
|   ❌    | `Filecoin.MpoolBatchPushUntrusted`                   | `-` | `-` |
|   ❌    | `Filecoin.MpoolCheckMessages`                        | `-` | `-` |
|   ❌    | `Filecoin.MpoolCheckPendingMessages`                 | `-` | `-` |
|   ❌    | `Filecoin.MpoolCheckReplaceMessages`                 | `-` | `-` |
|   ❌    | `Filecoin.MpoolClear`                                | `-` | `-` |
|   ❌    | `Filecoin.MpoolGetConfig`                            | `-` | `-` |
|   ✔️    | `Filecoin.MpoolGetNonce`                             | `(Address)` | `u64` |
|   ✔️    | `Filecoin.MpoolPending`                              | `(TipsetKeys)` | `Vec<SignedMessage>` |
|   ✔️    | `Filecoin.MpoolPush`                                 | `(SignedMessageJson)` | `CidJson` |
|   ✔️    | `Filecoin.MpoolPushMessage`                          | `(UnsignedMessageJson, Option<MessageSendSpec>)` | `SignedMessageJson` |
|   ❌    | `Filecoin.MpoolPushUntrusted`                        | `-` | `-` |
|   ✔️    | `Filecoin.MpoolSelect`                               | `(TipsetKeysJson, f64)` | `Vec<SignedMessageJson>` |
|   ❌    | `Filecoin.MpoolSetConfig`                            | `-` | `-` |
|   ❌    | `Filecoin.MsigAddApprove`                            | `-` | `-` |
|   ❌    | `Filecoin.MsigAddCancel`                             | `-` | `-` |
|   ❌    | `Filecoin.MsigAddPropose`                            | `-` | `-` |
|   ❌    | `Filecoin.MsigApprove`                               | `-` | `-` |
|   ❌    | `Filecoin.MsigApproveTxnHash`                        | `-` | `-` |
|   ❌    | `Filecoin.MsigCancel`                                | `-` | `-` |
|   ❌    | `Filecoin.MsigCreate`                                | `-` | `-` |
|   ❌    | `Filecoin.MsigGetAvailableBalance`                   | `-` | `-` |
|   ❌    | `Filecoin.MsigGetPending`                            | `-` | `-` |
|   ❌    | `Filecoin.MsigGetVested`                             | `-` | `-` |
|   ❌    | `Filecoin.MsigGetVestingSchedule`                    | `-` | `-` |
|   ❌    | `Filecoin.MsigPropose`                               | `-` | `-` |
|   ❌    | `Filecoin.MsigRemoveSigner`                          | `-` | `-` |
|   ❌    | `Filecoin.MsigSwapApprove`                           | `-` | `-` |
|   ❌    | `Filecoin.MsigSwapCancel`                            | `-` | `-` |
|   ❌    | `Filecoin.MsigSwapPropose`                           | `-` | `-` |
|   ❌    | `Filecoin.NodeStatus`                                | `-` | `-` |
|   ❌    | `Filecoin.PaychAllocateLane`                         | `-` | `-` |
|   ❌    | `Filecoin.PaychAvailableFunds`                       | `-` | `-` |
|   ❌    | `Filecoin.PaychAvailableFundsByFromTo`               | `-` | `-` |
|   ❌    | `Filecoin.PaychCollect`                              | `-` | `-` |
|   ❌    | `Filecoin.PaychGet`                                  | `-` | `-` |
|   ❌    | `Filecoin.PaychGetWaitReady`                         | `-` | `-` |
|   ❌    | `Filecoin.PaychList`                                 | `-` | `-` |
|   ❌    | `Filecoin.PaychNewPayment`                           | `-` | `-` |
|   ❌    | `Filecoin.PaychSettle`                               | `-` | `-` |
|   ❌    | `Filecoin.PaychStatus`                               | `-` | `-` |
|   ❌    | `Filecoin.PaychVoucherAdd`                           | `-` | `-` |
|   ❌    | `Filecoin.PaychVoucherCheckSpendable`                | `-` | `-` |
|   ❌    | `Filecoin.PaychVoucherCheckValid`                    | `-` | `-` |
|   ❌    | `Filecoin.PaychVoucherCreate`                        | `-` | `-` |
|   ❌    | `Filecoin.PaychVoucherList`                          | `-` | `-` |
|   ❌    | `Filecoin.PaychVoucherSubmit`                        | `-` | `-` |
|   ✔️    | `Filecoin.StateAccountKey`                           | `(AddressJson, TipsetKeysJson)` | `Option<AddressJson>` |
|   ✔️    | `Filecoin.StateAllMinerFaults`                       | `(ChainEpoch, TipsetKeysJson)` | `Vec<Fault>` |
|   ✔️    | `Filecoin.StateCall`                                 | `(UnsignedMessageJson, TipsetKeysJson)` | `InvocResult` |
|   ❌    | `Filecoin.StateChangedActors`                        | `-` | `-` |
|   ❌    | `Filecoin.StateCirculatingSupply`                    | `-` | `-` |
|   ❌    | `Filecoin.StateCompute`                              | `-` | `-` |
|   ❌    | `Filecoin.StateDealProviderCollateralBounds`         | `-` | `-` |
|   ❌    | `Filecoin.StateDecodeParams`                         | `-` | `-` |
|   ✔️    | `Filecoin.StateGetActor`                             | `(AddressJson, TipsetKeysJson)` | `Option<ActorStateJson>` |
|   ✔️    | `Filecoin.StateListActors`                           | `(TipsetKeysJson)` | `Vec<AddressJson>` |
|   ❌    | `Filecoin.StateListMessages`                         | `-` | `-` |
|   ❌    | `Filecoin.StateListMiners`                           | `-` | `-` |
|   ❌    | `Filecoin.StateLookupID`                             | `-` | `-` |
|   ✔️    | `Filecoin.StateMarketBalance`                        | `(AddressJson, TipsetKeysJson)` | `MarketBalance` |
|   ✔️    | `Filecoin.StateMarketDeals`                          | `(TipsetKeysJson)` | `HashMap<String, MarketDeal>` |
|   ❌    | `Filecoin.StateMarketParticipants`                   | `-` | `-` |
|   ❌    | `Filecoin.StateMarketStorageDeal`                    | `-` | `-` |
|   ❌    | `Filecoin.StateMinerActiveSectors`                   | `-` | `-` |
|   ❌    | `Filecoin.StateMinerAvailableBalance`                | `-` | `-` |
|   ✔️    | `Filecoin.StateMinerDeadlines`                       | `(AddressJson, TipsetKeysJson)` | `Vec<Deadline>` |
|   ✔️    | `Filecoin.StateMinerFaults`                          | `(AddressJson, TipsetKeysJson)` | `BitFieldJson` |
|   ✔️    | `Filecoin.StateMinerInfo`                            | `(AddressJson, TipsetKeysJson)` | `MinerInfo` |
|   ✔️    | `Filecoin.StateMinerInitialPledgeCollateral`         | `(AddressJson, SectorPreCommitInfo, TipsetKeysJson)` | `BigInt` |
|   ✔️    | `Filecoin.StateMinerPartitions`                      | `(AddressJson, u64, TipsetKeysJson)` | `Vec<Partition>` |
|   ✔️    | `Filecoin.StateMinerPower`                           | `(Option<AddressJson>, TipsetKeysJson)` | `MinerPower` |
|   ✔️    | `Filecoin.StateMinerPreCommitDepositForPower`        | `(AddressJson, SectorPreCommitInfo, TipsetKeysJson)` | `BigInt` |
|   ✔️    | `Filecoin.StateMinerProvingDeadline`                 | `(AddressJson, TipsetKeysJson)` | `DeadlineInfo` |
|   ✔️    | `Filecoin.StateMinerRecoveries`                      | `(AddressJson, TipsetKeysJson)` | `BitFieldJson` |
|   ✔️    | `Filecoin.StateMinerSectorAllocated`                 | `(AddressJson, SectorNumber, TipsetKeysJson)` | `bool` |
|   ❌    | `Filecoin.StateMinerSectorCount`                     | `-` | `-` |
|   ✔️    | `Filecoin.StateMinerSectors`                         | `(AddressJson, BitFieldJson, TipsetKeysJson)` | `Vec<SectorOnChainInfo>` |
|   ✔️    | `Filecoin.StateNetworkName`                          | `()` | `String` |
|   ✔️    | `Filecoin.StateNetworkVersion`                       | `(TipsetKeysJson)` | `NetworkVersion` |
|   ❌    | `Filecoin.StateReadState`                            | `-` | `-` |
|   ✔️    | `Filecoin.StateReplay`                               | `(TipsetKeys, Cid)` | `InvocResult` |
|   ❌    | `Filecoin.StateSearchMsg`                            | `-` | `-` |
|   ❌    | `Filecoin.StateSectorExpiration`                     | `-` | `-` |
|   ✔️    | `Filecoin.StateSectorGetInfo`                        | `(AddressJson, SectorNumber, TipsetKeysJson)` | `Option<SectorOnChainInfo>` |
|   ❌    | `Filecoin.StateSectorPartition`                      | `-` | `-` |
|   ❌    | `Filecoin.StateSectorPreCommitInfo`                  | `-` | `-` |
|   ❌    | `Filecoin.StateVMCirculatingSupplyInternal`          | `-` | `-` |
|   ❌    | `Filecoin.StateVerifiedClientStatus`                 | `-` | `-` |
|   ❌    | `Filecoin.StateVerifiedRegistryRootKey`              | `-` | `-` |
|   ❌    | `Filecoin.StateVerifierStatus`                       | `-` | `-` |
|   ✔️    | `Filecoin.StateWaitMsg`                              | `(CidJson, u64)` | `MessageLookup` |
|   ✔️    | `Filecoin.SyncCheckBad`                              | `(CidJson)` | `String` |
|   ❌    | `Filecoin.SyncCheckpoint`                            | `-` | `-` |
|   ❌    | `Filecoin.SyncMarkBad`                               | `-` | `-` |
|   ✔️    | `Filecoin.SyncState`                                 | `()` | `RPCSyncState` |
|   ❌    | `Filecoin.SyncSubmitBlock`                           | `-` | `-` |
|   ❌    | `Filecoin.SyncUnmarkAllBad`                          | `-` | `-` |
|   ❌    | `Filecoin.SyncUnmarkBad`                             | `-` | `-` |
|   ❌    | `Filecoin.SyncValidateTipset`                        | `-` | `-` |
|   ✔️    | `Filecoin.WalletBalance`                             | `(Address)` | `BigInt` |
|   ✔️    | `Filecoin.WalletDefaultAddress`                      | `()` | `Address` |
|   ❌    | `Filecoin.WalletDelete`                              | `-` | `-` |
|   ✔️    | `Filecoin.WalletExport`                              | `(Address)` | `KeyInfoJson` |
|   ✔️    | `Filecoin.WalletHas`                                 | `(Address)` | `bool` |
|   ✔️    | `Filecoin.WalletImport`                              | `()` | `Address` |
|   ✔️    | `Filecoin.WalletList`                                | `()` | `Vec<AddressJson>` |
|   ✔️    | `Filecoin.WalletNew`                                 | `(SignatureTypeJson)` | `Address` |
|   ❌    | `Filecoin.WalletSetDefault`                          | `-` | `-` |
|   ✔️    | `Filecoin.WalletSign`                                | `(AddressJson, Vec<u8>)` | `SignatureJson` |
|   ✔️    | `Filecoin.WalletSignMessage`                         | `(Address, UnsignedMessageJson)` | `SignedMessageJson` |
|   ❌    | `Filecoin.WalletValidateAddress`                     | `-` | `-` |
|   ✔️    | `Filecoin.WalletVerify`                              | `(Address, Vec<u8>, SignatureJson)` | `bool` |

## Help & Contributions

If there's a particular API that's needed that we're missing, be sure to let us know.

Feel free to reach out in #fil-forest-help in the [Filecoin Slack](https://docs.filecoin.io/community/chat-and-discussion-forums/), file a GitHub issue, or contribute a pull request.
