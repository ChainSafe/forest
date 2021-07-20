#!/bin/bash

if [ "$1" == "help" ]; then
    echo "Smoke Test\nRun forest node; set FULLNODE_API_INFO; smoke test CLI"
    exit
fi

ADDR=$FULLNODE_API_INFO

if [ -z "$ADDR" ]; then
    echo "FULLNODE_API_INFO is not set. Use $0 help for usage"
    exit
fi

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

TOKEN="$(cut -d':' -f1 <<< $ADDR)"

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
RPC_ENDPOINTS+=("STATE_MINER_SECTORS" "STATE_CALL" "STATE_MINER_DEADLINES" "STATE_SECTOR_PRECOMMIT_INFO" "STATE_MINER_INFO")
RPC_ENDPOINTS+=("STATE_SECTOR_GET_INFO" "STATE_MINER_PROVING_DEADLINE" "STATE_MINER_FAULTS" "STATE_MINER_ALL_FAULTS")
RPC_ENDPOINTS+=("STATE_MINER_RECOVERIES" "STATE_MINER_PARTITIONS" "STATE_REPLAY" "STATE_NETWORK_NAME" "STATE_NETWORK_VERSION")
RPC_ENDPOINTS+=("STATE_GET_ACTOR" "STATE_ACCOUNT_KEY" "STATE_LOOKUP_ID" "STATE_MARKET_BALANCE" "STATE_MARKET_DEALS")
RPC_ENDPOINTS+=("STATE_GET_RECEIPT" "STATE_WAIT_MSG" "MINER_CREATE_BLOCK" "STATE_MINER_SECTOR_ALLOCATED" "STATE_MINER_PRECOMMIT_DEPOSIT_FOR_POWER")
RPC_ENDPOINTS+=("STATE_MINER_INITIAL_PLEDGE_COLLATERAL" "MINER_GET_BASE_INFO")


for endpoint in ${RPC_ENDPOINTS[@]}; do
    METHOD="Filecoin.${endpoint}"
    REQUEST_BODY="{\"jsonrpc\": \"2.0\", \"method\": \"$METHOD\", \"params\":[], \"id\": 0}"

    RESPONSE_CODE=$(curl -w "%{http_code}" -s -o /dev/null -X POST -H "$CONTENT_TYPE_HEADER" -H "$AUTH_HEADER" -d "$REQUEST_BODY" http://127.0.0.1:1234/rpc/v0)

    if [ $RESPONSE_CODE = '200' ]; then
        echo -e "${METHOD} ${GREEN} OK ${NC}"
    else
        echo -e "${METHOD} ${RED} FAIL ${NC}"
    fi

done
