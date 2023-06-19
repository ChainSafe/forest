// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

/* global netPeers, netDisconnect, walletList, walletDefaultAddress, walletBalance, syncStatus */
/* global sendMessage */

module.exports = {
  greet: function () {
    console.log(
      "Welcome to the Forest Javascript console!\n\nTo exit, press ctrl-d or type :quit"
    );
  },
  showPeers: function () {
    let ids = netPeers()
      .map((x) => x.ID)
      .sort();
    let buffer = "";
    for (var i = 0; i < ids.length; i++) {
      let id = ids[i];
      buffer += `${i}:\t${id}\n`;
    }
    console.log(buffer);
  },
  getPeer: function (peerID) {
    return netPeers().find((x) => x.ID == peerID);
  },
  disconnectPeers: function (count) {
    let ids = netPeers()
      .map((x) => x.ID)
      .sort();
    // clamp count
    let new_count = Math.min(count, ids.length);
    for (var i = 0; i < new_count; i++) {
      netDisconnect(ids[i]);
    }
  },
  isPeerConnected: function (peerID) {
    return netPeers().some((x) => x.ID == peerID);
  },
  showWallet: function () {
    let addrs = walletList();
    let defaultAddr = walletDefaultAddress();
    let buffer = "Address                                         Balance\n";
    for (var i = 0; i < addrs.length; i++) {
      const addr = addrs[i];
      const line = `${addr}       ${walletBalance(addr)} attoFIL\n`;
      if (addr == defaultAddr) {
        buffer = buffer.concat("\033[1m", line, "\033[0m");
      } else {
        buffer = buffer.concat(line);
      }
    }
    console.log(buffer);
  },
  sendFIL: function (to, amount) {
    let from = walletDefaultAddress();
    return sendMessage(from, to, amount.toString());
  },
  showSyncStatus: function () {
    let stage = syncStatus().ActiveSyncs[0].Stage;
    let height = syncStatus().ActiveSyncs[0].Epoch;
    let result = `sync status:
Stage:  ${stage}
Height: ${height}
`;
    console.log(result);
  },
};
