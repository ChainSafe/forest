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
| `vmTrace`   | Low-level EVM execution (not yet implemented, not supproted by FVM) |

## Response Format (Parity Style)

Forest uses the **Parity/OpenEthereum trace format**, which differs from Geth's debug API.

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

### StateDiff Response

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

**Geth (prestateTracer with diffMode):**

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

## Testing with Tracer Contract

The `Tracer.sol` contract provides various functions to test different tracing scenarios.

### Contract Location

```
src/tool/subcommands/api_cmd/contracts/tracer/Tracer.sol
```

### Prerequisites

- [Foundry](https://book.getfoundry.sh/getting-started/installation) installed (`forge`, `cast` commands)
- A running Forest node or Anvil for local testing or both node for comparison

---

### Option 1: Testing with Forest Node

#### Starting Forest Node

**Calibration Network (Testnet - Recommended for testing):**

```bash
forest --chain calibnet --auto-download-snapshot --encrypt-keystore false
```

**Mainnet:**

```bash
forest --chain mainnet --auto-download-snapshot --encrypt-keystore false
```

Forest RPC endpoint: `http://localhost:2345/rpc/v1`

#### Deploying Contract on Forest

To deploy the contract on Calibnet or Mainnet, you need:

1. A funded wallet with FIL tokens for gas
2. Convert your Filecoin address to an Ethereum-style address (f4/0x format)

```bash
# Deploy using forge (requires funded account)
forge create src/tool/subcommands/api_cmd/contracts/tracer/Tracer.sol:Tracer \
    --rpc-url http://localhost:2345/rpc/v1 \
    --boradcast \
    --private-key <FOREST_WALLET_PRIVATE_KEY>
```

#### Existing Deployed Contracts

If you don't want to deploy your own contract, you can use these pre-deployed addresses:

| Network  | Contract Address                             | Notes                                       |
| -------- | -------------------------------------------- | ------------------------------------------- |
| Calibnet | `0x73a43475aa2ccb14246613708b399f4b2ba546c7` | Full Tracer.sol with storage diff functions |

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

---

### Option 2: Testing with Anvil (Local Development)

#### What is Anvil?

[Anvil](https://getfoundry.sh/anvil/reference/) is a local Ethereum development node included with Foundry. It provides:

- Instant block mining
- Pre-funded test accounts (10 accounts with 10,000 ETH each)
- Support for `debug_traceCall` with various tracers
- No real tokens required

Anvil uses **Geth-style** tracing (`debug_traceCall` with `prestateTracer`), while Forest uses **Parity-style** tracing (`trace_call` with `stateDiff`). This makes Anvil useful for comparison testing.

#### Starting Anvil

```bash
# Start Anvil with tracer to allow debug_traceCall API's
anvil --tracing
```

Anvil RPC endpoint: `http://localhost:8545`

#### Deploying Contract on Anvil

```bash
# Use the first pre-funded account's private key
forge create src/tool/subcommands/api_cmd/contracts/tracer/Tracer.sol:Tracer \
    --rpc-url http://localhost:8545 \
    --broadcast \
    --private-key <ANVIL_OUTPUT_PRIVATE_KEY>

# Output:
# Deployer: 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
# Deployed to: 0x5FbDB2315678afecb367f032d93F642f64180aa3
# Transaction hash: 0x...
```

#### Comparing Forest vs Anvil Responses

**Forest (Parity-style `trace_call`):**

```bash
curl -s -X POST "http://localhost:2345/rpc/v1" \
    -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "trace_call",
        "params": [
            {"from": "0x...", "to": "0x...", "data": "0x4018d9aa..."},
            ["stateDiff"],
            "latest"
        ]
    }'
```

**Anvil (Geth-style `debug_traceCall`):**

```bash
curl -s -X POST "http://localhost:8545" \
    -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "debug_traceCall",
        "params": [
            {"from": "0x...", "to": "0x...", "data": "0x4018d9aa..."},
            "latest",
            {"tracer": "prestateTracer", "tracerConfig": {"diffMode": true}}
        ]
    }'
```

---

### Storage Layout

| Slot | Variable       | Description                  |
| ---- | -------------- | ---------------------------- |
| 0    | `x`            | Initialized to 42            |
| 1    | `balances`     | Mapping base slot            |
| 2    | `storageTestA` | Starts empty (for add tests) |
| 3    | `storageTestB` | Starts empty                 |
| 4    | `storageTestC` | Starts empty                 |
| 5    | `dynamicArray` | Array length slot            |

### Function Reference

#### Basic Operations

| Function            | Selector     | Description                 |
| ------------------- | ------------ | --------------------------- |
| `setX(uint256)`     | `0x4018d9aa` | Write to slot 0             |
| `deposit()`         | `0xd0e30db0` | Receive ETH, update mapping |
| `withdraw(uint256)` | `0x2e1a7d4d` | Send ETH from contract      |
| `doRevert()`        | `0xafc874d2` | Always reverts              |

#### Call Tracing

| Function                | Selector     | Description            |
| ----------------------- | ------------ | ---------------------- |
| `callSelf(uint256)`     | `0xa1a88595` | Single nested CALL     |
| `delegateSelf(uint256)` | `0x8f5e07b8` | DELEGATECALL trace     |
| `complexTrace()`        | `0x6659ab96` | Multiple nested calls  |
| `deepTrace(uint256)`    | `0x0f3a17b8` | Recursive N-level deep |

#### Storage Diff Testing

| Function                                   | Selector     | Description          |
| ------------------------------------------ | ------------ | -------------------- |
| `storageAdd(uint256)`                      | `0x55cb64b4` | Add to empty slot 2  |
| `storageChange(uint256)`                   | `0x7c8f6e57` | Modify existing slot |
| `storageDelete()`                          | `0xd92846a3` | Set slot to zero     |
| `storageMultiple(uint256,uint256,uint256)` | `0x310af204` | Change slots 2,3,4   |

## Example curl Requests for Forest node.

> **Note:**: Anvil has a different params format check above, request data is same as Forest.

### 1. Basic Trace - setX(123)

```bash
curl -s -X POST "http://localhost:2345/rpc/v1" \
    -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "trace_call",
        "params": [
            {
                "from": "0xYOUR_ACCOUNT",
                "to": "0xCONTRACT_ADDRESS",
                "data": "0x4018d9aa000000000000000000000000000000000000000000000000000000000000007b"
            },
            ["trace"],
            "latest"
        ]
    }' | jq '.'
```

### 2. State Diff - Storage Change

```bash
curl -s -X POST "http://localhost:2345/rpc/v1" \
    -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "trace_call",
        "params": [
            {
                "from": "0xYOUR_ACCOUNT",
                "to": "0xCONTRACT_ADDRESS",
                "data": "0x4018d9aa000000000000000000000000000000000000000000000000000000000000007b"
            },
            ["stateDiff"],
            "latest"
        ]
    }' | jq '.result.stateDiff'
```

### 3. Balance Change - deposit() with ETH

```bash
curl -s -X POST "http://localhost:2345/rpc/v1" \
    -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "trace_call",
        "params": [
            {
                "from": "0xYOUR_ACCOUNT",
                "to": "0xCONTRACT_ADDRESS",
                "data": "0xd0e30db0",
                "value": "0xde0b6b3a7640000"
            },
            ["trace", "stateDiff"],
            "latest"
        ]
    }' | jq '.'
```

### 4. Deep Trace - deepTrace(3)

```bash
curl -s -X POST "http://localhost:2345/rpc/v1" \
    -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "trace_call",
        "params": [
            {
                "from": "0xYOUR_ACCOUNT",
                "to": "0xCONTRACT_ADDRESS",
                "data": "0x0f3a17b80000000000000000000000000000000000000000000000000000000000000003"
            },
            ["trace"],
            "latest"
        ]
    }' | jq '.result.trace | length'
```

### 5. Multiple Storage Slots - storageMultiple(10,20,30)

```bash
curl -s -X POST "http://localhost:2345/rpc/v1" \
    -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "trace_call",
        "params": [
            {
                "from": "0xYOUR_ACCOUNT",
                "to": "0xCONTRACT_ADDRESS",
                "data": "0x310af204000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000014000000000000000000000000000000000000000000000000000000000000001e"
            },
            ["stateDiff"],
            "latest"
        ]
    }' | jq '.result.stateDiff'
```

## Integration Test Script

An automated test script is available to compare Forest's `trace_call` with Anvil's `debug_traceCall`:

```bash
# Run the test (requires Forest and Anvil running)
./scripts/tests/trace_call_integration_test.sh

or

# Deploy contract on Anvil first, forest and anvil node should already be running
./scripts/tests/trace_call_integration_test.sh --deploy
```

### Test Categories

1. **Trace Tests**: Call hierarchy, subcalls, reverts, deep traces
2. **Balance Diff Tests**: ETH transfers, deposits
3. **Storage Diff Tests**: Single slot, multiple slots, value comparison

## Generating Function Selectors

Use `cast` from Foundry to generate function selectors:

```bash
# Get selector for a function
cast sig "setX(uint256)"
# Output: 0x4018d9aa

# Encode full calldata
cast calldata "setX(uint256)" 123
# Output: 0x4018d9aa000000000000000000000000000000000000000000000000000000000000007b
```

## Troubleshooting

### Common Issues

1. **Empty storage in stateDiff**: Ensure the contract is an EVM actor (has bytecode)
2. **Call reverts**: Check function requirements (e.g., `storageChange` requires slot to have value first)
3. **Missing contract**: Verify contract is deployed at the specified address

### Debug Tips

```bash
# Check if address has code
curl -s -X POST "http://localhost:2345/rpc/v1" \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","id":1,"method":"eth_getCode","params":["0xCONTRACT","latest"]}' \
    | jq -r '.result | length'

# Check account balance
curl -s -X POST "http://localhost:2345/rpc/v1" \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","id":1,"method":"eth_getBalance","params":["0xACCOUNT","latest"]}' \
    | jq '.result'
```

## Offical Resources

| Resource                                                                                                       | Description                                                                        |
| -------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| [OpenEthereum trace module](https://openethereum.github.io/JSONRPC-trace-module)                               | Official Parity/OpenEthereum documentation for `trace_call` and `stateDiff` format |
| [Geth Built-in Tracers](https://geth.ethereum.org/docs/developers/evm-tracing/built-in-tracers)                | Geth documentation for `prestateTracer` and `callTracer`                           |
| [Alchemy: trace_call vs debug_traceCall](https://www.alchemy.com/docs/reference/trace_call-vs-debug_tracecall) | Detailed comparison of both tracing methods                                        |
| [Reth trace Namespace](https://reth.rs/jsonrpc/trace)                                                          | Reth's implementation of the trace API (follows Parity format)                     |
| [Foundry Book - Anvil](https://book.getfoundry.sh/reference/anvil/)                                            | Anvil local development node documentation                                         |
