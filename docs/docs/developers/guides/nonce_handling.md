# Nonce Handling

This guide documents how Forest calculates, assigns, and manages message
`nonces` (sequence numbers).

## What is a nonce?

Every Filecoin message carries a **sequence number** (nonce) that must equal the
sender's current actor nonce on-chain. The VM enforces strict sequential
ordering: nonce 0, then 1, then 2, and so on. A message with a nonce that does
not match the expected value is rejected.

The message pool is responsible for tracking which nonce to assign next, both
from on-chain state and from pending (not-yet-included) messages.

## State nonce calculation

The **state nonce** is the next expected nonce derived from on-chain data. It is
computed by `get_state_sequence` in `src/message_pool/msgpool/msg_pool.rs`:

1. Fetch the actor's on-chain sequence from the parent state.
2. Scan messages already included in the **current tipset** (which have not yet
   been reflected in the parent state). For each message from the same sender,
   advance the nonce if `msg.sequence + 1 > next_nonce`.
3. Cache the result keyed by `(TipsetKey, Address)` so repeated lookups within
   the same tipset are free.

Address resolution happens transparently: ID addresses (e.g. `f0123`) are
resolved to their deterministic key form (e.g. `f3...`) via `resolve_to_key`,
with results cached in `key_cache`.

## Pending nonce (`MsgSet`)

Each sender in the message pool has a `MsgSet` that tracks pending messages. The
key field is `next_sequence`, which represents the **first gap nonce** -- the
lowest nonce for which no pending message exists.

### Adding a message (`MsgSet::add`)

When a message arrives:

- If its nonce equals `next_sequence`, increment `next_sequence` and advance
  past any consecutive existing messages (gap-filling loop).
- If its nonce exceeds `next_sequence + MAX_NONCE_GAP` (4) and the call is
  `strict`, reject with `NonceGap`.
- If its nonce is above `next_sequence` but within the gap limit, accept it and
  mark a nonce gap.
- Replace-by-fee (`RBF`) for an existing nonce is rejected when `strict` and a
  nonce gap is present.

The `strict` and `trusted` parameters are independent:

| Parameter | Derived from             | Controls                                            |
| --------- | ------------------------ | --------------------------------------------------- |
| `strict`  | `!local` in `add_tipset` | Whether nonce gap checks run                        |
| `trusted` | `TrustPolicy`            | `MAX_NONCE_GAP` (4 vs 0) and pending message limits |

### Removing a message (`MsgSet::rm`)

- **Applied** (on-chain): advance `next_sequence` to `nonce + 1` if needed. For
  unknown messages (not in our pool), also run the gap-filling loop to advance
  past consecutive known messages.
- **Pruned** (evicted): rewind `next_sequence` to the removed nonce if it
  creates a gap.

### Effective nonce (`get_sequence`)

`MessagePool::get_sequence` returns `max(state_nonce, pending_next_sequence)`,
giving the next nonce that should be assigned to a new message.

## Locking strategy

Forest uses a two-tier locking strategy, similar to Lotus (`MpoolLocker` +
`MessageSigner.lk`):

### Per-sender lock (`MpoolLocker`)

`MpoolLocker` maintains a `HashMap<Address, Arc<Mutex<()>>>` behind a
synchronous mutex. Each sender gets its own async mutex, so concurrent
`MpoolPushMessage` calls for different senders proceed in parallel while calls
for the same sender are serialized.

This lock covers the entire RPC critical section -- from gas estimation through
the final push -- preventing a second request from reading stale nonce state
while the first is still in-flight.

### Global nonce lock (`NonceTracker`)

`NonceTracker` holds a single global `tokio::sync::Mutex<()>`. It serializes the
narrow window of nonce-read, sign, push, and persist across all senders, ensuring
no two messages are assigned the same nonce even under high concurrency.

### Why two locks?

The per-sender lock prevents a broader class of races (e.g., two requests
reading gas estimates that both assume the same balance). The global lock
prevents nonce collisions specifically. Separating them allows gas estimation to
proceed in parallel for different senders while the nonce-critical section
remains serialized.

## Nonce persistence

`NonceTracker` persists the next expected nonce per address at key
`/mpool/nonces/{addr}` in the `SettingsStore` (backed by the node's database).

On restart, `next_nonce` returns `max(mpool_nonce, persisted_nonce)`:

- If a message was pushed and persisted but not yet included on-chain, the
  persisted nonce prevents reuse.
- If the `mpool` nonce is higher (e.g., messages arrived via gossip), the
  `mpool` value is used and a warning is logged.

The nonce is only persisted **after** a successful push. If signing or pushing
fails, the nonce is not consumed and can be reused.

## Chain reorganization

`head_change` in `src/message_pool/msgpool/mod.rs` handles tipset revert/apply:

- **Apply**: messages included in the new tipset are removed from the pending
  pool via `MsgSet::rm(nonce, applied=true)`.
- **Revert**: messages from the reverted tipset are re-added to the pool with
  `strict=false` and `TrustPolicy::Trusted`, allowing them back without nonce
  gap restrictions.

The state nonce cache is naturally invalidated when the tipset changes, since it
is keyed by `TipsetKey`.
