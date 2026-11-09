# Correctness lens

Done when every changed function has been read against each of its callers, every fix has a stated root cause with a confidence level under `## Root cause` in `correctness.md`, and every sibling pattern found by grep has a verdict under `## Sibling patterns` (pattern, grep command, each site, and `same defect`, `not affected`, or `unknown`).

Severity and evidence: `references/report-format.md`.

## Checklist

- `root-cause`: the change fixes the linked issue or stated problem at its cause. A repro that fails before and passes after shows the cause; a fallback, widened tolerance, or swallowed error hides a symptom (a retry on a network or peer transient is not one). State the confidence: proven by repro, argued from code, or guessed.
- `blast-radius`: every caller of each changed function, through trait implementations and macro registrations (`for_each_rpc_method!`) too. A shared helper that now does more (loads parent state, reads the network, takes a lock) changes every caller, including callers on garbage-collected state.
- `sibling-pattern`: the same defect elsewhere (grep the pattern the fix removes); symmetric paths (`eth_call`, `eth_estimateGas`, and `debug_traceCall`; trace and replay; by-hash and by-number; HTTP and WebSocket); every network; both directions of a conversion (encode and decode, to and from Lotus JSON). The finding sits on the changed line and lists the other sites in `evidence`.
- `wrong-result`: values compared in canonical form (digests, hashes, CIDs, and addresses as bytes or typed values, never as encoded strings, where case, prefix, and padding differ); units (attoFIL vs FIL, epochs vs seconds); values round-tripped through `Display` and parsed back (a path printed then parsed as a URL).
- `stale-leftover`: variables, config fields, comments ("360 seconds"), tests, scripts, and docs the change made dead or wrong. Grep the old name and the old value.
- `concurrency`: `select!` cancellation safety; events emitted before the subscriber exists; `broadcast` receivers lagging and dropping; events delivered after disconnect; lock ordering; check-then-use races.
- `boundary`: genesis and epoch 0, null rounds, reorg revert and apply, the upgrade activation epoch, finality depth, empty input, `-1` sentinels, maximum values.
- `upgrade-time-bomb`: code, tests, or RPC snapshot tests that break when the next network version activates on calibnet or mainnet; comments and docs worded so they need editing at every upgrade.
- `nondeterminism`: hash-map iteration order reaching output, state, or consensus; wall-clock time or local randomness in validation.
