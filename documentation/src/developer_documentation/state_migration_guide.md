## State migration guide ⏩

This guide is intended to help to implement new state migration in the future.
It will be based on the current state migration implementation for NV18 and
NV19.

### State migration requirements

- The proper actor bundle is released for at least the test network. It should
  be available on the
  [actor bundles repository](https://github.com/filecoin-project/builtin-actors/releases).
  You can verify which upgrade needs which bundle in the
  [network upgrade matrix](https://github.com/filecoin-project/core-devs/tree/master/Network%20Upgrades).
- The state migration should be implemented in the
  [Go library](https://github.com/filecoin-project/go-state-types/tree/master/builtin).
  This is the source of truth for the state migration. Also, we should carefully
  analyze the FIPs and implement the migration based on them. In case of doubt,
  we should always consider the FIPs as the source of truth and reach out to the
  Lotus team if we find potential issues in their implementation.

### Development

#### Import the actor bundle

The first step is to import the actor bundle into Forest. This is done by:

- adding the bundle to the `HeightInfos` struct in the network definitions files
  (e.g.,
  [calibnet](https://github.com/ChainSafe/forest/blob/main/networks/src/calibnet/mod.rs)).

```rust
HeightInfo {
    height: Height::Hygge,
    epoch: 322_354,
    bundle: Some(ActorBundleInfo {
        manifest: Cid::try_from("bafy2bzaced25ta3j6ygs34roprilbtb3f6mxifyfnm7z7ndquaruxzdq3y7lo").unwrap(),
        url: Url::parse("https://github.com/filecoin-project/builtin-actors/releases/download/v10.0.0-rc.1/builtin-actors-calibrationnet.car").unwrap()
})
```

- Adding the download at the proper height to the `load_bundles` function in the
  [daemon](https://github.com/ChainSafe/forest/blob/main/forest/daemon/src/bundle/mod.rs).
  This step could be potentially done automatically in the future.

```rust
if epoch < config.chain.epoch(Height::Hygge) {
    bundles.push(get_actors_bundle(config, Height::Hygge).await?);
}
```

### Implement the migration

The next step is to implement the migration itself. In this guide, we will take
the `translate Go code into Rust` approach. It's not the cleanest way to do it,
but it's the easiest. Note that the Forest state migration design is not the
same as the Lotus one (we tend to avoid code duplications), so we must be
careful when translating the code.

#### Create the migration module

Create the migration module in the
[state migration crate](https://github.com/ChainSafe/forest/tree/main/vm/state_migration/src).
A valid approach is just to copy-paste the previous migration module and modify
it accordingly. The files that will most likely be present:

- `mod.rs`: here we bundle our migration modules and export the final migration
  function,
- `system.rs`: here we define the system actor migration logic which (so far)
  seems to not change between upgrades,
- `migration.rs`: the heart of the migration. Here we add the migration logic
  for each actor. Its Go equivalent is the
  [top.go](https://github.com/filecoin-project/go-state-types/blob/master/builtin/v10/migration/top.go),
  in case of NV18,
- `verifier.rs`: checks for the migration definition.

We will most likely need as many custom migrators as there are in the Go
implementation. In other terms, if you see that the Go
[migration](https://github.com/filecoin-project/go-state-types/tree/master/builtin/v10/migration)
contains:

- `eam.go` - Ethereum Account Manager migration,
- `init.go` - Init actor migration,
- `system.go` - System actor migration,

Then our implementation will need to define those as well.

#### The actual migration

This part will largely depend on the complexity of the network upgrade itself.
The goal is to translate the `MigrateStateTree` method from
[Go](https://github.com/filecoin-project/go-state-types/blob/master/builtin/v10/migration/top.go#L28)
to the `add_nvXX_migrations` in the `migration.rs` file. The
`add_nvXX_migrations` method is responsible for adding all the migrations that
are needed for the network upgrade and the logic in between. Note that the
Forest version is much simpler as it doesn't contain the migration `engine`
(implemented in the base module).

The first thing to do is to get the current system actor state and the current
manifest. Then we will map the old actor codes to the new ones.

```rust
let state_tree = StateTree::new_from_root(store.clone(), state)?;
let system_actor = state_tree
    .get_actor(&Address::new_id(0))?
    .ok_or_else(|| anyhow!("system actor not found"))?;

let system_actor_state = store
    .get_obj::<SystemStateV10>(&system_actor.state)?
    .ok_or_else(|| anyhow!("system actor state not found"))?;
let current_manifest_data = system_actor_state.builtin_actors;
let current_manifest = Manifest::load(&store, &current_manifest_data, 1)?;

let (version, new_manifest_data): (u32, Cid) = store
    .get_cbor(new_manifest)?
    .ok_or_else(|| anyhow!("new manifest not found"))?;
let new_manifest = Manifest::load(&store, &new_manifest_data, version)?;

```

⚠️ Stay vigilant! The `StateTree` versioning is independent of the network and
actor versioning. At the time of writing, the following holds:

- `StateTreeVersion0` - Actors version < v2
- `StateTreeVersion1` - Actors version v2
- `StateTreeVersion2` - Actors version v3
- `StateTreeVersion3` - Actors version v4
- `StateTreeVersion4` - Actors version v5 up to v9
- `StateTreeVersion5` - Actors version v10 and above These are not compatible
  with each other and when using a new FVM, we can only use the latest one.

For actors that don't need any state migration, we can use the `nil_migrator`.

````rust
current_manifest.builtin_actor_codes().for_each(|code| {
let id = current_manifest.id_by_code(code);
let new_code = new_manifest.code_by_id(id).unwrap();
self.add_migrator(*code, nil_migrator(*new_code));
});

For each actor with non-trivial migration logic, we add the migration function. For example, for the `init` actor, we have:
```rust
self.add_migrator(
*current_manifest.get_init_code(),
init::init_migrator(*new_manifest.get_init_code()),
);
````

and we define the `init_migrator` in a separate module. This logic may include
setting some defaults on the new fields, changing the current ones to an
upgraded version and so on.

#### Verifier

An optional (but recommended) piece of code that performs some sanity checks on
the state migration definition. At the time of writing, it checks that all
builtin actors are assigned a migration function.

```rust
let verifier = Arc::new(Verifier::default());
```

#### Post-migration actions

Some code, like creating an entirely new actor (in the case of NV18 creating EAM
and Ethereum Account actors), needs to be executed post-migration. This is done
in the post-migration actions.

```rust
let post_migration_actions = [create_eam_actor, create_eth_account_actor]
    .into_iter()
    .map(|action| Arc::new(action) as PostMigrationAction<DB>)
    .collect();
```

#### Creating the migration object and running it

We take all the migrations that we have defined previously, all the
post-migration actions, and create the migration object.

```rust
let mut migration = StateMigration::<DB>::new(Some(verifier), post_migration_actions);
migration.add_nv18_migrations(blockstore.clone(), state, &new_manifest_cid)?;

let actors_in = StateTree::new_from_root(blockstore.clone(), state)?;
let actors_out = StateTree::new(blockstore.clone(), StateTreeVersion::V5)?;
let new_state =
migration.migrate_state_tree(blockstore.clone(), epoch, actors_in, actors_out)?;

Ok(new_state)
```

The new state is the result of the migration.

### Use the migration

After completing the migration, we need to invoke it at the proper height. This
is done in the `handle_state_migrations` method in the
[state manager](https://github.com/ChainSafe/forest/blob/main/blockchain/state_manager/src/lib.rs).
This step could be potentially done automatically in the future.

### Testing

We currently lack a framework for properly testing the network upgrades before
they actually happen. This should change in the future.

For now, we can do it using a snapshot generated after the network upgrade,
e.g., 100 epochs after and validating previous epochs which should include the
upgrade height.

```shell
forest --chain calibnet --encrypt-keystore false --halt-after-import --height=-200 --import-snapshot <SNAPSHOT>
```

### Future considerations

- Testing without the need for a snapshot or a running node. This would allow us
  to test the network upgrade in a more isolated way. See how it is done in the
  [Go library](https://github.com/filecoin-project/go-state-types/blob/master/builtin/v9/migration/test/migration_test.go).
- Grab the actor bundles from the IPFS. This would make Forest less dependent on
  the Github infrastructure.
  [Issue #2765](https://github.com/ChainSafe/forest/issues/2765)
- Consider pre-migrations as Lotus does. It is not needed at the moment (the
  mainnet upgrade takes several seconds at most) but may become a bottleneck if
  the migration is too heavy.
