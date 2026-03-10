# `debug_traceTransaction` Developer Guide

This guide covers testing and development workflows for Forest's `debug_traceTransaction` implementation. For API documentation and user-facing usage, see the [API guide](/knowledge_base/rpc/debug_trace_transaction).

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

Anvil uses the same **Geth style** tracing as `debug_traceTransaction`, making it ideal for direct comparison testing — verifying that Forest produces identical or semantically equivalent results.

### Prerequisites

- [Foundry](https://book.getfoundry.sh/getting-started/installation) installed (`forge`, `cast` commands)
- A running [Forest node](https://docs.forest.chainsafe.io/getting_started/syncing) and Anvil instance

### What is Anvil?

[Anvil](https://getfoundry.sh/anvil/reference/) is a local Ethereum development node included with Foundry. It provides:

- Instant block mining
- Pre-funded test accounts (10 accounts with 10,000 ETH each)
- Support for `debug_traceTransaction` with various tracers
- No real tokens required

### Starting Anvil

```bash
# Start Anvil with tracer to allow `debug_traceTransaction` API's
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

### Sending Test Transactions on Anvil

Unlike `trace_call` (which simulates calls), `debug_traceTransaction` traces mined transactions. You must first send transactions to get transaction hashes:

```bash
# Set variables
export ANVIL_RPC="http://localhost:8545"
export ANVIL_KEY="0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"  # Anvil default key
export ANVIL_CONTRACT="0x5FbDB2315678afecb367f032d93F642f64180aa3"  # Deployed address

# Send transactions
cast send $ANVIL_CONTRACT "setX(uint256)" 123 \
    --rpc-url $ANVIL_RPC --private-key $ANVIL_KEY

cast send $ANVIL_CONTRACT "deposit()" \
    --value 1ether --rpc-url $ANVIL_RPC --private-key $ANVIL_KEY

cast send $ANVIL_CONTRACT "storageAdd(uint256)" 100 \
    --rpc-url $ANVIL_RPC --private-key $ANVIL_KEY

cast send $ANVIL_CONTRACT "callSelf(uint256)" 456 \
    --rpc-url $ANVIL_RPC --private-key $ANVIL_KEY

cast send $ANVIL_CONTRACT "storageMultiple(uint256,uint256,uint256)" 10 20 30 \
    --rpc-url $ANVIL_RPC --private-key $ANVIL_KEY
```

Save the transaction hashes from the output for use in the tracing examples below.

### Getting the correct transaction hash on Forest

`debug_traceTransaction` expects the canonical **EthHash** (0x...). If your client returned something else, resolve it first.

- **0x... value** (e.g. from `cast send`): call `eth_getTransactionByHash` and use the response's **`hash`** field for tracing. Forest resolves via the indexer or by message lookup.
- **Literal CID** (e.g. `bafy2bzace...`): `eth_getTransactionByHash` accepts only EthHash. Use `eth_getTransactionHashByCid` to get the hash, then pass it to `debug_traceTransaction`.

Example (0x... from cast):

```bash
HASH_0X="0x..."   # from cast send or block explorer
TX_HASH=$(curl -s -X POST http://localhost:2345/rpc/v1 -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","id":1,"method":"eth_getTransactionByHash","params":["'"$HASH_0X"'"]}' \
    | jq -r '.result.hash // empty')

curl -s -X POST "http://localhost:2345/rpc/v1" -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","id":1,"method":"debug_traceTransaction","params":["'"$TX_HASH"'",{"tracer":"prestateTracer","tracerConfig":{"diffMode":true}}]}'
```

If you have a CID: `TX_HASH=$(curl -s ... -d '{"method":"eth_getTransactionHashByCid","params":["'"$MSG_CID"'"]}' | jq -r '.result // empty')`, then use `$TX_HASH` in the trace call above.

### Comparing Forest vs Anvil Responses

Both Forest and Anvil use the same `debug_traceTransaction` method and tracer format, so responses can be compared directly.

**Forest** (use the canonical `hash` from `eth_getTransactionByHash` as described above):

```bash
curl -s -X POST "http://localhost:2345/rpc/v1" \
    -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "debug_traceTransaction",
        "params": [
            "'$TX_HASH'",
            {"tracer": "prestateTracer", "tracerConfig": {"diffMode": true}}
        ]
    }'
```

**Anvil:**

```bash
curl -s -X POST "http://localhost:8545" \
    -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "debug_traceTransaction",
        "params": [
            "'$TX_HASH'",
            {"tracer": "prestateTracer", "tracerConfig": {"diffMode": true}}
        ]
    }'
```

### Expected Differences Between Forest and Anvil

When comparing `debug_traceTransaction` output from Forest and Anvil, expect these `Filecoin-specific` differences:

| Aspect                 | Forest (Filecoin)                          | Anvil (Ethereum)               |
| ---------------------- | ------------------------------------------ | ------------------------------ |
| **Extra addresses**    | Includes `0xff00...` Filecoin ID addresses | Only EVM addresses             |
| **Coinbase**           | Not included (gas at protocol level)       | Included as `0x0000...0000`    |
| **Nonce format**       | Hex string (e.g., `"0x1e"`)                | Integer (e.g., `30`)           |
| **Balance format**     | Hex string (e.g., `"0xde0b6b3a7640000"`)   | Hex string (same)              |
| **Storage key format** | Full 32-byte padded hex                    | Full 32-byte padded hex (same) |

### Test Scenarios

When testing `debug_traceTransaction`, cover these categories:

#### 1. `prestateTracer` Tests

| Test Case                   | Function                    | What to Verify                                              |
| --------------------------- | --------------------------- | ----------------------------------------------------------- |
| Simple storage write        | `setX(123)`                 | `Pre-state` shows old value, `post-state` shows new value   |
| ETH deposit                 | `deposit()` with value      | Balance changes in `pre/post` for sender and contract       |
| Storage add (empty → value) | `storageAdd(100)`           | Storage slot absent in `pre`, present in `post` (diff mode) |
| Storage change              | `storageChange(200)`        | Storage slot values differ between `pre` and `post`         |
| Multiple storage writes     | `storageMultiple(10,20,30)` | Multiple storage slots change in single transaction         |
| Default mode (no diff)      | `setX(123)`                 | Only `pre-state` returned, no post object                   |

#### 2. `callTracer` Tests

| Test Case       | Function            | What to Verify                               |
| --------------- | ------------------- | -------------------------------------------- |
| Simple call     | `setX(123)`         | Single top-level CALL frame                  |
| Nested call     | `callSelf(456)`     | Parent CALL with child CALL in `calls` array |
| Delegate call   | `delegateSelf(789)` | `DELEGATECALL` type in call frame            |
| Deep recursive  | `deepTrace(3)`      | N-level nested CALL hierarchy                |
| Complex mixed   | `complexTrace()`    | Mix of CALL, `DELEGATECALL`, `STATICCALL`    |
| Revert at depth | `failAtDepth(3,1)`  | Error field populated in failing frame       |

#### 3. `flatCallTracer` Tests

| Test Case      | Function        | What to Verify                                          |
| -------------- | --------------- | ------------------------------------------------------- |
| Simple call    | `setX(123)`     | Single flat trace entry                                 |
| Nested call    | `callSelf(456)` | Two entries with correct `traceAddress` and `subtraces` |
| Deep recursive | `deepTrace(3)`  | Flat list with incrementing `traceAddress` depth        |

## Official Resources

- [Geth `debug_traceTransaction`](https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-debug#debugtracetransaction)
- [Geth Built-in Tracers](https://geth.ethereum.org/docs/developers/evm-tracing/built-in-tracers)
- [Reth debug Namespace](https://reth.rs/jsonrpc/debug)
- [Foundry Book - Anvil](https://book.getfoundry.sh/reference/anvil/)
- [Foundry Book - Cast](https://book.getfoundry.sh/reference/cast/)
