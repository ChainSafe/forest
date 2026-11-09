# Parity lens

Done when every changed consensus, protocol, or RPC-output behaviour has a Lotus permalink at the reference revision, plus the master commit your prompt names when they differ, and a row in the deviation table.

The implementation may differ from Lotus; observable results match for every input class, edge cases and errors included. Reading Lotus and building permalinks: `references/report-format.md` "Evidence and Lotus reads". Settled behaviour that gets no finding: `references/known-deviations.md`.

## Kinds of behaviour

- Consensus: block and message validation, state transition, gas, randomness and beacon entries, network-version gates, migrations, weight, fork choice. Any deviation is `consensus-deviation`, also where Lotus looks wrong; the fix matches Lotus and the report suggests raising it upstream.
- Protocol: CBOR encoding, gossip validation outcomes, chain exchange, hello, bitswap. A deviation is `protocol-deviation`.
- RPC output: JSON shape, field casing, `omitempty` vs `null`, Go nil slice `null` vs `[]`, hex vs decimal, array ordering, error codes and messages, null-round behaviour. A deviation is `rpc-deviation` unless the diff justifies it and the changelog names it. A `.context` added to an error that reaches an RPC response replaces the message callers see (`src/rpc/error.rs`). For `eth_*` methods, compare Geth and Reth as well.
- Bounds: batch length, body size, filter range, lookback, nonce gap, pending messages per sender, and similar limits. A value that differs from Lotus's without a stated reason is `bound-deviation`. Enforcement belongs to the adversarial lens.

## Input classes

Compare Forest and Lotus on each of these for every changed behaviour:

- Every element of a peer-supplied list. Lotus validates the list as supplied; iterating only the expected range lets extra, trailing, or duplicate entries through unchecked.
- Every network version the code path reaches, historical mainnet epochs included: early mainnet ran randomness, beacon, and actor rules that calibnet history never exercised. Check the oldest epoch an RPC method accepts as well as the head.
- Errors, empty input, and absent values: what Lotus returns, rejects, or skips.

## Pitfalls

- Comparison direction, `<` vs `<=`, above all for network version at an upgrade epoch: Lotus `GetNetworkVersion` treats the upgrade epoch itself as the old version (`height <= atOrBelow`, <https://github.com/filecoin-project/lotus/blob/29d72bdf77d13e6e3e20264d7a9167feb2a41ab0/chain/stmgr/stmgr.go#L537-L546>), while some gates compare raw height (`>=`, for example <https://github.com/filecoin-project/lotus/blob/29d72bdf77d13e6e3e20264d7a9167feb2a41ab0/chain/store/messages.go#L140>). Read which form the Lotus counterpart uses.
- Message size with vs without the signature.
- ID vs robust address keying.
- Go wrapping integer arithmetic vs Rust checked or trapping arithmetic.
- Big-integer division: `types.BigDiv` and `big.Div` are Euclidean (Go `big.Int.Div`), `big.Int.Quo` truncates like Rust `/`. Read which one the Lotus code calls.
- Sort stability. Hash-map iteration order belongs to the correctness lens (`nondeterminism`).

## Evidence

- Every claim about Lotus behaviour carries a permalink; a claim without one is `unverified-lotus-claim`.
- Parity claims in code comments: `precheck.txt` flags the missing link (`matches-lotus`); this lens checks whether the claim is true.
- Evidence ladder: code reading with a permalink, then differential tests (RPC snapshot tests in `src/tool/subcommands/api_cmd/test_snapshots.txt`, `forest-tool api compare`), then a devnet run. A consensus change backed by code reading only gets a `differential-evidence-missing` finding that names the command or test that would produce the evidence.

## Deviation table

In `parity.md`, the table defined in report-format.md "Lens report", one row per changed consensus, protocol, or RPC-output behaviour, matching rows included. "Same?" reads `yes`, `no`, or `differs on master`; "Changelog" reads `present`, `needed`, or `n/a`.
