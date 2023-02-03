// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

console.log("Welcome to the Forest Javascript console!\n\nTo exit, press ctrl-d or type :quit");

// // Load filecoin module
// let filecoin = require("./filecoin.js");

function showPeers() {
    let ids = netPeers().map((x) => x.ID).sort();
    for (var i = 0; i < ids.length; i++) {
        let id = ids[i];
        console.log(`${i}:\t${id}`);
    }
    console.log("");
}

function getPeer(peerID) {
    return netPeers().find((x) => x.ID == peerID);
}

function disconnectPeers(count) {
    let ids = netPeers().map((x) => x.ID).sort();
    // clamp count
    let new_count = Math.min(count, ids.length);
    for (var i = 0; i < new_count; i++) {
        netDisconnect(ids[i]);
    }
} 

function isPeerConnected(peerID) {
    return netPeers().some((x) => x.ID == peerID);
}

function showWallet() {
    let addrs = walletList();
    let defaultAddr = walletDefaultAddress();
    let buffer = "";
    buffer = buffer.concat("Address                                         Balance\n");
    for (var i = 0; i < addrs.length; i++) {
        const isDefault = (defaultAddr == addrs[i]);
        if (isDefault) {
            buffer += "\033[1m";
        }
        buffer = buffer.concat(addrs[i], "       ", walletBalance(addrs[i]), " attoFIL\n");
        if (isDefault) {
            buffer += "\033[0m";
        }
    }
    console.log(buffer);
}

function showSyncStatus() {
    let stage = syncStatus().ActiveSyncs[0].Stage;
    let height = syncStatus().ActiveSyncs[0].Epoch;
    let result =
`sync status:
Stage:  ${stage}
Height: ${height}
`;
    console.log(result);
}

function sendFIL(to, amount) {
    let from = walletDefaultAddress();
    return sendMessage(from, to, amount.toString());
}
