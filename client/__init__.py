from .definitions import *

import typing
import abc
import json

RpcSyncState = RPCSyncState


class Client(abc.ABC):
    @abc.abstractmethod
    def call(self, method_name: str, params: list[str]) -> str: raise NotImplementedError
    def AuthNew(self, params: AuthNewParams) -> VecU8LotusJson:
        return VecU8LotusJson.model_validate_json(self.call("Filecoin.AuthNew", [params.model_dump_json(by_alias=True)]))
    def AuthVerify(self, header_raw: str) -> AllocStringString:
        return AllocStringString.model_validate_json(self.call("Filecoin.AuthVerify", [json.dumps(header_raw)]))
    def BeaconGetEntry(self, first: int) -> BeaconEntryLotusJson:
        return BeaconEntryLotusJson.model_validate_json(self.call("Filecoin.BeaconGetEntry", [json.dumps(first)]))
    def ChainGetMessage(self, msg_cid: CidLotusJsonGenericFor64) -> MessageLotusJson:
        return MessageLotusJson.model_validate_json(self.call("Filecoin.ChainGetMessage", [msg_cid.model_dump_json(by_alias=True)]))
    def ChainGetParentMessages(self, block_cid: CidLotusJsonGenericFor64) -> ForestFilecoinRpcMethodsChainApiMessage:
        return ForestFilecoinRpcMethodsChainApiMessage.model_validate_json(self.call("Filecoin.ChainGetParentMessages", [block_cid.model_dump_json(by_alias=True)]))
    def ChainGetParentReceipts(self, block_cid: CidLotusJsonGenericFor64) -> ForestFilecoinRpcMethodsChainApiReceipt:
        return ForestFilecoinRpcMethodsChainApiReceipt.model_validate_json(self.call("Filecoin.ChainGetParentReceipts", [block_cid.model_dump_json(by_alias=True)]))
    def ChainGetMessagesInTipset(self, tsk: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> ForestFilecoinRpcMethodsChainApiMessage:
        return ForestFilecoinRpcMethodsChainApiMessage.model_validate_json(self.call("Filecoin.ChainGetMessagesInTipset", [tsk.model_dump_json(by_alias=True)]))
    def ChainExport(self, params: ChainExportParams) -> typing.Any:
        return json.loads(self.call("Filecoin.ChainExport", [params.model_dump_json(by_alias=True)]))
    def ChainReadObj(self, cid: CidLotusJsonGenericFor64) -> VecU8LotusJson:
        return VecU8LotusJson.model_validate_json(self.call("Filecoin.ChainReadObj", [cid.model_dump_json(by_alias=True)]))
    def ChainHasObj(self, cid: CidLotusJsonGenericFor64) -> bool:
        return json.loads(self.call("Filecoin.ChainHasObj", [cid.model_dump_json(by_alias=True)]))
    def ChainGetBlockMessages(self, cid: CidLotusJsonGenericFor64) -> BlockMessages:
        return BlockMessages.model_validate_json(self.call("Filecoin.ChainGetBlockMessages", [cid.model_dump_json(by_alias=True)]))
    def ChainGetPath(self, from_: NonEmptyArrayOfCidLotusJsonGenericFor64, to: NonEmptyArrayOfCidLotusJsonGenericFor64) -> ForestFilecoinRpcMethodsChainPathChangeForestFilecoinBlocksTipsetLotusJsonTipsetLotusJson:
        return ForestFilecoinRpcMethodsChainPathChangeForestFilecoinBlocksTipsetLotusJsonTipsetLotusJson.model_validate_json(self.call("Filecoin.ChainGetPath", [from_.model_dump_json(by_alias=True), to.model_dump_json(by_alias=True)]))
    def ChainGetTipSetByHeight(self, height: int, tsk: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> TipsetLotusJson:
        return TipsetLotusJson.model_validate_json(self.call("Filecoin.ChainGetTipSetByHeight", [json.dumps(height), tsk.model_dump_json(by_alias=True)]))
    def ChainGetTipSetAfterHeight(self, height: int, tsk: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> TipsetLotusJson:
        return TipsetLotusJson.model_validate_json(self.call("Filecoin.ChainGetTipSetAfterHeight", [json.dumps(height), tsk.model_dump_json(by_alias=True)]))
    def ChainGetGenesis(self, ) -> typing.Any:
        return json.loads(self.call("Filecoin.ChainGetGenesis", []))
    def ChainHead(self, ) -> TipsetLotusJson:
        return TipsetLotusJson.model_validate_json(self.call("Filecoin.ChainHead", []))
    def ChainGetBlock(self, cid: CidLotusJsonGenericFor64) -> ForestFilecoinLotusJsonBlockHeaderBlockHeaderLotusJson:
        return ForestFilecoinLotusJsonBlockHeaderBlockHeaderLotusJson.model_validate_json(self.call("Filecoin.ChainGetBlock", [cid.model_dump_json(by_alias=True)]))
    def ChainGetTipSet(self, tsk: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> TipsetLotusJson:
        return TipsetLotusJson.model_validate_json(self.call("Filecoin.ChainGetTipSet", [tsk.model_dump_json(by_alias=True)]))
    def ChainSetHead(self, tsk: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> None:
        return json.loads(self.call("Filecoin.ChainSetHead", [tsk.model_dump_json(by_alias=True)]))
    def ChainGetMinBaseFee(self, lookback: int) -> str:
        return json.loads(self.call("Filecoin.ChainGetMinBaseFee", [json.dumps(lookback)]))
    def ChainTipSetWeight(self, tsk: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> String:
        return String.model_validate_json(self.call("Filecoin.ChainTipSetWeight", [tsk.model_dump_json(by_alias=True)]))
    def Session(self, ) -> str:
        return json.loads(self.call("Filecoin.Session", []))
    def Version(self, ) -> PublicVersion:
        return PublicVersion.model_validate_json(self.call("Filecoin.Version", []))
    def Shutdown(self, ) -> None:
        return json.loads(self.call("Filecoin.Shutdown", []))
    def StartTime(self, ) -> str:
        return json.loads(self.call("Filecoin.StartTime", []))
    def GasEstimateGasLimit(self, msg: MessageLotusJson, tsk: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> int:
        return json.loads(self.call("Filecoin.GasEstimateGasLimit", [msg.model_dump_json(by_alias=True), tsk.model_dump_json(by_alias=True)]))
    def GasEstimateMessageGas(self, msg: MessageLotusJson, spec: typing.Any, tsk: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> MessageLotusJson:
        return MessageLotusJson.model_validate_json(self.call("Filecoin.GasEstimateMessageGas", [msg.model_dump_json(by_alias=True), json.dumps(spec), tsk.model_dump_json(by_alias=True)]))
    def GasEstimateFeeCap(self, message: MessageLotusJson, max_queue_blocks: int, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> str:
        return json.loads(self.call("Filecoin.GasEstimateFeeCap", [message.model_dump_json(by_alias=True), json.dumps(max_queue_blocks), tipset_key.model_dump_json(by_alias=True)]))
    def GasEstimateGasPremium(self, nblocksincl: int, sender: String, gas_limit: int, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> str:
        return json.loads(self.call("Filecoin.GasEstimateGasPremium", [json.dumps(nblocksincl), sender.model_dump_json(by_alias=True), json.dumps(gas_limit), tipset_key.model_dump_json(by_alias=True)]))
    def MpoolGetNonce(self, address: String) -> int:
        return json.loads(self.call("Filecoin.MpoolGetNonce", [address.model_dump_json(by_alias=True)]))
    def MpoolPending(self, tsk: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> ForestFilecoinLotusJsonSignedMessageSignedMessageLotusJson:
        return ForestFilecoinLotusJsonSignedMessageSignedMessageLotusJson.model_validate_json(self.call("Filecoin.MpoolPending", [tsk.model_dump_json(by_alias=True)]))
    def MpoolSelect(self, tsk: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64, tq: float) -> ForestFilecoinLotusJsonSignedMessageSignedMessageLotusJson:
        return ForestFilecoinLotusJsonSignedMessageSignedMessageLotusJson.model_validate_json(self.call("Filecoin.MpoolSelect", [tsk.model_dump_json(by_alias=True), json.dumps(tq)]))
    def MpoolPush(self, msg: ForestFilecoinLotusJsonSignedMessageSignedMessageLotusJson) -> CidLotusJsonGenericFor64:
        return CidLotusJsonGenericFor64.model_validate_json(self.call("Filecoin.MpoolPush", [msg.model_dump_json(by_alias=True)]))
    def MpoolPushMessage(self, usmg: MessageLotusJson, spec: typing.Any) -> ForestFilecoinLotusJsonSignedMessageSignedMessageLotusJson:
        return ForestFilecoinLotusJsonSignedMessageSignedMessageLotusJson.model_validate_json(self.call("Filecoin.MpoolPushMessage", [usmg.model_dump_json(by_alias=True), json.dumps(spec)]))
    def NetAddrsListen(self, ) -> AddrInfo:
        return AddrInfo.model_validate_json(self.call("Filecoin.NetAddrsListen", []))
    def NetPeers(self, ) -> AddrInfo:
        return AddrInfo.model_validate_json(self.call("Filecoin.NetPeers", []))
    def NetListening(self, ) -> bool:
        return json.loads(self.call("Filecoin.NetListening", []))
    def NetInfo(self, ) -> NetInfoResult:
        return NetInfoResult.model_validate_json(self.call("Forest.NetInfo", []))
    def NetConnect(self, info: AddrInfo) -> None:
        return json.loads(self.call("Filecoin.NetConnect", [info.model_dump_json(by_alias=True)]))
    def NetDisconnect(self, id: str) -> None:
        return json.loads(self.call("Filecoin.NetDisconnect", [json.dumps(id)]))
    def NetAgentVersion(self, id: str) -> str:
        return json.loads(self.call("Filecoin.NetAgentVersion", [json.dumps(id)]))
    def NetAutoNatStatus(self, ) -> NatStatusResult:
        return NatStatusResult.model_validate_json(self.call("Filecoin.NetAutoNatStatus", []))
    def NetVersion(self, ) -> str:
        return json.loads(self.call("Filecoin.NetVersion", []))
    def MinerGetBaseInfo(self, address: String, epoch: int, tsk: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> typing.Any:
        return json.loads(self.call("Filecoin.MinerGetBaseInfo", [address.model_dump_json(by_alias=True), json.dumps(epoch), tsk.model_dump_json(by_alias=True)]))
    def StateCall(self, message: MessageLotusJson, tsk: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> ApiInvocResult:
        return ApiInvocResult.model_validate_json(self.call("Filecoin.StateCall", [message.model_dump_json(by_alias=True), tsk.model_dump_json(by_alias=True)]))
    def StateGetBeaconEntry(self, epoch: int) -> BeaconEntryLotusJson:
        return BeaconEntryLotusJson.model_validate_json(self.call("Filecoin.StateGetBeaconEntry", [json.dumps(epoch)]))
    def StateListMessages(self, message_filter: MessageFilter, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64, max_height: int) -> ForestFilecoinLotusJsonCidCidLotusJsonGeneric64:
        return ForestFilecoinLotusJsonCidCidLotusJsonGeneric64.model_validate_json(self.call("Filecoin.StateListMessages", [message_filter.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True), json.dumps(max_height)]))
    def StateNetworkName(self, ) -> str:
        return json.loads(self.call("Filecoin.StateNetworkName", []))
    def StateReplay(self, cid: CidLotusJsonGenericFor64, tsk: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> InvocResult:
        return InvocResult.model_validate_json(self.call("Filecoin.StateReplay", [cid.model_dump_json(by_alias=True), tsk.model_dump_json(by_alias=True)]))
    def StateSectorGetInfo(self, miner_address: String, sector_number: int, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> SectorOnChainInfo:
        return SectorOnChainInfo.model_validate_json(self.call("Filecoin.StateSectorGetInfo", [miner_address.model_dump_json(by_alias=True), json.dumps(sector_number), tipset_key.model_dump_json(by_alias=True)]))
    def StateSectorPreCommitInfo(self, miner_address: String, sector_number: int, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> SectorPreCommitOnChainInfo:
        return SectorPreCommitOnChainInfo.model_validate_json(self.call("Filecoin.StateSectorPreCommitInfo", [miner_address.model_dump_json(by_alias=True), json.dumps(sector_number), tipset_key.model_dump_json(by_alias=True)]))
    def StateAccountKey(self, address: String, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> String:
        return String.model_validate_json(self.call("Filecoin.StateAccountKey", [address.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True)]))
    def StateLookupID(self, address: String, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> String:
        return String.model_validate_json(self.call("Filecoin.StateLookupID", [address.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True)]))
    def StateGetActor(self, address: String, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> typing.Any:
        return json.loads(self.call("Filecoin.StateGetActor", [address.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True)]))
    def StateMinerInfo(self, address: String, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> MinerInfoLotusJson:
        return MinerInfoLotusJson.model_validate_json(self.call("Filecoin.StateMinerInfo", [address.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True)]))
    def StateMinerActiveSectors(self, address: String, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> SectorOnChainInfo:
        return SectorOnChainInfo.model_validate_json(self.call("Filecoin.StateMinerActiveSectors", [address.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True)]))
    def StateMinerPartitions(self, address: String, deadline_index: int, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> ForestFilecoinRpcTypesMinerPartitions:
        return ForestFilecoinRpcTypesMinerPartitions.model_validate_json(self.call("Filecoin.StateMinerPartitions", [address.model_dump_json(by_alias=True), json.dumps(deadline_index), tipset_key.model_dump_json(by_alias=True)]))
    def StateMinerSectors(self, address: String, sectors: typing.Any, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> SectorOnChainInfo:
        return SectorOnChainInfo.model_validate_json(self.call("Filecoin.StateMinerSectors", [address.model_dump_json(by_alias=True), json.dumps(sectors), tipset_key.model_dump_json(by_alias=True)]))
    def StateMinerSectorCount(self, address: String, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> MinerSectors:
        return MinerSectors.model_validate_json(self.call("Filecoin.StateMinerSectorCount", [address.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True)]))
    def StateMinerPower(self, address: String, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> MinerPowerLotusJson:
        return MinerPowerLotusJson.model_validate_json(self.call("Filecoin.StateMinerPower", [address.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True)]))
    def StateMinerDeadlines(self, address: String, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> ForestFilecoinRpcTypesApiDeadline:
        return ForestFilecoinRpcTypesApiDeadline.model_validate_json(self.call("Filecoin.StateMinerDeadlines", [address.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True)]))
    def StateMinerProvingDeadline(self, address: String, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> ApiDeadlineInfo:
        return ApiDeadlineInfo.model_validate_json(self.call("Filecoin.StateMinerProvingDeadline", [address.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True)]))
    def StateMinerFaults(self, address: String, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> BitFieldLotusJson:
        return BitFieldLotusJson.model_validate_json(self.call("Filecoin.StateMinerFaults", [address.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True)]))
    def StateMinerRecoveries(self, address: String, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> BitFieldLotusJson:
        return BitFieldLotusJson.model_validate_json(self.call("Filecoin.StateMinerRecoveries", [address.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True)]))
    def StateMinerAvailableBalance(self, address: String, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> String:
        return String.model_validate_json(self.call("Filecoin.StateMinerAvailableBalance", [address.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True)]))
    def StateMinerInitialPledgeCollateral(self, address: String, sector_pre_commit_info: SectorPreCommitInfo2, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> String:
        return String.model_validate_json(self.call("Filecoin.StateMinerInitialPledgeCollateral", [address.model_dump_json(by_alias=True), sector_pre_commit_info.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True)]))
    def StateGetReceipt(self, cid: CidLotusJsonGenericFor64, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> ReceiptLotusJson:
        return ReceiptLotusJson.model_validate_json(self.call("Filecoin.StateGetReceipt", [cid.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True)]))
    def StateGetRandomnessFromTickets(self, personalization: int, rand_epoch: int, entropy: VecU8LotusJson, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> VecU8LotusJson:
        return VecU8LotusJson.model_validate_json(self.call("Filecoin.StateGetRandomnessFromTickets", [json.dumps(personalization), json.dumps(rand_epoch), entropy.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True)]))
    def StateGetRandomnessFromBeacon(self, personalization: int, rand_epoch: int, entropy: VecU8LotusJson, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> VecU8LotusJson:
        return VecU8LotusJson.model_validate_json(self.call("Filecoin.StateGetRandomnessFromBeacon", [json.dumps(personalization), json.dumps(rand_epoch), entropy.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True)]))
    def StateReadState(self, address: String, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> ApiActorState:
        return ApiActorState.model_validate_json(self.call("Filecoin.StateReadState", [address.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True)]))
    def StateCirculatingSupply(self, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> String:
        return String.model_validate_json(self.call("Filecoin.StateCirculatingSupply", [tipset_key.model_dump_json(by_alias=True)]))
    def MsigGetAvailableBalance(self, address: String, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> String:
        return String.model_validate_json(self.call("Filecoin.MsigGetAvailableBalance", [address.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True)]))
    def MsigGetPending(self, address: String, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> ForestFilecoinRpcTypesTransaction:
        return ForestFilecoinRpcTypesTransaction.model_validate_json(self.call("Filecoin.MsigGetPending", [address.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True)]))
    def StateVerifiedClientStatus(self, address: String, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> typing.Any:
        return json.loads(self.call("Filecoin.StateVerifiedClientStatus", [address.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True)]))
    def StateVMCirculatingSupplyInternal(self, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> CirculatingSupply:
        return CirculatingSupply.model_validate_json(self.call("Filecoin.StateVMCirculatingSupplyInternal", [tipset_key.model_dump_json(by_alias=True)]))
    def StateListMiners(self, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> ForestFilecoinLotusJsonStringifyForestFilecoinShimAddressAddress:
        return ForestFilecoinLotusJsonStringifyForestFilecoinShimAddressAddress.model_validate_json(self.call("Filecoin.StateListMiners", [tipset_key.model_dump_json(by_alias=True)]))
    def StateNetworkVersion(self, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> int:
        return json.loads(self.call("Filecoin.StateNetworkVersion", [tipset_key.model_dump_json(by_alias=True)]))
    def StateMarketBalance(self, address: String, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> MarketBalance:
        return MarketBalance.model_validate_json(self.call("Filecoin.StateMarketBalance", [address.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True)]))
    def StateMarketDeals(self, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> typing.Any:
        return json.loads(self.call("Filecoin.StateMarketDeals", [tipset_key.model_dump_json(by_alias=True)]))
    def StateDealProviderCollateralBounds(self, size: int, verified: bool, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> DealCollateralBounds:
        return DealCollateralBounds.model_validate_json(self.call("Filecoin.StateDealProviderCollateralBounds", [json.dumps(size), json.dumps(verified), tipset_key.model_dump_json(by_alias=True)]))
    def StateMarketStorageDeal(self, deal_id: int, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> ApiMarketDeal:
        return ApiMarketDeal.model_validate_json(self.call("Filecoin.StateMarketStorageDeal", [json.dumps(deal_id), tipset_key.model_dump_json(by_alias=True)]))
    def StateWaitMsg(self, message_cid: CidLotusJsonGenericFor64, confidence: int) -> MessageLookup:
        return MessageLookup.model_validate_json(self.call("Filecoin.StateWaitMsg", [message_cid.model_dump_json(by_alias=True), json.dumps(confidence)]))
    def StateSearchMsg(self, message_cid: CidLotusJsonGenericFor64) -> MessageLookup:
        return MessageLookup.model_validate_json(self.call("Filecoin.StateSearchMsg", [message_cid.model_dump_json(by_alias=True)]))
    def StateSearchMsgLimited(self, message_cid: CidLotusJsonGenericFor64, look_back_limit: int) -> MessageLookup:
        return MessageLookup.model_validate_json(self.call("Filecoin.StateSearchMsgLimited", [message_cid.model_dump_json(by_alias=True), json.dumps(look_back_limit)]))
    def StateFetchRoot(self, root_cid: CidLotusJsonGenericFor64, save_to_file: typing.Any) -> str:
        return json.loads(self.call("Filecoin.StateFetchRoot", [root_cid.model_dump_json(by_alias=True), json.dumps(save_to_file)]))
    def StateMinerPreCommitDepositForPower(self, address: String, sector_pre_commit_info: SectorPreCommitInfo2, tipset_key: ForestFilecoinLotusJsonCidCidLotusJsonGeneric64) -> String:
        return String.model_validate_json(self.call("Filecoin.StateMinerPreCommitDepositForPower", [address.model_dump_json(by_alias=True), sector_pre_commit_info.model_dump_json(by_alias=True), tipset_key.model_dump_json(by_alias=True)]))
    def NodeStatus(self, ) -> NodeStatusResult:
        return NodeStatusResult.model_validate_json(self.call("Filecoin.NodeStatus", []))
    def SyncCheckBad(self, cid: CidLotusJsonGenericFor64) -> str:
        return json.loads(self.call("Filecoin.SyncCheckBad", [cid.model_dump_json(by_alias=True)]))
    def SyncMarkBad(self, cid: CidLotusJsonGenericFor64) -> None:
        return json.loads(self.call("Filecoin.SyncMarkBad", [cid.model_dump_json(by_alias=True)]))
    def SyncState(self, ) -> RpcSyncState:
        return RpcSyncState.model_validate_json(self.call("Filecoin.SyncState", []))
    def SyncSubmitBlock(self, blk: GossipBlockLotusJson) -> None:
        return json.loads(self.call("Filecoin.SyncSubmitBlock", [blk.model_dump_json(by_alias=True)]))
    def WalletBalance(self, address: String) -> String:
        return String.model_validate_json(self.call("Filecoin.WalletBalance", [address.model_dump_json(by_alias=True)]))
    def WalletDefaultAddress(self, ) -> typing.Any:
        return json.loads(self.call("Filecoin.WalletDefaultAddress", []))
    def WalletExport(self, address: String) -> KeyInfoLotusJson:
        return KeyInfoLotusJson.model_validate_json(self.call("Filecoin.WalletExport", [address.model_dump_json(by_alias=True)]))
    def WalletHas(self, address: String) -> bool:
        return json.loads(self.call("Filecoin.WalletHas", [address.model_dump_json(by_alias=True)]))
    def WalletImport(self, key: KeyInfoLotusJson) -> String:
        return String.model_validate_json(self.call("Filecoin.WalletImport", [key.model_dump_json(by_alias=True)]))
    def WalletList(self, ) -> ForestFilecoinLotusJsonStringifyForestFilecoinShimAddressAddress:
        return ForestFilecoinLotusJsonStringifyForestFilecoinShimAddressAddress.model_validate_json(self.call("Filecoin.WalletList", []))
    def WalletNew(self, signature_type: SignatureTypeLotusJson2) -> String:
        return String.model_validate_json(self.call("Filecoin.WalletNew", [signature_type.model_dump_json(by_alias=True)]))
    def WalletSetDefault(self, address: String) -> None:
        return json.loads(self.call("Filecoin.WalletSetDefault", [address.model_dump_json(by_alias=True)]))
    def WalletSign(self, address: String, message: VecU8LotusJson) -> SignatureLotusJson:
        return SignatureLotusJson.model_validate_json(self.call("Filecoin.WalletSign", [address.model_dump_json(by_alias=True), message.model_dump_json(by_alias=True)]))
    def WalletValidateAddress(self, address: str) -> String:
        return String.model_validate_json(self.call("Filecoin.WalletValidateAddress", [json.dumps(address)]))
    def WalletVerify(self, address: String, message: VecU8LotusJson, signature: SignatureLotusJson) -> bool:
        return json.loads(self.call("Filecoin.WalletVerify", [address.model_dump_json(by_alias=True), message.model_dump_json(by_alias=True), signature.model_dump_json(by_alias=True)]))
    def WalletDelete(self, address: String) -> None:
        return json.loads(self.call("Filecoin.WalletDelete", [address.model_dump_json(by_alias=True)]))
    def Web3ClientVersion(self, ) -> str:
        return json.loads(self.call("Filecoin.Web3ClientVersion", []))
    def EthSyncing(self, ) -> EthSyncingResultLotusJson:
        return EthSyncingResultLotusJson.model_validate_json(self.call("Filecoin.EthSyncing", []))
    def EthAccounts(self, ) -> AllocStringString:
        return AllocStringString.model_validate_json(self.call("Filecoin.EthAccounts", []))
    def EthBlockNumber(self, ) -> str:
        return json.loads(self.call("Filecoin.EthBlockNumber", []))
    def EthChainId(self, ) -> str:
        return json.loads(self.call("Filecoin.EthChainId", []))
    def EthGasPrice(self, ) -> BigInt:
        return BigInt.model_validate_json(self.call("Filecoin.EthGasPrice", []))
    def EthGetBalance(self, address: Address, block_param: str) -> BigInt:
        return BigInt.model_validate_json(self.call("Filecoin.EthGetBalance", [address.model_dump_json(by_alias=True), json.dumps(block_param)]))
    def EthGetBlockByNumber(self, block_param: str, full_tx_info: bool) -> Block:
        return Block.model_validate_json(self.call("Filecoin.EthGetBlockByNumber", [json.dumps(block_param), json.dumps(full_tx_info)]))


class HttpClient(Client):
    
    def __init__(self, url: str, token: str | None = None)-> None:
        self.url = url
        self.token = token

    def __repr__(self) -> str:
        if self.token is not None:
            return f"HttpClient(url={self.url}, token=...)"
        else:
            return f"HttpClient(url={self.url})"

    
    def call(self, method_name: str, params: list[str]) -> str:
        import requests
        req = {
            "jsonrpc": "2.0",
            "id": None,
            "method": method_name,
            "params": [json.loads(it) for it in params]
        }
        http_resp = requests.post(self.url, json=req, auth = f"Bearer {self.token}" if self.token is not None else None)
        http_resp.raise_for_status()
        json_resp = http_resp.json()
        assert isinstance(json_resp, dict)
        if json_resp.get("result") is None:
            raise RuntimeError(json_resp)
        return json.dumps(json_resp["result"])

