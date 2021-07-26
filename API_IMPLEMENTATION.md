# Forest API Implementation Report

## Stats

- Forest method count: 66
- Lotus method count: 169
- API coverage: 39.05%

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
| `Filecoin.MpoolGetNonce`                             | `0` | `String` | `Address`
| `Filecoin.MpoolPending`                              | `0` | `CidJsonVec` | `TipsetKeys`
| `Filecoin.StateMinerSectorAllocated`                 | `1` | `u64` | `SectorNumber`
| `Filecoin.StateReplay`                               | `0` | `CidJson` | `TipsetKeys`
| `Filecoin.StateReplay`                               | `1` | `TipsetKeysJson` | `Cid`
| `Filecoin.StateWaitMsg`                              | `1` | `i64` | `u64`
| `Filecoin.WalletBalance`                             | `0` | `String` | `Address`
| `Filecoin.WalletExport`                              | `0` | `String` | `Address`
| `Filecoin.WalletHas`                                 | `0` | `String` | `Address`
| `Filecoin.WalletNew`                                 | `0` | `SignatureTypeJson` | `KeyType`
| `Filecoin.WalletSignMessage`                         | `0` | `String` | `Address`
| `Filecoin.WalletVerify`                              | `0` | `String` | `Address`
| `Filecoin.WalletVerify`                              | `1` | `String` | `Vec<u8>`

### Results Mismatches

| Method | Forest Result | Lotus Result |
| ------ | ------------- | ------------ |
| `Filecoin.ChainReadObj`                              | `String` | `Vec<u8>`
| `Filecoin.ChainTipSetWeight`                         | `String` | `BigInt`
| `Filecoin.GasEstimateFeeCap`                         | `String` | `BigInt`
| `Filecoin.GasEstimateGasPremium`                     | `String` | `BigInt`
| `Filecoin.StateMinerInitialPledgeCollateral`         | `String` | `BigInt`
| `Filecoin.StateMinerPreCommitDepositForPower`        | `String` | `BigInt`
| `Filecoin.StateNetworkName`                          | `String` | `dNetworkName`
| `Filecoin.WalletBalance`                             | `String` | `BigInt`
| `Filecoin.WalletDefaultAddress`                      | `String` | `Address`
| `Filecoin.WalletImport`                              | `String` | `Address`
| `Filecoin.WalletNew`                                 | `String` | `Address`

