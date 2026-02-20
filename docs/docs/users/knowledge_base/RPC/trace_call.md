# trace_call API Guide

This guide explains the `trace_call` RPC method implemented in Forest, which follows the **[Parity/OpenEthereum](https://openethereum.github.io/JSONRPC-trace-module#trace_call) and [reth](https://reth.rs/jsonrpc/trace#trace-format-specification) trace format**.

## Overview

`trace_call` executes an EVM call and returns detailed execution traces without creating a transaction on the blockchain. It's useful for:

- Debugging smart contract calls
- Analyzing gas usage and call patterns
- Inspecting state changes before execution
- Understanding nested call hierarchies

## Request Format

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "trace_call",
  "params": [
    {
      "from": "0x...", // Sender address
      "to": "0x...", // Contract address
      "data": "0x...", // Encoded function call
      "value": "0x0", // ETH value (optional)
      "gas": "0x...", // Gas limit (optional)
      "gasPrice": "0x..." // Gas price (optional)
    },
    ["trace", "stateDiff"], // Trace types to return
    "latest" // Block number or tag
  ]
}
```

### Trace Types

| Type        | Description                                                         |
| ----------- | ------------------------------------------------------------------- |
| `trace`     | Call hierarchy with inputs, outputs, gas used                       |
| `stateDiff` | State changes (balance, nonce, code, storage)                       |
| `vmTrace`   | Low-level EVM execution (not yet implemented, not supported by FVM) |

## Response Format (Parity Style)

Forest uses the **Parity/OpenEthereum trace format**, which differs from [Geth's debug API's](https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-debug#debugtracecall).

### Trace Response

```json
{
  "result": {
    "output": "0x...",
    "trace": [
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
        },
        "error": null
      }
    ],
    "stateDiff": { ... }
  }
}
```

### `stateDiff` Response

State changes use **Delta notation**:

| Symbol | Meaning   | Example                                                  |
| ------ | --------- | -------------------------------------------------------- |
| `"="`  | Unchanged | `"balance": "="`                                         |
| `"+"`  | Added     | `"balance": { "+": "0x1000" }`                           |
| `"-"`  | Removed   | `"balance": { "-": "0x1000" }`                           |
| `"*"`  | Changed   | `"balance": { "*": { "from": "0x100", "to": "0x200" } }` |

```json
{
  "stateDiff": {
    "0xcontract...": {
      "balance": "=",
      "code": "=",
      "nonce": "=",
      "storage": {
        "0x0000...0000": {
          "*": {
            "from": "0x000...02a",
            "to": "0x000...07b"
          }
        }
      }
    },
    "0xsender...": {
      "balance": "=",
      "code": "=",
      "nonce": {
        "*": {
          "from": "0x5",
          "to": "0x6"
        }
      },
      "storage": {}
    }
  }
}
```

## Parity vs Geth Format Comparison

| Aspect               | Forest (Parity)                      | Geth                                    |
| -------------------- | ------------------------------------ | --------------------------------------- |
| **API Method**       | `trace_call`                         | `debug_traceCall`                       |
| **State Format**     | Delta notation (`"*"`, `"+"`, `"-"`) | Separate `pre`/`post` objects           |
| **Unchanged Values** | Shows `"="`                          | Included in `pre`, absent in `post`     |
| **Storage Changes**  | `{ "*": { from, to } }`              | Compare `pre.storage` vs `post.storage` |
| **Code Field**       | `"="` if unchanged                   | Full bytecode in `pre`                  |

### Example: Same call, different formats

**Forest (Parity):**

```json
{
  "storage": {
    "0x00...00": {
      "*": {
        "from": "0x00...2a",
        "to": "0x00...7b"
      }
    }
  }
}
```

**Geth (`prestateTracer` with `diffMode`):**

```json
{
  "pre": {
    "storage": { "0x00...00": "0x00...2a" }
  },
  "post": {
    "storage": { "0x00...00": "0x00...7b" }
  }
}
```

## Using trace_call with Forest

### Prerequisites

- A running Forest node — follow the [Getting Started](https://docs.forest.chainsafe.io/getting_started/syncing) guide to start and sync.
- A deployed EVM contract to trace against (see [Deployed Contracts](#deployed-contracts) below, or deploy your own).

Forest RPC endpoint: `http://localhost:2345/rpc/v1`

### Deployed Contracts

