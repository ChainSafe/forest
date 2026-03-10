# `debug_traceTransaction` API Guide

This guide explains the `debug_traceTransaction` RPC method implemented in Forest, which follows the **[Geth debug namespace](https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-debug#debugtracetransaction)** format.

## Overview

`debug_traceTransaction` re-executes an existing on-chain transaction and returns detailed execution traces. Unlike `trace_call` (which simulates a call), this method traces a transaction that has already been mined. It's useful for:

- Debugging failed or unexpected transactions
- Analyzing the full execution trace of a historical transaction
- Inspecting `pre/post` state changes caused by a specific transaction
- Understanding nested call hierarchies in complex transactions

## Request Format

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "debug_traceTransaction",
  "params": [
    "0x...",
    {
      "tracer": "prestateTracer",
      "tracerConfig": { "diffMode": false }
    }
  ]
}
```

### Supported Tracers

| Tracer           | Description                                                     |
| ---------------- | --------------------------------------------------------------- |
| `callTracer`     | Call hierarchy with inputs, outputs, gas used, and nested calls |
| `flatCallTracer` | Flattened list of all calls (no nesting)                        |
| `prestateTracer` | `Pre-execution` state snapshot of all touched accounts          |

### Tracer Configuration

#### `prestateTracer` config

| Option     | Type    | Default | Description                                      |
| ---------- | ------- | ------- | ------------------------------------------------ |
| `diffMode` | boolean | `false` | When `true`, returns both `pre` and `post` state |

#### `callTracer` config

| Option        | Type    | Default | Description                                                   |
| ------------- | ------- | ------- | ------------------------------------------------------------- |
| `onlyTopCall` | boolean | `false` | When `true`, only trace top call                              |
| `withLog`     | boolean | `false` | When `true`, includes logs in the trace (not yet implemented) |

## Response Formats

### `prestateTracer` (Default Mode)

Returns the `pre-execution` state of every account touched during the transaction. Each account includes only the fields that are relevant (empty fields are omitted).

```json
{
  "result": {
    "0xcontract...": {
      "balance": "0xde0b6b3a7640000",
      "code": "0x6080...",
      "nonce": "0x1",
      "storage": {
        "0x0000...0000": "0x0000...002a"
      }
    },
    "0xsender...": {
      "balance": "0x72c3a2e371dcc225",
      "nonce": "0x1e"
    }
  }
}
```

**Fields per account:**

| Field     | Type             | Description                                        |
| --------- | ---------------- | -------------------------------------------------- |
| `balance` | hex string       | Account balance before the transaction             |
| `nonce`   | hex string       | Account nonce before the transaction               |
| `code`    | hex string       | Contract bytecode (omitted for EOAs)               |
| `storage` | map (hex -> hex) | Storage slots that were changed by the transaction |

### `prestateTracer` (Diff Mode)

When `diffMode: true`, returns separate `pre` and `post` objects showing the state before and after the transaction.

```json
{
  "result": {
    "pre": {
      "0xcontract...": {
        "balance": "0xde0b6b3a7640000",
        "nonce": "0x1",
        "storage": {
          "0x0000...0000": "0x0000...002a"
        }
      },
      "0xsender...": {
        "balance": "0x72c3a2e371dcc225",
        "nonce": "0x1e"
      }
    },
    "post": {
      "0xcontract...": {
        "storage": {
          "0x0000...0000": "0x0000...007b"
        }
      },
      "0xsender...": {
        "balance": "0x72c3a1fb5a411da3",
        "nonce": "0x1f"
      }
    }
  }
}
```

**Diff mode behavior:**

- `pre` contains the state before execution for all touched accounts
- `post` contains only the fields that changed after execution
- Accounts that were deleted appear in `pre` but not in `post`
- Accounts that were created appear in `post` but not in `pre`
- Unchanged accounts are omitted from both `pre` and `post`
- Zero-value storage entries are stripped from `post`

### `callTracer`

Returns the call hierarchy as a nested tree of call frames.

```json
{
  "result": {
    "type": "CALL",
    "from": "0xsender...",
    "to": "0xcontract...",
    "value": "0x0",
    "gas": "0x...",
    "gasUsed": "0x...",
    "input": "0x4018d9aa...",
    "output": "0x...",
    "calls": [
      {
        "type": "CALL",
        "from": "0xcontract...",
        "to": "0xcontract...",
        "value": "0x0",
        "gas": "0x...",
        "gasUsed": "0x...",
        "input": "0x...",
        "output": "0x..."
      }
    ]
  }
}
```

### `flatCallTracer`

Returns a flat list of all call frames (equivalent to Parity-style `trace`).

```json
{
  "result": [
    {
      "type": "call",
      "subtraces": 1,
      "traceAddress": [],
      "action": {
        "callType": "call",
        "from": "0x...",
        "to": "0x...",
        "gas": "0x...",
        "value": "0x0",
        "input": "0x..."
      },
      "result": {
        "gasUsed": "0x...",
        "output": "0x..."
      }
    }
  ]
}
```

## Using `debug_traceTransaction` with Forest

### Prerequisites

- A running Forest node — follow the [Getting Started](https://docs.forest.chainsafe.io/getting_started/syncing) guide to start and sync.
- A transaction hash from a mined transaction on the synced network.

Forest RPC endpoint: `http://localhost:2345/rpc/v1`

