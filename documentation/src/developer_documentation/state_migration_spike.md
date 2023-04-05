## State migration spike 🛂

### What is state migration?

State migration is a process where the `StateTree` contents are transformed from
an older form to a newer form. Certain Actors may need to be created or migrated
as well.

### Why do we need to migrate?

Migration is required when the structure of the state changes. This happens when
new fields are added or existing ones are modified. Migration is **not**
required in case of new behaviour.

In case of NV18, the `StateTree` changed from version 4 to version 5. See
https://github.com/filecoin-project/ref-fvm/pull/1062

### What to upgrade?

We need to upgrade the `StateTree` which is represented as
`HAMT<Cid, ActorState>` to the latest version.

On top of that, we need to migrate certain actors. In the case of NV18 upgrade,
it's the
[init](https://github.com/filecoin-project/go-state-types/blob/d8fdbda2ad86de55bcde7f567c6da9c5f430c7a1/builtin/v10/migration/init.go#L32)
and
[system](https://github.com/filecoin-project/go-state-types/blob/master/builtin/v10/migration/system.go#L24)
actor.
[EAM](https://github.com/filecoin-project/go-state-types/blob/master/builtin/v10/migration/eam.go)
actor needs to be created.

### When to upgrade?

There is a separate upgrade schedule for each network. In Lotus, it is defined
in
[upgrades.go](https://github.com/filecoin-project/lotus/blob/dbbcf4b2ee9626796e23a096c66e67ff350810e4/chain/consensus/filcns/upgrades.go#L83).
In Venus, in
[fork.go](https://github.com/filecoin-project/venus/blob/master/pkg/fork/fork.go)
which has the same structure.

For the case of NV18, it is defined as

```go
Height:    build.UpgradeHyggeHeight,
Network:   network.Version18,
Migration: UpgradeActorsV10,
PreMigrations: []stmgr.PreMigration{{
	PreMigration:    PreUpgradeActorsV10,
	StartWithin:     60,
	DontStartWithin: 10,
	StopWithin:      5,
}},
Expensive: true,
```

### How to upgrade?

Iterate over the state of each actor at the given epoch and write the new state
along with any specific changes to the respective state. This involves iterating
over each of the HAMT nodes storing the state and writing them to the database.

[Lotus upgrade method](https://github.com/filecoin-project/lotus/blob/58900a70333a11a903cf9fe3f29e6a5c309cb000/chain/consensus/filcns/upgrades.go#L1591-L1612)
and the
[module](https://github.com/filecoin-project/go-state-types/tree/master/builtin/v10/migration)
dedicated to Actors `v10` migration. The core logic is
[here](https://github.com/filecoin-project/go-state-types/blob/master/builtin/v10/migration/top.go#L28).
The same module is used by Venus.

Forks migrations: handled by
[fork.go](https://github.com/filecoin-project/lotus/blob/58900a70333a11a903cf9fe3f29e6a5c309cb000/chain/stmgr/forks.go#L42-L53)
entities.

### Where to upgrade?

It should be done most likely in the apply blocks method.

[Lotus](https://github.com/filecoin-project/lotus/blob/74d94af03418c799350fc0f40d3758c23cd82ab8/chain/consensus/compute_state.go#L178):

```go
// handle state forks
// XXX: The state tree
pstate, err = sm.HandleStateForks(ctx, pstate, i, em, ts)
if err != nil {
	return cid.Undef, cid.Undef, xerrors.Errorf("error handling state forks: %w", err)
}
```

In
[Forest](https://github.com/ChainSafe/forest/blob/main/blockchain/state_manager/src/lib.rs#L421-L424)
we already have a hint from the past:

```rust
if epoch_i == turbo_height {
    todo!("cannot migrate state when using FVM - see https://github.com/ChainSafe/forest/issues/1454 for updates");
}
```

We can try with something simplistic to get it running, it's not an issue.
Afterwards we can implement a proper schedule with functors.

### Challenges

- Doing the state migration efficiently; we need to traverse every entry in the
  state trie. Lotus does pre-migration which are filling relevant caches to
  speed up the eventual full migration at the upgrade epoch. We might need to do
  something like this as well; it might not be necessary for the first
  iteration - depends on how performant the migration process would be in the
  Forest itself.
- No test network. While we can use existing snapshots from before the upgrade
  to test state migration, it is not sustainable if we want to continuously
  support calibration network. We either require a local devnet for testing
  migration **before** they actually happen on real networks or we can try
  supporting more bleeding-edge networks. The former approach is more solid, but
  the latter might be easier to implement at first (and would give Forest more
  testnets support which is always welcome).
- There may be forks, so we probably need to keep the pre-migration and
  post-migration state in two caches for some back and forths. This in Lotus is
  handled with
  [HandleStateForks](https://github.com/filecoin-project/lotus/blob/f641139bf237e6e955a9a2f33cfc05ba52430b1b/chain/stmgr/forks.go#L175).
- For EAM Actor we may need some Ethereum methods we have not yet implemented.
  Perhaps what `builtin-actors` and `ref-fvm` expose will be enough.

### Current Forest implementation

For the moment Forest does not support migrations. The
[code](https://github.com/ChainSafe/forest/blob/state-migration-spike/vm/state_migration/src/lib.rs)
that was meant for this is not used at the moment. Most probably we will be able
to utilise it.

### Plan

We should start by adding an `nv18` to the state migration
[crate](https://github.com/ChainSafe/forest/tree/state-migration-spike/vm/state_migration/src),
along the lines of the
[Go equivalent](https://github.com/filecoin-project/go-state-types/blob/master/builtin/v10/migration/init.go).
Most likely this would mean adding some missing structures, related to the `v10`
actors (Ethereum ones).

Then try to plug it in
[apply_blocks](https://github.com/ChainSafe/forest/blob/main/blockchain/state_manager/src/lib.rs#L421-L424).
This may work for calibration network. Afterwards, we will most likely need to
iterate to achieve acceptable performance for mainnet. Some ideas on how to
achieve this can be taken from Lotus/Venus, e.g., pre-migration caching.

### Sources

- Rahul's article: https://hackmd.io/@tbdrqGmwSXiPjxgteK3hMg/r1D6cVM_u
- Lotus codebase - https://github.com/filecoin-project/lotus
- Venus codebase - https://github.com/filecoin-project/venus
