#! /bin/bash

TIPSET_CID=$(curl -X POST \
    --header 'Content-Type: application/json' \
    --data '{"jsonrpc":"2.0","method":"Filecoin.SyncState","params":null,"id":null}' \
    http://localhost:1234/rpc/v0 \
    | jq -r --arg SLASH "/" .result.ActiveSyncs[0].Base.Cids[0][$SLASH])
echo "TIPSET_CID: $TIPSET_CID"
cargo run -p forest_statediff --release -- chain --chain calibnet $TIPSET_CID $TIPSET_CID