### Deployed Contracts

A [Tracer](https://github.com/ChainSafe/forest/blob/963237708137e9c7388c57eba39a2f8bf12ace74/src/tool/subcommands/api_cmd/contracts/tracer/Tracer.sol) contract is `pre-deployed` on Calibnet and Mainnet for testing. You can send transactions to these contracts and then trace them.

| Network  | Contract Address                                                                                                                      |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| Calibnet | [`0x73a43475aa2ccb14246613708b399f4b2ba546c7`](https://calibration.filfox.info/en/address/0x73a43475aa2ccb14246613708b399f4b2ba546c7) |
| Mainnet  | [`0x9BB686Ba6a50D1CF670a98f522a59555d4977fb2`](https://filecoin.blockscout.com/address/0x9BB686Ba6a50D1CF670a98f522a59555d4977fb2)    |

> **Note:** To trace a transaction, the Forest node must have the state data for the epoch containing that transaction. Recent transactions on a synced node should always be traceable.

### Sending a Test Transaction

To generate a transaction for tracing, send a transaction using `cast`:

```bash
cast send $CONTRACT "setX(uint256)" 123 \
    --rpc-url http://localhost:2345/rpc/v1 \
    --private-key <YOUR_PRIVATE_KEY>

# Output:
# transactionHash: 0x90aa2b46...
```

Save the transaction hash for use in the tracing examples below.

## Example curl Requests

Before running the examples, set the following environment variables:

```bash
export FOREST_RPC_URL="http://localhost:2345/rpc/v1"
export TX_HASH="0xYOUR_TRANSACTION_HASH"
```

### 1. `Prestate` Trace - Default Mode

Returns the `pre-execution` state of all accounts touched by the transaction.

```bash
curl -s -X POST "$FOREST_RPC_URL" \
    -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "debug_traceTransaction",
        "params": [
            "'$TX_HASH'",
            {"tracer": "prestateTracer"}
        ]
    }' | jq '.'
```

### 2. `Prestate` Trace - Diff Mode

Returns both `pre` and `post` state, showing exactly what changed.

```bash
curl -s -X POST "$FOREST_RPC_URL" \
    -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "debug_traceTransaction",
        "params": [
            "'$TX_HASH'",
            {"tracer": "prestateTracer", "tracerConfig": {"diffMode": true}}
        ]
    }' | jq '.'
```

### 3. Call Trace

Returns the call hierarchy showing nested calls, gas usage, and return values.

```bash
curl -s -X POST "$FOREST_RPC_URL" \
    -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "debug_traceTransaction",
        "params": [
            "'$TX_HASH'",
            {"tracer": "callTracer"}
        ]
    }' | jq '.'
```

### 4. Flat Call Trace

Returns a flattened list of all calls (Parity-style trace format).

```bash
curl -s -X POST "$FOREST_RPC_URL" \
    -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "debug_traceTransaction",
        "params": [
            "'$TX_HASH'",
            {"tracer": "flatCallTracer"}
        ]
    }' | jq '.'
```

## Troubleshooting

### Common Issues

1. **"message not found in tipset"**: The transaction hash may not exist on the chain, or the node may not have synced the epoch containing this transaction.
2. **"replay for `prestate` failed"**: The node does not have the state data required to re-execute the transaction. Ensure the node is synced past the epoch containing the transaction.
3. **Empty storage in `prestate`**: The contract may not be an EVM actor, or no storage slots were modified by the transaction.
4. **Extra addresses in response**: Forest may include Filecoin ID addresses (e.g., `0xff00...`) alongside EVM addresses. This is expected behavior due to `Filecoin's` dual address representation.

### Debug Tips

```bash
# Look up a transaction by hash
curl -s -X POST "$FOREST_RPC_URL" \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","id":1,"method":"eth_getTransactionByHash","params":["'$TX_HASH'"]}' \
    | jq '.'

# Get transaction receipt (confirms it was mined)
curl -s -X POST "$FOREST_RPC_URL" \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","id":1,"method":"eth_getTransactionReceipt","params":["'$TX_HASH'"]}' \
    | jq '.result.status'
```

## `Filecoin-Specific` Behavior

Forest's `debug_traceTransaction` implementation has some differences from standard Ethereum implementations due to `Filecoin's` architecture:

| Aspect                | Forest (Filecoin)                              | Geth (Ethereum)                             |
| --------------------- | ---------------------------------------------- | ------------------------------------------- |
| **ID addresses**      | May include `0xff00...` Filecoin ID addresses  | Only EVM addresses                          |
| **Coinbase**          | Not included (gas handled at protocol level)   | Included as `0x0000...0000`                 |
| **Per-message state** | Re-executes all prior messages in the tipset   | Re-executes all prior transactions in block |
| **Storage model**     | EVM storage via KAMT (Key-Address-Merkle-Tree) | Standard Merkle Patricia Trie               |

## Official Resources

- [Geth `debug_traceTransaction`](https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-debug#debugtracetransaction)
- [Geth Built-in Tracers](https://geth.ethereum.org/docs/developers/evm-tracing/built-in-tracers)
- [Reth debug Namespace](https://reth.rs/jsonrpc/debug)
- [Foundry Book - Cast](https://book.getfoundry.sh/reference/cast/)
