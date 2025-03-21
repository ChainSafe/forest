//
// The methods here are defined with sample parameters. They are meant to be used on the mainnet and might not work on other networks.
// Also, some of the parameters might stop working at some point. More refinement is needed to make them more robust.
//

export const filecoinChainHead = {
  name: "Filecoin.ChainHead",
  params: [],
};

export const filecoinStateMinerPower = {
  name: "Filecoin.StateMinerPower",
  params: ["t01000", []],
};

export const filecoinStateMinerInfo = {
  name: "Filecoin.StateMinerInfo",
  params: ["t01000", []],
};

export const filecoinStateMarketStorageDeal = {
  name: "Filecoin.StateMarketStorageDeal",
  params: [109704581, []],
};

export const ethChainId = {
  name: "eth_chainId",
  params: [],
};

export const ethCall = {
  name: "eth_call",
  params: [
    {
      data: "0xf8b2cb4f000000000000000000000000cbff24ded1ce6b53712078759233ac8f91ea71b6",
      from: null,
      gas: "0x0",
      gasPrice: "0x0",
      to: "0x0c1d86d34e469770339b53613f3a2343accd62cb",
      value: "0x0",
    },
    "latest",
  ],
};

export const ethGasPrice = {
  name: "eth_gasPrice",
  params: [],
};

export const ethGetBalance = {
  name: "eth_getBalance",
  params: ["0x6743938A48fC8799A5608EF079C53f3cF3B84398", "latest"],
};

//
// Groupings of methods. Either arbitrary or based on data shared by RPC providers.
//

// All methods we have implemented in test scripts so far.
export const allMethods = [
  filecoinChainHead,
  filecoinStateMinerPower,
  filecoinStateMinerInfo,
  filecoinStateMarketStorageDeal,
  ethChainId,
  ethCall,
  ethGasPrice,
  ethGetBalance,
];

// The top 5 methods according to Hubert's metal die roll.
export const top5Methods = [
  filecoinChainHead,
  ethCall,
  ethChainId,
  ethGetBalance,
  // should be `ChainNotify`, but it's a subscription method, so it's tricky to test
  filecoinStateMinerPower,
];
