# Preliminary reading about Actor Events:

#### The Filecoin Actor Events FIP:

https://github.com/filecoin-project/FIPs/blob/master/FIPS/fip-0049.md

The event type looks as follows:

```rust
struct Event {
    ...,
    event: ActorEvent,
}

type ActorEvent = Vec<EventEntry>;

struct EventEntry {
    flags: u64,
    codec: u64,
    key: String,
    value: Bytes,
}
```

**Note:** Events can be indexed by value and/or key, which is indicated by the
flags member in the `EventEntry` (serve merely as client hints to signal
indexing intent).

#### EVM Events documentation:

https://docs.soliditylang.org/en/v0.8.27/contracts.html#events

https://github.com/filecoin-project/lotus/blob/master/chain/types/event.go

#### Ethereum LOG opcodes implementation:

https://github.com/filecoin-project/builtin-actors/blob/master/actors/evm/src/interpreter/instructions/mod.rs#L343C1-L347

https://github.com/filecoin-project/ref-fvm/blob/master/fvm/src/syscalls/event.rs

# How Lotus retrieve events from its chain store?

The `EventFilterManager` load the executed messages following those steps:

1. Read chain store.
2. Get messages from the specified tipset.
3. Extract parent message receipts from the tipset and verify that the lengths
   match between messages and receipt.
4. Populate the vector of executed messages.
5. Extract events from the receipts.

Executed messages are obtained using the `CollectEvents` function through a
filter view. These messages are packed into a `CollectedEvent` vector and stored
in the `eventFilter::collected` member. You can access them using the
`TakeCollectedEvents` function.

**Conclusion**: While Lotus implements a _Chain Index_, we don't really need it
to get actor events.

# Lotus Chain Index:

- A collection of SQL Data Definition Language (DDL) statements.
- SQL tables like `event`, `event_entry`
- Indexes

Indexes are used to make lookups faster, especially for queries that involve
sorting or filtering based on certain fields. The index tables defined are:

- `event_height`: event (height)
- `event_entry_event_id`: event_entry (event_id)

The Query Plan utilizes tables and indexes to select events, employing join
operations, sorting, and other methods.

**SQLite** is used internally and can grow significantly (over 40 GiB) depending
on how much historical state is indexed.

# Forest implementation design choices:

# A. Follow Lotus route

## Implement indexes and use SQL for querying events

**Question:** What are our options for Rust in this context?

1. **Rusqlite**?
2. **Sqlx** crate? In case of abstraction of the backend DB:
   - **PostgreSQL**
   - **SQLite**
   - Other options
3. Other crates?

## Implement indexes but with a different implementation

1. Use a graph database or a graph query language:
   - **Neo4j**
   - **GraphQL**

   Justifying this choice is challenging without a solid understanding of these
   technologies and the problem at hand.

2. Choose a similar SQL database but use alternative options for our DDL
   statements.

# B. Do something different

## Do not use indexes or make any attempts to optimize queries

The Rust-based Ethereum client **Reth** uses an index-less approach.

The primary optimization **Reth** employs is an in-memory LRU cache (named
`EthStateCache`) to reduce database lookups (it uses MDBX internally) and to
manage various caches, such as a receipts cache. It's hard to say how much this
speeds up operations, though.

# Conclusion

Hold off for now and take a Reth-like approach, but without memory caching.

**Rationale:**

- Before optimizing any queries, it's essential to first establish a baseline.
  Otherwise, how can we be sure we're optimizing a specific type of scenario?

  Determine whether indexing is absolutely necessary, and if so, for which types
  of queries it could be beneficial.

- It’s not guaranteed that these kinds of optimizations should occur at the node
  level, or at all. For example, a service like **Filfox** might not require
  such optimizations and could bear the cost of this feature. Additionally, an
  RPC provider might implement its own optimization layer for event queries that
  is independent of its backend (**Lotus** or **Forest**).

- A better understanding of the system could be achieved by implementing it
  initially without any optimizations. Performance tests, such as running
  queries with an increasing number of active filters, can help inform decisions
  about the technology to use for optimizing queries in a subsequent step.

- Implementing a complex system like an indexer can lead to potential bugs or
  inconsistencies in the results (for instance, see
  [issue](https://github.com/filecoin-project/lotus/issues/12254)).

  Also if we introduce a new DB, we need to manage the increased complexity
  associated with handling chain reorgs, DB migrations, populating DB with
  historical data, GC, etc.

- Lastly, and perhaps most importantly, the Lotus index implementation is
  currently undergoing a significant overhaul:

  https://github.com/filecoin-project/lotus/issues/11594

  https://github.com/filecoin-project/lotus/pull/12421

  We might want to wait until their new implementation stabilizes.
