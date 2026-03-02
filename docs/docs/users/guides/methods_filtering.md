---
title: RPC methods filtering
sidebar_position: 4
---

# RPC methods filtering

## Why filter RPC methods?

When running a Filecoin node, you might want to restrict the RPC methods that are available to the clients. This can be useful for security reasons, to limit the exposure of the node to the internet, or to reduce the load on the node by disabling unnecessary methods.

:::note
[JWT authentication](../knowledge_base/jwt_handling) is a different way to restrict access to the node. It allows you to authorize certain operations on the node using JWTs. However, JWT restrictions are hard-coded in the node and cannot be changed dynamically. If you want to make sure that a certain read-only method is not available to the clients, you can use the method filtering feature.

The methods are first filtered by the method filtering feature, and then the JWT authentication is applied. If a method is disallowed by the method filtering, the JWT token will not be checked for this method.
:::

## How to filter RPC methods

You need to run `forest` with the `--rpc-filter-list <PATH-TO-FILTER-LIST>` argument. If the filter list is not provided, all methods are allowed by default.

### Example

In this example, will disallow the `Filecoin.ChainExport` method which is used to export the chain to a file. This method should not be available to the clients due to its impact (compute, disk space, etc.) on the node.

1. Create a filter list file, for example, `filter-list.txt`:

```plaintext
# Disabling the snapshot exporting
!Filecoin.ChainExport
```

2. Run `forest` with the `--rpc-filter-list` argument:

```shell
forest --chain calibnet --rpc-filter-list filter-list.txt
```

3. Try to export the snapshot using the `forest-cli`:

```shell
forest-cli snapshot-export
```

You should see the following error:

```console
Getting ready to export...
Error: ErrorObject { code: ServerError(403), message: "Forbidden", data: None }

Caused by:
    ErrorObject { code: ServerError(403), message: "Forbidden", data: None }
```

## Filter list format

The filter list is a text file where each line represents a method that should be allowed or disallowed. The format is as follows:

- `!` at the beginning of the line means that the method is disallowed.
- `#` at the beginning of the line is a comment and is ignored.
- no prefix means that all the methods containing this name are allowed.

If there is a single allowed method (no prefix), all non-matching methods are disallowed by default.

:::warning
Some methods have aliases, so you need to filter all of them. This is most prominent in the `Filecoin.Eth.*` namespace. They are implemented for compatibility with Lotus, see [here](https://github.com/filecoin-project/lotus/blob/a9718c841e1fced8afc6e9fee2db2a2b565acc42/api/eth_aliases.go).
:::

## Example filter lists

Allow only the `Filecoin.StateCall` method. All other methods are disallowed:

```plaintext
Filecoin.StateCall
```

Disallow the `Filecoin.ChainExport` method. All other methods are allowed:

```plaintext
!Filecoin.ChainExport
```

Disallow the `Filecoin.EthGasPrice`, `Filecoin.EthEstimateGas`, and their aliases. All other methods are allowed:

```plaintext
!Filecoin.EthGasPrice
!eth_gasPrice
!Filecoin.EthEstimateGas
!eth_estimateGas
```

Allow all the methods in the `Filecoin.Chain` namespace. Disallow the `Filecoin.ChainExport` method. This will allow methods such as `Filecoin.ChainGetTipSet` and `Filecoin.ChainGetBlock` but disallow the `Filecoin.ChainExport` method:

```plaintext
Filecoin.Chain
!Filecoin.ChainExport
```

## Public RPC node recommendations

If you are running a public RPC node, it is recommended to filter certain methods (even those not requiring a JWT token) to reduce the load on the node and to improve security. Here is a list of methods that you might want to consider filtering:

```plaintext
# Creates a snapshot of the chain and writes it to a file. Very resource-intensive.
!Filecoin.ChainExport
# Potentially resource-intensive.
!Filecoin.StateCompute
!Filecoin.StateReplay
# Very memory-intensive on mainnet (way over 64 GB per call). It's fine on testnets.
!Filecoin.StateMarketDeals
```
