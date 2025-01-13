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

- adding the bundle cid to the `HeightInfos` struct in the network definitions
  files (e.g.,
  [calibnet](https://github.com/ChainSafe/forest/blob/main/src/networks/calibnet/mod.rs)).

```rust
HeightInfo {
    height: Height::Hygge,
    epoch: 322_354,
    bundle: Some(Cid::try_from("bafy2bzaced25ta3j6ygs34roprilbtb3f6mxifyfnm7z7ndquaruxzdq3y7lo").unwrap()),
}
```

- adding the bundle manifest cid and url to the `ACTOR_BUNDLES` in the
  `src/networks/actors_bundle.rs`.

- ensuring the bundle is mirrored in Forest's DO space under
  `https://forest-snapshots.fra1.cdn.digitaloceanspaces.com/actors/`.

```rust
"bafy2bzacecnhaiwcrpyjvzl4uv4q3jzoif26okl3m66q3cijp3dfwlcxwztwo" @ "v11.0.0" for "mainnet",
},
```

### Implement the migration

The next step is to implement the migration itself. In this guide, we will take
the `translate Go code into Rust` approach. It's not the cleanest way to do it,
but it's the easiest. Note that the Forest state migration design is not the
same as the Lotus one (we tend to avoid code duplications), so we must be
careful when translating the code.

#### Create the migration module

Create the nvXX migration module in the
[state migration module](https://github.com/ChainSafe/forest/tree/main/src/state_migration).
A valid approach is just to copy-paste the previous migration module and modify
it accordingly. The files that will most likely be present:

- `mod.rs`: here we bundle our migration modules and export the final migration
  function, defining the state types before and after migration, implementing
  the common system migrator and the verifier
- `migration.rs`: the heart of the migration. Here we add the migration logic
  for each actor. Its Go equivalent is the
  [top.go](https://github.com/filecoin-project/go-state-types/blob/master/builtin/v10/migration/top.go),
  in case of NV18,

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
    .get_cbor::<SystemStateOld>(&system_actor.state)?
    .ok_or_else(|| anyhow!("system actor state not found"))?;

let current_manifest = Manifest::load_with_actors(&store, &system_actor_state.builtin_actors, 1)?;

let new_manifest = Manifest::load(&store, &new_manifest, version)?;

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

```rust
for (name, code) in current_manifest.builtin_actors() {
    let new_code = new_manifest.code_by_name(name)?;
    self.add_migrator(*code, nil_migrator(*new_code));
}
```

For each actor with non-trivial migration logic, we add the migration function.
For example, for the `init` actor, we have:

```rust
self.add_migrator(
  *current_manifest.get_init_code(),
  init::init_migrator(*new_manifest.get_init_code()),
);
```

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
self.add_post_migrator(Arc::new(EamPostMigrator));

self.add_post_migrator(Arc::new(EthAccountPostMigrator));
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

### Test first development

When the Go migration code to translate from is large(e.g. nv17), it makes
development much easier to be able to attach debuggers. Follow below steps to
create simple unit tests for both Rust and Go with real calibnet or mainnet data
and attach debuggers when needed during development.

- Get input state cid. Run
  `forest --chain calibnet --encrypt-keystore false --halt-after-import --height=-200 --import-snapshot <SNAPSHOT>`,
  the input state cid will be in the failure messages `Previous state: <CID>`.
  And the expected output state cid can be found in state mismatch error
  messages.
- Export input state by running
  `forest-cli state fetch <PREVIOUS_STATE_CID> <PREVIOUS_STATE_CID>.car`
- Compress the car file by running `zstd <PREVIOUS_STATE_CID>.car`
- Move the compressed car file to data folder `src/state_migration/tests/data`
- Create a Rust test in `src/state_migration/tests/mod.rs`. Note: the output CID
  does not need to be correct to attach a debugger during development.

  Example test for nv17 on calibnet:

  ```rust
  #[tokio::test]
  async fn test_nv17_state_migration_calibnet() -> Result<()> {
      test_state_migration(
          Height::Shark,
          NetworkChain::Calibnet,
          Cid::from_str("bafy2bzacedxtdhqjsrw2twioyaeomdk4z7umhgfv36vzrrotjb4woutphqgyg")?,
          Cid::from_str("bafy2bzacecrejypa2rqdh3geg2u3qdqdrejrfqvh2ykqcrnyhleehpiynh4k4")?,
      )
      .await
  }
  ```

- Create a Go test in `src/state_migration/go-test/state_migration_test.go`.
  Note: `newManifestCid` is the bundle CID, epoch is the height that migration
  happens.
  [Instruction](https://code.visualstudio.com/docs/languages/go#_debugging) on
  debugging Go code in VS Code.

  Example test for nv17 on calibnet:

  ```go
  func TestStateMigrationNV17(t *testing.T) {
    startRoot := cid.MustParse("bafy2bzacedxtdhqjsrw2twioyaeomdk4z7umhgfv36vzrrotjb4woutphqgyg")
    newManifestCid := cid.MustParse("bafy2bzacedbedgynklc4dgpyxippkxmba2mgtw7ecntoneclsvvl4klqwuyyy")
    epoch := abi.ChainEpoch(16800)

    bs := migration9Test.NewSyncBlockStoreInMemory()
    ctx := context.Background()

    loadCar(t, ctx, bs, fmt.Sprintf("%s/.local/share/forest/bundles/calibnet/bundle_Shark.car", os.Getenv("HOME")))
    loadCompressedCar(t, ctx, bs, fmt.Sprintf("../data/%s.car.zst", startRoot))

    runStateMigration(t, ctx, cbor.NewCborStore(bs), startRoot, newManifestCid, epoch)
  }
  ```

### Performance considerations

Mainnet upgrades can take a long time even on a powerful machine, due to the
size of the state. It's useful to test beforehand how long (roughly) will a
migration take to better prepare ourselves and signal potential issues to other
implementation teams.

One _trick_ to test this is to:

- download any recent mainnet snapshot,
- change the schedule for the migration to happen 10 epochs before the
  snapshot's height,
- import the snapshot (on a clean database) and validate the migration epoch (or
  a range including that epoch)

For example, with the scheduled migration at 3411547, we can run:

```
forest --encrypt-keystore false --import-snapshot forest_snapshot_mainnet_2023-11-22_height_3411557.forest.car.zst --height=-20
```

While the migration itself should succeed, there will be a state mismatch
afterwards. This is not an issue.

```
2023-12-05T15:46:37.988136Z  INFO forest::state_migration: State migration at height Watermelon(epoch 3411547) was successful, Previous state: bafy2bzacedqswtcnhub5ea6upcjp4s7ghba5lgxri7ckezgsdxbkgnh6oyz3w, new state: bafy2bzacecxvz7jl3pt3ki4cirp4arfbmdxxcdb2ni4mzhkbbxqaug5z747gu, new state actors: bafy2bzaceb53kdtubm74czvthzah5inpejrrw7tdueajuhp3n7pbirzjwpqok. Took: 349.9679s.
2023-12-05T15:46:42.438174Z ERROR forest::state_manager: state mismatch height=3411549 expected_state=Cid(bafy2bzacecr6ll3w6kb5cyvcsl2e5z6wqrbhaxntzaabkbqikmhuj5a7ukbxk) expected_receipt=Cid(bafy2bzacebhp2zlhpabxgquiht7cu5rqug5sxtxyfadkiijdpaxmcrhdyfs3s) actual_state=Cid(bafy2bzacecjb4tc4hub2yytxdsr7kpozdabufgsvdixqkkllg3yqv7zxujs2g) actual_receipt=Cid(bafy2bzaceaqdlwllmddokd5izwvf7isqlzglueqcw62ttyn5j3nx2hzk4ecwg)
```

While the resulting state might be incorrect (not matching what Lotus
calculated), at least we verify that the migration isn't causing OOMs and takes
reasonable amount of time.

### Future considerations

- Grab the actor bundles from the IPFS. This would make Forest less dependent on
  the Github infrastructure.
  [Issue #2765](https://github.com/ChainSafe/forest/issues/2765)
- Consider pre-migrations as Lotus does. It is not needed at the moment (the
  mainnet upgrade takes several seconds at most) but may become a bottleneck if
  the migration is too heavy.
