// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

console.log("Welcome to the Forest Javascript console!\n\nTo exit, press ctrl-d or type :quit");

// You can easily modify the script and see changes by running: `$ ./target/release/forest-cli --token $TOKEN attach --jspath ./forest/cli/src/cli`

// // Load filecoin module
// let filecoin = require("./filecoin.js");

function showPeers() {
    let ids = netPeers().map((x) => x.ID).sort();
    let buffer = "";
    for (var i = 0; i < ids.length; i++) {
        let id = ids[i];
        buffer += `${i}:\t${id}\n`;
    }
    console.log(buffer);
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
    let buffer = "Address                                         Balance\n";
    for (var i = 0; i < addrs.length; i++) {
        const addr = addrs[i];
        const line = `${addr}       ${walletBalance(addr)} attoFIL\n`;
        if (addr == defaultAddr) {
            buffer = buffer.concat("\033[1m", line, "\033[0m")
        } else {
            buffer = buffer.concat(line);
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