A [Tracer](https://github.com/ChainSafe/forest/blob/963237708137e9c7388c57eba39a2f8bf12ace74/src/tool/subcommands/api_cmd/contracts/tracer/Tracer.sol) contract is pre-deployed on Calibnet and Mainnet for testing `trace_call`. It provides functions for storage writes, ETH transfers, nested calls, and reverts.

| Network  | Contract Address                                                                                                                      |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| Calibnet | [`0x73a43475aa2ccb14246613708b399f4b2ba546c7`](https://calibration.filfox.info/en/address/0x73a43475aa2ccb14246613708b399f4b2ba546c7) |
| Mainnet  | [`0x9BB686Ba6a50D1CF670a98f522a59555d4977fb2`](https://filecoin.blockscout.com/address/0x9BB686Ba6a50D1CF670a98f522a59555d4977fb2)    |

> **Note:** Contract availability depends on network state. Verify the contract exists before testing:
>
> ```bash
> curl -s -X POST "http://localhost:2345/rpc/v1" \
>     -H "Content-Type: application/json" \
>     -d '{"jsonrpc":"2.0","id":1,"method":"eth_getCode","params":["0x73a43475aa2ccb14246613708b399f4b2ba546c7","latest"]}' \
>     | jq -r '.result | length'
> ```
>
> If the result is `> 2`, the contract is deployed.

### Deploying Your Own Contract

To deploy a contract on Calibnet or Mainnet, you need:

1. A funded wallet with FIL tokens for gas (use the [Forest faucet](https://forest-explorer.chainsafe.dev/faucet) for testnet funds)
2. [Foundry](https://book.getfoundry.sh/getting-started/installation) installed (`forge` command)

```bash
forge create YourContract.sol:YourContract \
    --rpc-url http://localhost:2345/rpc/v1 \
    --broadcast \
    --private-key <YOUR_WALLET_PRIVATE_KEY>

# Output:
# Deployer: 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
# Deployed to: 0x5FbDB2315678afecb367f032d93F642f64180aa3
# Transaction hash: 0x...
```

## Example curl Requests

The examples below use the pre-deployed [Tracer](https://github.com/ChainSafe/forest/blob/963237708137e9c7388c57eba39a2f8bf12ace74/src/tool/subcommands/api_cmd/contracts/tracer/Tracer.sol) contract. Here are the functions used:

| Function                                   | Selector     | Description                         |
| ------------------------------------------ | ------------ | ----------------------------------- |
| `setX(uint256)`                            | `0x4018d9aa` | Write a value to storage slot 0     |
| `deposit()`                                | `0xd0e30db0` | Receive ETH, update balance mapping |
| `deepTrace(uint256)`                       | `0x0f3a17b8` | Recursive N-level nested calls      |
| `storageMultiple(uint256,uint256,uint256)` | `0x310af204` | Write to multiple storage slots     |

Before running the examples, set the following environment variables:

```bash
export FOREST_RPC_URL="http://localhost:2345/rpc/v1"
export SENDER="0xYOUR_ACCOUNT"              # your sender address
export CONTRACT="0xCONTRACT_ADDRESS"        # deployed Tracer contract
```

### 1. Basic Trace - `setX(123)`

```bash
curl -s -X POST "$FOREST_RPC_URL" \
    -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "trace_call",
        "params": [
            {
                "from": "'$SENDER'",
                "to": "'$CONTRACT'",
                "data": "0x4018d9aa000000000000000000000000000000000000000000000000000000000000007b"
            },
            ["trace"],
            "latest"
        ]
    }' | jq '.'
```

### 2. State Diff - Storage Change

```bash
curl -s -X POST "$FOREST_RPC_URL" \
    -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "trace_call",
        "params": [
            {
                "from": "'$SENDER'",
                "to": "'$CONTRACT'",
                "data": "0x4018d9aa000000000000000000000000000000000000000000000000000000000000007b"
            },
            ["stateDiff"],
            "latest"
        ]
    }' | jq '.result.stateDiff'
```

### 3. Balance Change - deposit() with ETH

```bash
curl -s -X POST "$FOREST_RPC_URL" \
    -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "trace_call",
        "params": [
            {
                "from": "'$SENDER'",
                "to": "'$CONTRACT'",
                "data": "0xd0e30db0",
                "value": "0xde0b6b3a7640000"
            },
            ["trace", "stateDiff"],
            "latest"
        ]
    }' | jq '.'
```

### 4. Deep Trace - `deepTrace(3)`

```bash
curl -s -X POST "$FOREST_RPC_URL" \
    -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "trace_call",
        "params": [
            {
                "from": "'$SENDER'",
                "to": "'$CONTRACT'",
                "data": "0x0f3a17b80000000000000000000000000000000000000000000000000000000000000003"
            },
            ["trace"],
            "latest"
        ]
    }' | jq '.result.trace | length'
```

### 5. Multiple Storage Slots - `storageMultiple(10,20,30)`

```bash
curl -s -X POST "$FOREST_RPC_URL" \
    -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "trace_call",
        "params": [
            {
                "from": "'$SENDER'",
                "to": "'$CONTRACT'",
                "data": "0x310af204000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000014000000000000000000000000000000000000000000000000000000000000001e"
            },
            ["stateDiff"],
            "latest"
        ]
    }' | jq '.result.stateDiff'
```

## Troubleshooting

### Common Issues

1. **Empty storage in `stateDiff`**: Ensure the contract is an EVM actor (has bytecode)
2. **Call reverts**: Check function requirements (e.g., `storageChange` requires slot to have value first)
3. **Missing contract**: Verify contract is deployed at the specified address

### Debug Tips

```bash
# Check if address has code
curl -s -X POST "$FOREST_RPC_URL" \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","id":1,"method":"eth_getCode","params":["'$CONTRACT'","latest"]}' \
    | jq -r '.result | length'

# Check account balance
curl -s -X POST "$FOREST_RPC_URL" \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","id":1,"method":"eth_getBalance","params":["'$SENDER'","latest"]}' \
    | jq '.result'
```

## Official Resources

- [OpenEthereum trace module](https://openethereum.github.io/JSONRPC-trace-module)
- [Geth Built-in Tracers](https://geth.ethereum.org/docs/developers/evm-tracing/built-in-tracers)
- [Alchemy: `trace_call` vs `debug_traceCall`](https://www.alchemy.com/docs/reference/trace_call-vs-debug_tracecall)
- [Reth trace Namespace](https://reth.rs/jsonrpc/trace)