## Method Table
|   | Method                                             | Params | Result |
| - | -------------------------------------------------- | ------ | ------
[x] | `Filecoin.BeaconGetEntry`                            | `(ChainEpoch)` | `BeaconEntryJson` |
[ ] | `Filecoin.ChainDeleteObj`                            | `-` | `-` |
[x] | `Filecoin.ChainGetBlock`                             | `(CidJson)` | `BlockHeaderJson` |
[x] | `Filecoin.ChainGetBlockMessages`                     | `(CidJson)` | `BlockMessages` |
[x] | `Filecoin.ChainGetGenesis`                           | `()` | `Option<TipsetJson>` |
[x] | `Filecoin.ChainGetMessage`                           | `(CidJson)` | `UnsignedMessageJson` |
[ ] | `Filecoin.ChainGetNode`                              | `-` | `-` |
[ ] | `Filecoin.ChainGetParentMessages`                    | `-` | `-` |
[ ] | `Filecoin.ChainGetParentReceipts`                    | `-` | `-` |
[ ] | `Filecoin.ChainGetPath`                              | `-` | `-` |
[ ] | `Filecoin.ChainGetRandomnessFromBeacon`              | `-` | `-` |
[ ] | `Filecoin.ChainGetRandomnessFromTickets`             | `-` | `-` |
[x] | `Filecoin.ChainGetTipSet`                            | `(TipsetKeysJson)` | `TipsetJson` |
[ ] | `Filecoin.ChainGetTipSetByHeight`                    | `-` | `-` |
[x] | `Filecoin.ChainHasObj`                               | `(CidJson)` | `bool` |
[x] | `Filecoin.ChainHead`                                 | `()` | `TipsetJson` |
[x] | `Filecoin.ChainReadObj`                              | `(CidJson)` | `String` |
[ ] | `Filecoin.ChainSetHead`                              | `-` | `-` |
[ ] | `Filecoin.ChainStatObj`                              | `-` | `-` |
[x] | `Filecoin.ChainTipSetWeight`                         | `(TipsetKeysJson)` | `String` |
[ ] | `Filecoin.ClientCalcCommP`                           | `-` | `-` |
[ ] | `Filecoin.ClientCancelDataTransfer`                  | `-` | `-` |
[ ] | `Filecoin.ClientCancelRetrievalDeal`                 | `-` | `-` |
[ ] | `Filecoin.ClientDealPieceCID`                        | `-` | `-` |
[ ] | `Filecoin.ClientDealSize`                            | `-` | `-` |
[ ] | `Filecoin.ClientFindData`                            | `-` | `-` |
[ ] | `Filecoin.ClientGenCar`                              | `-` | `-` |
[ ] | `Filecoin.ClientGetDealInfo`                         | `-` | `-` |
[ ] | `Filecoin.ClientGetDealStatus`                       | `-` | `-` |
[ ] | `Filecoin.ClientHasLocal`                            | `-` | `-` |
[ ] | `Filecoin.ClientImport`                              | `-` | `-` |
[ ] | `Filecoin.ClientListDataTransfers`                   | `-` | `-` |
[ ] | `Filecoin.ClientListDeals`                           | `-` | `-` |
[ ] | `Filecoin.ClientListImports`                         | `-` | `-` |
[ ] | `Filecoin.ClientListRetrievals`                      | `-` | `-` |
[ ] | `Filecoin.ClientMinerQueryOffer`                     | `-` | `-` |
[ ] | `Filecoin.ClientQueryAsk`                            | `-` | `-` |
[ ] | `Filecoin.ClientRemoveImport`                        | `-` | `-` |
[ ] | `Filecoin.ClientRestartDataTransfer`                 | `-` | `-` |
[ ] | `Filecoin.ClientRetrieve`                            | `-` | `-` |
[ ] | `Filecoin.ClientRetrieveTryRestartInsufficientFunds` | `-` | `-` |
[ ] | `Filecoin.ClientStartDeal`                           | `-` | `-` |
[ ] | `Filecoin.ClientStatelessDeal`                       | `-` | `-` |
[ ] | `Filecoin.CreateBackup`                              | `-` | `-` |
[x] | `Filecoin.GasEstimateFeeCap`                         | `(UnsignedMessageJson, i64, TipsetKeysJson)` | `String` |
[x] | `Filecoin.GasEstimateGasLimit`                       | `(UnsignedMessageJson, TipsetKeysJson)` | `i64` |
[x] | `Filecoin.GasEstimateGasPremium`                     | `(u64, AddressJson, i64, TipsetKeysJson)` | `String` |
[x] | `Filecoin.GasEstimateMessageGas`                     | `(UnsignedMessageJson, Option<MessageSendSpec>, TipsetKeysJson)` | `UnsignedMessageJson` |
[ ] | `Filecoin.MarketAddBalance`                          | `-` | `-` |
[ ] | `Filecoin.MarketGetReserved`                         | `-` | `-` |
[ ] | `Filecoin.MarketReleaseFunds`                        | `-` | `-` |
[ ] | `Filecoin.MarketReserveFunds`                        | `-` | `-` |
[ ] | `Filecoin.MarketWithdraw`                            | `-` | `-` |
[x] | `Filecoin.MinerCreateBlock`                          | `(BlockTemplate)` | `BlockMsgJson` |
[x] | `Filecoin.MinerGetBaseInfo`                          | `(AddressJson, ChainEpoch, TipsetKeysJson)` | `Option<MiningBaseInfoJson>` |
[ ] | `Filecoin.MpoolBatchPush`                            | `-` | `-` |
[ ] | `Filecoin.MpoolBatchPushMessage`                     | `-` | `-` |
[ ] | `Filecoin.MpoolBatchPushUntrusted`                   | `-` | `-` |
[ ] | `Filecoin.MpoolCheckMessages`                        | `-` | `-` |
[ ] | `Filecoin.MpoolCheckPendingMessages`                 | `-` | `-` |
[ ] | `Filecoin.MpoolCheckReplaceMessages`                 | `-` | `-` |
[ ] | `Filecoin.MpoolClear`                                | `-` | `-` |
[ ] | `Filecoin.MpoolGetConfig`                            | `-` | `-` |
[x] | `Filecoin.MpoolGetNonce`                             | `(String)` | `u64` |
[x] | `Filecoin.MpoolPending`                              | `(CidJsonVec)` | `Vec<SignedMessage>` |
[x] | `Filecoin.MpoolPush`                                 | `(SignedMessageJson)` | `CidJson` |
[x] | `Filecoin.MpoolPushMessage`                          | `(UnsignedMessageJson, Option<MessageSendSpec>)` | `SignedMessageJson` |
[ ] | `Filecoin.MpoolPushUntrusted`                        | `-` | `-` |
[x] | `Filecoin.MpoolSelect`                               | `(TipsetKeysJson, f64)` | `Vec<SignedMessageJson>` |
[ ] | `Filecoin.MpoolSetConfig`                            | `-` | `-` |
[ ] | `Filecoin.MsigAddApprove`                            | `-` | `-` |
[ ] | `Filecoin.MsigAddCancel`                             | `-` | `-` |
[ ] | `Filecoin.MsigAddPropose`                            | `-` | `-` |
[ ] | `Filecoin.MsigApprove`                               | `-` | `-` |
[ ] | `Filecoin.MsigApproveTxnHash`                        | `-` | `-` |
[ ] | `Filecoin.MsigCancel`                                | `-` | `-` |
[ ] | `Filecoin.MsigCreate`                                | `-` | `-` |
[ ] | `Filecoin.MsigGetAvailableBalance`                   | `-` | `-` |
[ ] | `Filecoin.MsigGetPending`                            | `-` | `-` |
[ ] | `Filecoin.MsigGetVested`                             | `-` | `-` |
[ ] | `Filecoin.MsigGetVestingSchedule`                    | `-` | `-` |
[ ] | `Filecoin.MsigPropose`                               | `-` | `-` |
[ ] | `Filecoin.MsigRemoveSigner`                          | `-` | `-` |
[ ] | `Filecoin.MsigSwapApprove`                           | `-` | `-` |
[ ] | `Filecoin.MsigSwapCancel`                            | `-` | `-` |
[ ] | `Filecoin.MsigSwapPropose`                           | `-` | `-` |
[ ] | `Filecoin.NodeStatus`                                | `-` | `-` |
[ ] | `Filecoin.PaychAllocateLane`                         | `-` | `-` |
[ ] | `Filecoin.PaychAvailableFunds`                       | `-` | `-` |
[ ] | `Filecoin.PaychAvailableFundsByFromTo`               | `-` | `-` |
[ ] | `Filecoin.PaychCollect`                              | `-` | `-` |
[ ] | `Filecoin.PaychGet`                                  | `-` | `-` |
[ ] | `Filecoin.PaychGetWaitReady`                         | `-` | `-` |
[ ] | `Filecoin.PaychList`                                 | `-` | `-` |
[ ] | `Filecoin.PaychNewPayment`                           | `-` | `-` |
[ ] | `Filecoin.PaychSettle`                               | `-` | `-` |
[ ] | `Filecoin.PaychStatus`                               | `-` | `-` |
[ ] | `Filecoin.PaychVoucherAdd`                           | `-` | `-` |
[ ] | `Filecoin.PaychVoucherCheckSpendable`                | `-` | `-` |
[ ] | `Filecoin.PaychVoucherCheckValid`                    | `-` | `-` |
[ ] | `Filecoin.PaychVoucherCreate`                        | `-` | `-` |
[ ] | `Filecoin.PaychVoucherList`                          | `-` | `-` |
[ ] | `Filecoin.PaychVoucherSubmit`                        | `-` | `-` |
[x] | `Filecoin.StateAccountKey`                           | `(AddressJson, TipsetKeysJson)` | `Option<AddressJson>` |
[x] | `Filecoin.StateAllMinerFaults`                       | `(ChainEpoch, TipsetKeysJson)` | `Vec<Fault>` |
[x] | `Filecoin.StateCall`                                 | `(UnsignedMessageJson, TipsetKeysJson)` | `InvocResult` |
[ ] | `Filecoin.StateChangedActors`                        | `-` | `-` |
[ ] | `Filecoin.StateCirculatingSupply`                    | `-` | `-` |
[ ] | `Filecoin.StateCompute`                              | `-` | `-` |
[ ] | `Filecoin.StateDealProviderCollateralBounds`         | `-` | `-` |
[ ] | `Filecoin.StateDecodeParams`                         | `-` | `-` |
[x] | `Filecoin.StateGetActor`                             | `(AddressJson, TipsetKeysJson)` | `Option<ActorStateJson>` |
[ ] | `Filecoin.StateListActors`                           | `-` | `-` |
[ ] | `Filecoin.StateListMessages`                         | `-` | `-` |
[ ] | `Filecoin.StateListMiners`                           | `-` | `-` |
[ ] | `Filecoin.StateLookupID`                             | `-` | `-` |
[x] | `Filecoin.StateMarketBalance`                        | `(AddressJson, TipsetKeysJson)` | `MarketBalance` |
[x] | `Filecoin.StateMarketDeals`                          | `(TipsetKeysJson)` | `HashMap<String, MarketDeal>` |
[ ] | `Filecoin.StateMarketParticipants`                   | `-` | `-` |
[ ] | `Filecoin.StateMarketStorageDeal`                    | `-` | `-` |
[ ] | `Filecoin.StateMinerActiveSectors`                   | `-` | `-` |
[ ] | `Filecoin.StateMinerAvailableBalance`                | `-` | `-` |
[x] | `Filecoin.StateMinerDeadlines`                       | `(AddressJson, TipsetKeysJson)` | `Vec<Deadline>` |
[x] | `Filecoin.StateMinerFaults`                          | `(AddressJson, TipsetKeysJson)` | `BitFieldJson` |
[x] | `Filecoin.StateMinerInfo`                            | `(AddressJson, TipsetKeysJson)` | `MinerInfo` |
[x] | `Filecoin.StateMinerInitialPledgeCollateral`         | `(AddressJson, SectorPreCommitInfo, TipsetKeysJson)` | `String` |
[x] | `Filecoin.StateMinerPartitions`                      | `(AddressJson, u64, TipsetKeysJson)` | `Vec<Partition>` |
[ ] | `Filecoin.StateMinerPower`                           | `-` | `-` |
[x] | `Filecoin.StateMinerPreCommitDepositForPower`        | `(AddressJson, SectorPreCommitInfo, TipsetKeysJson)` | `String` |
[x] | `Filecoin.StateMinerProvingDeadline`                 | `(AddressJson, TipsetKeysJson)` | `DeadlineInfo` |
[x] | `Filecoin.StateMinerRecoveries`                      | `(AddressJson, TipsetKeysJson)` | `BitFieldJson` |
[x] | `Filecoin.StateMinerSectorAllocated`                 | `(AddressJson, u64, TipsetKeysJson)` | `bool` |
[ ] | `Filecoin.StateMinerSectorCount`                     | `-` | `-` |
[x] | `Filecoin.StateMinerSectors`                         | `(AddressJson, BitFieldJson, TipsetKeysJson)` | `Vec<SectorOnChainInfo>` |
[x] | `Filecoin.StateNetworkName`                          | `()` | `String` |
[x] | `Filecoin.StateNetworkVersion`                       | `(TipsetKeysJson)` | `NetworkVersion` |
[ ] | `Filecoin.StateReadState`                            | `-` | `-` |
[x] | `Filecoin.StateReplay`                               | `(CidJson, TipsetKeysJson)` | `InvocResult` |
[ ] | `Filecoin.StateSearchMsg`                            | `-` | `-` |
[ ] | `Filecoin.StateSectorExpiration`                     | `-` | `-` |
[x] | `Filecoin.StateSectorGetInfo`                        | `(AddressJson, SectorNumber, TipsetKeysJson)` | `Option<SectorOnChainInfo>` |
[ ] | `Filecoin.StateSectorPartition`                      | `-` | `-` |
[ ] | `Filecoin.StateSectorPreCommitInfo`                  | `-` | `-` |
[ ] | `Filecoin.StateVMCirculatingSupplyInternal`          | `-` | `-` |
[ ] | `Filecoin.StateVerifiedClientStatus`                 | `-` | `-` |
[ ] | `Filecoin.StateVerifiedRegistryRootKey`              | `-` | `-` |
[ ] | `Filecoin.StateVerifierStatus`                       | `-` | `-` |
[x] | `Filecoin.StateWaitMsg`                              | `(CidJson, i64)` | `MessageLookup` |
[x] | `Filecoin.SyncCheckBad`                              | `(CidJson)` | `String` |
[ ] | `Filecoin.SyncCheckpoint`                            | `-` | `-` |
[ ] | `Filecoin.SyncMarkBad`                               | `-` | `-` |
[x] | `Filecoin.SyncState`                                 | `()` | `RPCSyncState` |
[ ] | `Filecoin.SyncSubmitBlock`                           | `-` | `-` |
[ ] | `Filecoin.SyncUnmarkAllBad`                          | `-` | `-` |
[ ] | `Filecoin.SyncUnmarkBad`                             | `-` | `-` |
[ ] | `Filecoin.SyncValidateTipset`                        | `-` | `-` |
[x] | `Filecoin.WalletBalance`                             | `(String)` | `String` |
[x] | `Filecoin.WalletDefaultAddress`                      | `()` | `String` |
[ ] | `Filecoin.WalletDelete`                              | `-` | `-` |
[x] | `Filecoin.WalletExport`                              | `(String)` | `KeyInfoJson` |
[x] | `Filecoin.WalletHas`                                 | `(String)` | `bool` |
[x] | `Filecoin.WalletImport`                              | `()` | `String` |
[x] | `Filecoin.WalletList`                                | `()` | `Vec<AddressJson>` |
[x] | `Filecoin.WalletNew`                                 | `(SignatureTypeJson)` | `String` |
[ ] | `Filecoin.WalletSetDefault`                          | `-` | `-` |
[x] | `Filecoin.WalletSign`                                | `(AddressJson, Vec<u8>)` | `SignatureJson` |
[x] | `Filecoin.WalletSignMessage`                         | `(String, UnsignedMessageJson)` | `SignedMessageJson` |
[ ] | `Filecoin.WalletValidateAddress`                     | `-` | `-` |
[x] | `Filecoin.WalletVerify`                              | `(String, String, SignatureJson)` | `bool` |

## Help & Contributions

If there's a particular API that's needed that we're missing, be sure to let us know.

Feel free to reach out in #fil-forest-help in the [Filecoin Slack](https://docs.filecoin.io/community/chat-and-discussion-forums/), file a GitHub issue, or contribute a pull request.
