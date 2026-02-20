# trace_call Developer Guide

This guide covers testing and development workflows for Forest's `trace_call` implementation. For API documentation and user-facing usage, see the [trace_call API guide](/knowledge_base/rpc/trace_call).

## Tracer Contract

The [`Tracer.sol`](https://github.com/ChainSafe/forest/blob/963237708137e9c7388c57eba39a2f8bf12ace74/src/tool/subcommands/api_cmd/contracts/tracer/Tracer.sol) contract provides various functions to test different tracing scenarios.

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
| `delegateSelf(uint256)` | `0x8f5e07b8` | `DELEGATECALL` trace   |
| `complexTrace()`        | `0x6659ab96` | Multiple nested calls  |
| `deepTrace(uint256)`    | `0x0f3a17b8` | Recursive N-level deep |

#### Storage Diff Testing

| Function                                   | Selector     | Description          |
| ------------------------------------------ | ------------ | -------------------- |
| `storageAdd(uint256)`                      | `0x55cb64b4` | Add to empty slot 2  |
| `storageChange(uint256)`                   | `0x7c8f6e57` | Modify existing slot |
| `storageDelete()`                          | `0xd92846a3` | Set slot to zero     |
| `storageMultiple(uint256,uint256,uint256)` | `0x310af204` | Change slots 2,3,4   |

### Generating Function Selectors

Use `cast` from Foundry to generate function selectors:

```bash
# Get selector for a function
cast sig "setX(uint256)"
# Output: 0x4018d9aa

# Encode full calldata
cast calldata "setX(uint256)" 123
# Output: 0x4018d9aa000000000000000000000000000000000000000000000000000000000000007b
```

### Deployed Contracts

Pre-deployed Tracer contracts for quick testing:

| Network  | Contract Address                                                                                                                      |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| Calibnet | [`0x73a43475aa2ccb14246613708b399f4b2ba546c7`](https://calibration.filfox.info/en/address/0x73a43475aa2ccb14246613708b399f4b2ba546c7) |
| Mainnet  | [`0x9BB686Ba6a50D1CF670a98f522a59555d4977fb2`](https://filecoin.blockscout.com/address/0x9BB686Ba6a50D1CF670a98f522a59555d4977fb2)    |

## Comparison Testing with Anvil

Anvil uses **Geth style** tracing (`debug_traceCall` with `prestateTracer`), while Forest uses **Parity style** tracing (`trace_call` with `stateDiff`). This makes Anvil useful for comparison testing — verifying that Forest produces semantically equivalent results in a different format.

### Prerequisites

- [Foundry](https://book.getfoundry.sh/getting-started/installation) installed (`forge`, `cast` commands)
- A running [Forest node](https://docs.forest.chainsafe.io/getting_started/syncing) and Anvil instance

### What is Anvil?

[Anvil](https://getfoundry.sh/anvil/reference/) is a local Ethereum development node included with Foundry. It provides:

- Instant block mining
- Pre-funded test accounts (10 accounts with 10,000 ETH each)
- Support for `debug_traceCall` with various tracers
- No real tokens required

### Starting Anvil

```bash
# Start Anvil with tracer to allow debug_traceCall API's
anvil --tracing
```

Anvil RPC endpoint: `http://localhost:8545`

### Deploying Contract on Anvil

```bash
forge create src/tool/subcommands/api_cmd/contracts/tracer/Tracer.sol:Tracer \
    --rpc-url http://localhost:8545 \
    --broadcast \
    --private-key <ANVIL_OUTPUT_PRIVATE_KEY>

# Output:
# Deployer: 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
# Deployed to: 0x5FbDB2315678afecb367f032d93F642f64180aa3
# Transaction hash: 0x...
```

### Comparing Forest vs Anvil Responses

The same contract call can be tested against both nodes. The request data payloads are identical; only the method name and parameter ordering differ.

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

**Anvil (Geth style `debug_traceCall`):**

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

## Official Resources

- [OpenEthereum trace module](https://openethereum.github.io/JSONRPC-trace-module)
- [Geth Built-in Tracers](https://geth.ethereum.org/docs/developers/evm-tracing/built-in-tracers)
- [Alchemy: `trace_call` vs `debug_traceCall`](https://www.alchemy.com/docs/reference/trace_call-vs-debug_tracecall)
- [Reth trace Namespace](https://reth.rs/jsonrpc/trace)
- [Foundry Book - Anvil](https://book.getfoundry.sh/reference/anvil/)
