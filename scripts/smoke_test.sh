#!/bin/bash

# set temporary configuration
FOREST_CONFIG="/tmp/temp_config.toml"
echo "encrypt_keystore=false" > $FOREST_CONFIG

# start forest daemon
RUST_LOG=off forest -c $FOREST_CONFIG > /dev/null &

# get token and multiaddr info
FULL_ADDR=$(forest -c $FOREST_CONFIG auth api-info -p admin)

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

# extract token from auth api-info output
TOKEN="$(cut -d':' -f1 <<< $FULL_ADDR)"
TOKEN=${TOKEN#"FULLNODE_API_INFO=\""}

# set headers for http requests 
AUTH_HEADER="Authorization: Bearer ${TOKEN}"
CONTENT_TYPE_HEADER="Content-Type: application/json-rpc"

# Wallet
RPC_ENDPOINTS=("WalletList" "WalletBalance" "WalletDefaultAddress" "WalletExport" "WalletHas" "WalletImport" "WalletNew")
RPC_ENDPOINTS+=("WalletSetDefault" "WalletSign" "WalletVerify")

# Sync
RPC_ENDPOINTS+=("SyncCheckBad" "SyncMarkBad" "SyncState" "SyncSubmitBlock")

# Message Pool
RPC_ENDPOINTS+=("MpoolEstimateGasPrice" "MpoolGetNonce" "MpoolPending" "MpoolPush" "MpoolPushMessage" "MpoolSelect")

# Chain
RPC_ENDPOINTS+=("ChainGetMessage" "ChainReadObj" "ChainHasObj" "ChainGetBlockMessages" "ChainGetTipsetByHeight" "ChainGetGenesis")
RPC_ENDPOINTS+=("ChainHead" "ChainHeadSubscription" "ChainNotify" "ChainTipSetWeight" "ChainGetBlock" "ChainGetTipSet")
RPC_ENDPOINTS+=("ChainGetRandomnessFromTickets" "ChainGetRandomnessFromBeacon")

# Auth
RPC_ENDPOINTS+=("AuthNew" "AuthVerify")

# Net
RPC_ENDPOINTS+=("NetAddrsListen" "NetPeers" "NetConnect" "NetDisconnect")

# Common
RPC_ENDPOINTS+=("Version")

# Gas
RPC_ENDPOINTS+=("GasEstimateFeeCap" "GasEstimateGasPremium" "GasEstimateGasLimit" "GasEstimateMessageGas")

# State
RPC_ENDPOINTS+=("StateMinerSectors" "StateCall" "StateMinerDeadlines" "StateSectorPrecommitInfo" "StateMinerInfo")
RPC_ENDPOINTS+=("StateSectorGetInfo" "StateMinerProvingDeadline" "StateMinerFaults" "StateAllMinerFaults")
RPC_ENDPOINTS+=("StateMinerRecoveries" "StateMinerPartitions" "StateReplay" "StateNetworkName" "StateNetworkVersion")
RPC_ENDPOINTS+=("StateGetActor" "StateAccountKey" "StateLookupId" "StateMarketBalance" "StateMarketDeals")
RPC_ENDPOINTS+=("StateGetReceipt" "StateWaitMsg" "MinerCreateBlock" "StateMinerSectorAllocated" "StateMinerPreCommitDepositForPower")
RPC_ENDPOINTS+=("StateMinerInitialPledgeCollateral" "MinerGetBaseInfo")


# send requests programmatically
for endpoint in ${RPC_ENDPOINTS[@]}; do
    METHOD="Filecoin.${endpoint}"
    REQUEST_BODY="{\"jsonrpc\": \"2.0\", \"method\": \"$METHOD\", \"params\":[], \"id\": 0}"

    RESPONSE_CODE=$(curl -w "%{http_code}" -s -o /dev/null -X POST -H "$CONTENT_TYPE_HEADER" -H "$AUTH_HEADER" -d "$REQUEST_BODY" http://127.0.0.1:1234/rpc/v0)

    # a response is a response and considered a passing test
    # we are not passing params to endpoints so some methods will fail due to lack of params
    if [ $RESPONSE_CODE = '200' ] || [ $RESPONSE_CODE = '500' ]; then
        echo -e "${METHOD} ${GREEN} OK ${NC}"
    else
        echo -e "${METHOD} ${RED} FAIL ${RESPONSE_CODE} ${NC}"
    fi

done

# Kill forest daemon
ps -ef | grep forest | grep -v grep | awk '{print $2}' | xargs kill
