<style>
.notImp {background-color: #1f1fff; padding: 0 5px;}
.partSupp {background-color: #9d00ec; padding: 0 5px;}
.supp {background-color: #0c7326; padding: 0 5px;}
.plan {background-color: #ce4d00; padding: 0 5px;}
</style>

# JSON-RPC API

<div class="warning">

This API is still a WIP, with more methods being added continuously.

Need a specific method? Let us know on
[Github](https://github.com/ChainSafe/forest/issues) or Slack (#fil-forest-help)
🙏

</div>

# Overview

The RPC interface is the primary mechanism for interacting with Forest. The
implementation is still a WIP, with support for more methods being added
continuously.

As there is presently no cross-client specification, the
[Lotus V1 interface](https://github.com/filecoin-project/lotus/blob/master/documentation/en/api-v1-unstable-methods.md)
is the reference for Forest's implementation.

# Supported Methods

| Status                                          | Definition                                    |
| ----------------------------------------------- | --------------------------------------------- |
| <span class=notImp>Not Implemented</span>       | No development planned or completed           |
| <span class=partSupp>Partially Supported</span> | Some methods are supported                    |
| <span class=supp>Supported</span>               | All methods supported                         |
| <span class=plan>Planned</span>                 | Work planned to implement some or all methods |

## V1 Status

|           | Status                                          | Notes                           |
| --------- | ----------------------------------------------- | ------------------------------- |
| Common    | <span class=partSupp>Partially Supported</span> |                                 |
| Auth      | <span class=supp>Supported</span>               |                                 |
| Chain     | <span class=partSupp>Partially Supported</span> |                                 |
| Client    | <span class=notImp>Not Implemented</span>       |                                 |
| Create    | <span class=notImp>Not Implemented</span>       |                                 |
| Eth       | <span class=partSupp>Partially Supported</span> |                                 |
| Filecoin  | <span class=notImp>Not Implemented</span>       |                                 |
| Gas       | <span class=supp>Supported</span>               |                                 |
| Get       | <span class=notImp>Not Implemented</span>       |                                 |
| I         | <span class=notImp>Not Implemented</span>       |                                 |
| Log       | <span class=notImp>Not Implemented</span>       |                                 |
| Market    | <span class=notImp>Not Implemented</span>       |                                 |
| Miner     | <span class=partSupp>Partially Supported</span> |                                 |
| Mpool     | <span class=partSupp>Partially Supported</span> |                                 |
| Msig      | <span class=notImp>Not Implemented</span>       |                                 |
| Net       | <span class=partSupp>Partially Supported</span> |                                 |
| Node      | <span class=supp>Supported</span>               |                                 |
| Paych     | <span class=notImp>Not Implemented</span>       |                                 |
| Start     | <span class=notImp>Not Implemented</span>       |                                 |
| State     | <span class=partSupp>Partially Supported</span> |                                 |
| Subscribe | <span class=notImp>Not Implemented</span>       |                                 |
| Sync      | <span class=partSupp>Partially Supported</span> |                                 |
| Wallet    | <span class=partSupp>Partially Supported</span> | Missing only `WalletSignMesage` |
| Web3      | <span class=notImp>Not Implemented</span>       |                                 |
