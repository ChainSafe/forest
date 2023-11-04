pub trait Api {
    fn ChainHasObj(
        arg0: <::cid::Cid as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <::std::primitive::bool as crate::lotus_json::HasLotusJson>::LotusJson;
    fn ChainReadObj(
        arg0: <::cid::Cid as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <::std::vec::Vec<
        ::std::primitive::u8,
    > as crate::lotus_json::HasLotusJson>::LotusJson;
    fn ChainTipSetWeight(
        arg0: <crate::blocks::TipsetKeys as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <::num::BigInt as crate::lotus_json::HasLotusJson>::LotusJson;
    fn ClientHasLocal(
        arg0: <::cid::Cid as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <::std::primitive::bool as crate::lotus_json::HasLotusJson>::LotusJson;
    fn MarketAddBalance(
        arg0: <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson,
        arg1: <::num::BigInt as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <::cid::Cid as crate::lotus_json::HasLotusJson>::LotusJson;
    fn MarketGetReserved(
        arg0: <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <::num::BigInt as crate::lotus_json::HasLotusJson>::LotusJson;
    fn MarketReserveFunds(
        arg0: <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson,
        arg1: <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson,
        arg2: <::num::BigInt as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <::cid::Cid as crate::lotus_json::HasLotusJson>::LotusJson;
    fn MarketWithdraw(
        arg0: <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson,
        arg1: <::num::BigInt as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <::cid::Cid as crate::lotus_json::HasLotusJson>::LotusJson;
    fn MpoolGetNonce(
        arg0: <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <::std::primitive::u64 as crate::lotus_json::HasLotusJson>::LotusJson;
    fn MsigGetAvailableBalance(
        arg0: <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson,
        arg1: <crate::blocks::TipsetKeys as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <::num::BigInt as crate::lotus_json::HasLotusJson>::LotusJson;
    fn MsigGetVested(
        arg0: <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson,
        arg1: <crate::blocks::TipsetKeys as crate::lotus_json::HasLotusJson>::LotusJson,
        arg2: <crate::blocks::TipsetKeys as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <::num::BigInt as crate::lotus_json::HasLotusJson>::LotusJson;
    fn NetListening() -> <::std::primitive::bool as crate::lotus_json::HasLotusJson>::LotusJson;
    fn PaychAllocateLane(
        arg0: <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <::std::primitive::u64 as crate::lotus_json::HasLotusJson>::LotusJson;
    fn PaychCollect(
        arg0: <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <::cid::Cid as crate::lotus_json::HasLotusJson>::LotusJson;
    fn PaychGetWaitReady(
        arg0: <::cid::Cid as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson;
    fn PaychSettle(
        arg0: <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <::cid::Cid as crate::lotus_json::HasLotusJson>::LotusJson;
    fn StateAccountKey(
        arg0: <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson,
        arg1: <crate::blocks::TipsetKeys as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson;
    fn StateLookupID(
        arg0: <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson,
        arg1: <crate::blocks::TipsetKeys as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson;
    fn StateLookupRobustAddress(
        arg0: <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson,
        arg1: <crate::blocks::TipsetKeys as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson;
    fn StateMinerAvailableBalance(
        arg0: <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson,
        arg1: <crate::blocks::TipsetKeys as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <::num::BigInt as crate::lotus_json::HasLotusJson>::LotusJson;
    fn StateVerifiedRegistryRootKey(
        arg0: <crate::blocks::TipsetKeys as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson;
    fn SyncValidateTipset(
        arg0: <crate::blocks::TipsetKeys as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <::std::primitive::bool as crate::lotus_json::HasLotusJson>::LotusJson;
    fn WalletBalance(
        arg0: <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <::num::BigInt as crate::lotus_json::HasLotusJson>::LotusJson;
    fn WalletDefaultAddress() -> <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson;
    fn WalletHas(
        arg0: <crate::shim::address::Address as crate::lotus_json::HasLotusJson>::LotusJson,
    ) -> <::std::primitive::bool as crate::lotus_json::HasLotusJson>::LotusJson;
}

