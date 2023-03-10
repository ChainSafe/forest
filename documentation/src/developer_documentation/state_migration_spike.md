## State migration spike  🛂

### What is state migration?

State migration is a process where the `StateTree` contents are transformed from an older form to a newer form. 

### Why do we need to migrate?
Migration is required when the structure of the state changes. This happens when new fields are added or existing ones are modified. Migration is **not** required in case of new behaviour.

In case of NV18, the `StateTree` changed from version 4 to version 5. See https://github.com/filecoin-project/ref-fvm/pull/1062

### What to upgrade?

We need to upgrade the `StateTree` which is represented as `HAMT<Cid, ActorState>`.

### When to upgrade?

There is a separate upgrade schedule for each network. In Lotus, it is defined in [upgrades.go](https://github.com/filecoin-project/lotus/blob/dbbcf4b2ee9626796e23a096c66e67ff350810e4/chain/consensus/filcns/upgrades.go#L83).

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

Iterate over the state of each actor at the given epoch and write the new state along with any specific changes to the respective state. This involves iterating over each of the HAMT nodes storing the state and writing them to the database.

[Lotus upgrade method](https://github.com/filecoin-project/lotus/blob/58900a70333a11a903cf9fe3f29e6a5c309cb000/chain/consensus/filcns/upgrades.go#L1591-L1612) and the [module](https://github.com/filecoin-project/go-state-types/tree/master/builtin/v10/migration) dedicated to Actors `v10` migration. The core logic is [here](https://github.com/filecoin-project/go-state-types/blob/master/builtin/v10/migration/top.go#L28).

Forks migrations: handled by [fork.go](https://github.com/filecoin-project/lotus/blob/58900a70333a11a903cf9fe3f29e6a5c309cb000/chain/stmgr/forks.go#L42-L53) entities.

### Where to upgrade?


### Challenges
- Doing the state migration efficiently; we need to traverse every entry in the state trie. Lotus does pre-migration which are filling relevant caches to speed up the eventual full migration at the upgrade epoch. We might need to do something like this as well; it might not be necessary for the first iteration - depends on how performant the migration process would be in the Forest itself.
- No test network. While we can use existing snapshots from before the upgrade to test state migration, it is not sustainable if we want to continuously support calibration network. We either require a local devnet for testing migration **before** they actually happen on real networks or we can try supporting more bleeding-edge networks. The former approach is more solid, but the latter might be easier to implement at first (and would give Forest more testnets support which is always welcome).



Sources:
- Rahul's article: https://hackmd.io/@tbdrqGmwSXiPjxgteK3hMg/r1D6cVM_u
- Lotus codebase - https://github.com/filecoin-project/lotus
- Venus codebase - https://github.com/filecoin-project/venus