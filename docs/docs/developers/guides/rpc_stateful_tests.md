# RPC Stateful Tests

Some methods in the Filecoin Ethereum JSON-RPC API require stateful interactions for meaningful testing. These tests validate both **schema compatibility** and **method semantics**, especially for RPC endpoints that rely on internal node state.

This includes:

- All subscription-based methods
- Filter-related methods (e.g., `eth_newFilter`, `eth_getFilterLogs`)

## Prerequisites

Before running the tests, perform the following setup steps:

1. Run a Lotus or Forest node (calibnet recommended). Make sure `FULLNODE_API_INFO` is defined.
2. Create a f4 address, fund it, and deploy a test smart contract (the deployed contract must emit an event with a known topic when invoked).
3. The f4 address must hold enough FIL to invoke the contract.

   Run the test suite with:
   `forest-tool api test-stateful --to <CONTRACT_ADDR> --from <FROM_ADDR> --payload <INVOKE_PAYLOAD> --topic <TOPIC>`

   where:
   - `CONTRACT_ADDR`: f4 address of the deployed smart contract
   - `FROM_ADDR`: f4 address invoking the contract
   - `INVOKE_PAYLOAD`: Calldata that will trigger the contract's event
   - `TOPIC`: The event topic expected to be emitted during invocation

## Example output

```console
export FULLNODE_API_INFO="<TOKEN>:/ip4/127.0.0.1/tcp/1234/http"
forest-tool api test-stateful \
  --to t410f2jhqlciub25ad3immo5kug2fluj625xiex6lbyi \
  --from t410f5uudc3yoiodsva73rxyx5sxeiaadpaplsu6mofy \
  --payload 40c10f19000000000000000000000000ed28316f0e43872a83fb8df17ecae440003781eb00000000000000000000000000000000000000000000000006f05b59d3b20000 \
  --topic 0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef
running 7 tests
test eth_newFilter install/uninstall ... ok
test eth_newFilter under limit ... ok
test eth_newFilter just under limit ... ok
test eth_newFilter over limit ... ok
test eth_newBlockFilter works ... ok
test eth_newPendingTransactionFilter works ... ok
test eth_getFilterLogs works ... ok
test result: ok. 7 passed; 0 failed; 0 ignored; 0 filtered out
```

The goal is to ensure that Forest now passes all the existing scenarios. These scenarios are not exhaustive, and additional ones can be added as needed.

## Adding a new test

To extend test coverage for another RPC method or cover more semantics:

1. Add the RPC method to Forest if not yet implemented, following the guidance in [RPC compatibility guide](./rpc_api_compatibility).
2. Create a new test scenario in the `src/tool/subcommands/api_cmd/stateful_tests.rs` file
3. Your internal test function should return `Ok(())` on success. Use `anyhow::Result` for error handling.

   Ensure the test behaves consistently on both Lotus and Forest nodes.

## Example test function

```rust
pub async fn test_eth_method(client: Arc<Client>) -> anyhow::Result<()> {
    // Setup call to the method
    // Assert intermediate states
    // State cleanup
    // Return Ok when the sequence completes successfully
    Ok(())
}
```

## Notes

The current test framework assumes a running node and a valid wallet.

Consider implementing `forest-tool evm deploy` and `forest-tool evm invoke` subcommands to simplify contract deployment and test invocation.
