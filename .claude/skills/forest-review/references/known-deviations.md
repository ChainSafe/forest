# Known deviations

Behaviour that looks like a Lotus deviation or a bug and is settled: deliberate deviations and faithful ports. A change that keeps an entry's behaviour gets no parity finding. A change that alters it is a deviation like any other. An entry whose permalink or Forest location no longer supports it is itself a finding (class `unverified-lotus-claim`); re-verify at the reference revision and master, then update the date.

Entry format: `##` title, one-line description, Forest location, Lotus permalink pinned to a tag or 40-hex SHA, `verified: YYYY-MM-DD` with the revisions checked.

## `accessList` on eth transactions

Deliberate, geth-style: Forest omits `accessList` on legacy transactions and emits `[]` on typed ones; Lotus has no `omitempty` and emits `null` for eth-originated transactions (`[]` only for native messages).

- Forest: `src/rpc/methods/eth.rs` field `ApiEthTx::access_list`, locked by the `access_list_serialization` cases `explicit_none_omitted` and `legacy_homestead_omitted`
- Lotus: https://github.com/filecoin-project/lotus/blob/29d72bdf77d13e6e3e20264d7a9167feb2a41ab0/chain/types/ethtypes/eth_transactions.go#L84
- verified: 2026-11-03 (v1.38.3, master `599e9c8c5bb8`)

## eth_subscribe logs on reorg

Faithful: a reverted tipset re-emits the logs of its parent's messages with `removed: true`, so a reorg removes and re-adds the common ancestor's logs; the net delta is correct.

- Forest: `src/rpc/methods/eth.rs` function `eth_logs_for_head_change`
- Lotus: https://github.com/filecoin-project/lotus/blob/29d72bdf77d13e6e3e20264d7a9167feb2a41ab0/chain/events/filter/event.go#L328-L344
- verified: 2026-11-03 (v1.38.3, master `599e9c8c5bb8`)

## Null rounds in by-number eth methods

Faithful: `eth_getBlockByNumber`, `eth_getBlockTransactionCountByNumber`, `eth_getTransactionByBlockNumberAndIndex`, `eth_getBlockReceipts`, `eth_getBlockReceiptsLimited`, `trace_block`, and `trace_replayBlockTransactions` fail at a null round with code 12 and `requested epoch was a null round (<epoch>)` (the `strict` branch).

- Forest: `src/rpc/methods/eth.rs` calls with `ResolveNullTipset::Fail`; `src/rpc/methods/eth/errors.rs` `NULL_ROUND_CODE` and `EthErrors::NullRound`
- Lotus: https://github.com/filecoin-project/lotus/blob/29d72bdf77d13e6e3e20264d7a9167feb2a41ab0/node/impl/eth/tipsetresolver.go#L183-L205
- verified: 2026-11-03 (v1.38.3, master `599e9c8c5bb8`)

## Mpool chain re-sort after merge

Deliberate: after merging a chain, Lotus re-sorts the tail `chains[i+1:]` with a comparator that indexes the full `chains` slice, so it compares the wrong elements; Forest sorts the tail by its own elements, stably. Block production only, not consensus.

- Forest: `src/message_pool/msgpool/selection.rs` call `sort_range_effective(i + 1..)`, implemented in `src/message_pool/msg_chain.rs`
- Lotus: https://github.com/filecoin-project/lotus/blob/29d72bdf77d13e6e3e20264d7a9167feb2a41ab0/chain/messagepool/selection.go#L333-L337
- verified: 2026-11-03 (v1.38.3, master `599e9c8c5bb8`)
