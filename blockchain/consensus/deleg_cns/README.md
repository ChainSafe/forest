# Delegated Consensus

_Delegated Consensus_ is a simplistic consensus protocol made for demo purposes, where only a single miner is allowed to propose blocks - this miner is who the others "delegate to".

## Setup Genesis

To get started, we need to generate a `genesis.car` file that contains the public key of this miner, so that participants can validate that the blocks they receive are signed by the expected miner.

At the time of this writing _Forest_ doesn't have the necessary machinery to prepare its own `genesis.car` file (although it has [commands](https://github.com/ChainSafe/forest/blob/3d149dcd7cfd23a8ce2793e107a4c7db20948584/forest/src/cli/genesis_cmd.rs#L19) to prepare a template for it), so for the time being we have to rely on [Lotus CLI](https://github.com/filecoin-project/lotus/tree/v1.17.0-rc3/cmd) to do this for us.


### Build Lotus

Ideally we could avoid having to build Lotus just for this purpose, and use the published [Docker image](https://hub.docker.com/r/filecoin/lotus). However, not all commands that we need are part of the image; for example `lotus-seed` is only available in the [lotus-test](https://github.com/filecoin-project/lotus/blob/v1.17.0-rc3/Dockerfile.lotus#L232) build target.

Still, we can use docker to build the CLI without having to install all necessary build dependencies ourselves. Assuming that we already have [Lotus](https://github.com/filecoin-project/lotus) cloned from Github and we are in the top level `lotus` directory, we can use the following command to build an image we need:

```bash
docker build -t filecoin/lotus-test -f Dockerfile.lotus --target lotus-test .
```

### Generate signing keys

According to the [spec](https://spec.filecoin.io/#section-systems.filecoin_nodes.repository.key_store) we need to generate a BLS key for signing blocks, a.k.a. the "worker" key.

We could use the [lotus-keygen](https://github.com/filecoin-project/lotus/tree/v1.17.0-rc3/cmd/lotus-keygen) command to generate Secp256k1 or BLS keys, but unfortunately this command is not copied into the Docker image we built. However, that's not a problem in this case becase the `lotus-seed` command will generate a key for us, if it's not given an existing one.

### Generate the pre-seal file

Lotus will generate quite a few different files, so let's create a directory to put them into.

```bash
mkdir -p genesis-files
```

The first file we generate will be the `preseal.json`, which contains the worker keys to that we want to put in the `genesis.json` file later.

```bash
docker run -it --rm \
  -v $PWD/genesis-files:/out \
  --user $(id -u):$(id -g) \
  --entrypoint lotus-seed \
  filecoin/lotus-test \
    --sector-dir /out \
    pre-seal --miner-addr t01000
```

Note that we used `t01000` as the miner address, which is also the default, and practically the only one we can use (besides its mainnet variant) as the ID of the first miner, because Lotus insists the numbering starts from `1000`.

Let's see what we have:

```console
$ ls genesis-files
cache  pre-seal-t01000.json  pre-seal-t01000.key  sealed  sectorstore.json  unsealed  update  update-cache
```

The [pre-seal command](https://github.com/filecoin-project/lotus/blob/v1.17.0-rc3/cmd/lotus-seed/main.go#L59) generated two a `pre-seal-t01000.json` file that we have to merge into a Genesis template, and also a `pre-seal-t01000.key` file, which contains a [generated BLS key](https://github.com/filecoin-project/lotus/blob/9794652e0be9a7709934580f36a4eeaa894ea2ad/cmd/lotus-seed/seed/seed.go#L93) that we will use to sign blocks later.

We can ignore the rest of the files because we will not be doing any actual Filecoin mining.

### Generate a Genesis template

The next command we need is [genesis new](https://github.com/filecoin-project/lotus/blob/v1.17.0-rc3/cmd/lotus-seed/genesis.go#L48) to get ourselves a `genesis.json` file.


```bash
docker run -it --rm \
  -v $PWD/genesis-files:/out \
  --user $(id -u):$(id -g) \
  --entrypoint lotus-seed \
  filecoin/lotus-test \
    --sector-dir /out \
    genesis new "/out/genesis.json"
```

### Add Miner

Next we have to add the the miner we created earlier to the Genesis template using the [genesis add-miner](https://github.com/filecoin-project/lotus/blob/v1.17.0-rc3/cmd/lotus-seed/genesis.go#L90) command.

```bash
docker run -it --rm \
  -v $PWD/genesis-files:/out \
  --user $(id -u):$(id -g) \
  --entrypoint lotus-seed \
  filecoin/lotus-test \
    --sector-dir /out \
    genesis add-miner "/out/genesis.json" "/out/pre-seal-t01000.json"
```

The command modifies `genesis.json` in place and appends to the `Accounts` and `Miners` collections, and also adds an initial balance of 50,000,000 to the miner Actor.

Let's see what we have in the end:

```console
$ cat genesis-files/genesis.json
{
  "NetworkVersion": 16,
  "Accounts": [
    {
      "Type": "account",
      "Balance": "50000000000000000000000000",
      "Meta": {
        "Owner": "t3w3sv7uykya4vkzk77d2h7lfuldtaa5rr3nvzh6fkutu5lesdwefgnqm2u3obpyrhhzbupl42kubzqmvfivjq"
      }
    }
  ],
  "Miners": [
    {
      "ID": "t01000",
      "Owner": "t3w3sv7uykya4vkzk77d2h7lfuldtaa5rr3nvzh6fkutu5lesdwefgnqm2u3obpyrhhzbupl42kubzqmvfivjq",
      "Worker": "t3w3sv7uykya4vkzk77d2h7lfuldtaa5rr3nvzh6fkutu5lesdwefgnqm2u3obpyrhhzbupl42kubzqmvfivjq",
      "PeerId": "12D3KooWNuDNyfRb4iLvnwFqNifFgJT8XSGBVAjduKQYD9LhhrkL",
      "MarketBalance": "0",
      "PowerBalance": "0",
      "SectorSize": 2048,
      "Sectors": [
        {
          "CommR": {
            "/": "bagboea4b5abcbkgmygsqqyg4p3dg2hdvcpkfsvae2l4lroxbvkj5remofegxyryv"
          },
          "CommD": {
            "/": "baga6ea4seaqemcrx5m3n7udmoz7x53aae5wnapqsxkk5n5ysvled3hwnoqm7wda"
          },
          "SectorID": 0,
          "Deal": {
            "PieceCID": {
              "/": "baga6ea4seaqemcrx5m3n7udmoz7x53aae5wnapqsxkk5n5ysvled3hwnoqm7wda"
            },
            "PieceSize": 2048,
            "VerifiedDeal": false,
            "Client": "t3w3sv7uykya4vkzk77d2h7lfuldtaa5rr3nvzh6fkutu5lesdwefgnqm2u3obpyrhhzbupl42kubzqmvfivjq",
            "Provider": "t01000",
            "Label": "0",
            "StartEpoch": 0,
            "EndEpoch": 9001,
            "StoragePricePerEpoch": "0",
            "ProviderCollateral": "0",
            "ClientCollateral": "0"
          },
          "DealClientKey": {
            "Type": "bls",
            "PrivateKey": "NOrAYwVSGwX2vtQ9MMySUV8YFxKiO97mGRALrejE/Tk=",
            "PublicKey": "tuVf0wrAOVVlX/j0f6y0WOYAdjHba5P4qqTp1ZJDsQpmwZqm3BfiJz5DR6+aVQOY",
            "Address": "t3w3sv7uykya4vkzk77d2h7lfuldtaa5rr3nvzh6fkutu5lesdwefgnqm2u3obpyrhhzbupl42kubzqmvfivjq"
          },
          "ProofType": 5
        }
      ]
    }
  ],
  "NetworkName": "localnet-d39f7d65-d9c6-4e2e-8129-6f13dc096ea9",
  "VerifregRootKey": {
    "Type": "multisig",
    "Balance": "0",
    "Meta": {
      "Signers": [
        "t1ceb34gnsc6qk5dt6n7xg6ycwzasjhbxm3iylkiy"
      ],
      "Threshold": 1,
      "VestingDuration": 0,
      "VestingStart": 0
    }
  },
  "RemainderAccount": {
    "Type": "multisig",
    "Balance": "0",
    "Meta": {
      "Signers": [
        "t1ceb34gnsc6qk5dt6n7xg6ycwzasjhbxm3iylkiy"
      ],
      "Threshold": 1,
      "VestingDuration": 0,
      "VestingStart": 0
    }
  }
}
```

### Generate a CAR file

All that's left is for the `genesis.json` file to be turned into `genesis.car` by running the [genesis car](https://github.com/filecoin-project/lotus/blob/v1.17.0-rc3/cmd/lotus-seed/genesis.go#L559) command, which will create the Genesis block and the initial State Tree, and save it in the IPLD format that Forest expects to be started with.

```bash
docker run -it --rm \
  -v $PWD/genesis-files:/out \
  --user $(id -u):$(id -g) \
  --entrypoint lotus-seed \
  filecoin/lotus-test \
    --sector-dir /out \
    genesis car --out "/out/genesis.car" "/out/genesis.json"
```

The log helpfully tells us what is happening:

```console
2022-07-19T13:38:55.057Z	WARN	genesis	testing/genesis.go:57	Generating new random genesis block, note that this SHOULD NOT happen unless you are setting up new network
init set t3w3sv7uykya4vkzk77d2h7lfuldtaa5rr3nvzh6fkutu5lesdwefgnqm2u3obpyrhhzbupl42kubzqmvfivjq t0100
init set t1ceb34gnsc6qk5dt6n7xg6ycwzasjhbxm3iylkiy t0101
[flexi_logger][ERRCODE::Time] flexi_logger has to work with UTC rather than with local time, caused by IndeterminateOffset
    See https://docs.rs/flexi_logger/latest/flexi_logger/error_info/index.html#time
publishing 1 storage deals on miner t01000 with worker t3w3sv7uykya4vkzk77d2h7lfuldtaa5rr3nvzh6fkutu5lesdwefgnqm2u3obpyrhhzbupl42kubzqmvfivjq
0.000001621003269618 FIL
2022-07-19T13:38:58.463Z	INFO	genesis	genesis/genesis.go:591	Empty Genesis root: bafy2bzacedswlcz5ddgqnyo3sak3jmhmkxashisnlpq6ujgyhe4mlobzpnhs6
GENESIS MINER ADDRESS: t01000
2022-07-19T13:38:58.468Z	WARN	genesis	testing/genesis.go:97	WRITING GENESIS FILE AT /out/genesis.car
```

and we have our Genesis file:

```console
$ ls -lh genesis-files/*.car
-rw-r--r-- 1 aakoshh aakoshh 5.3M Jul 19 14:38 genesis-files/genesis.car
```

If we look carefully at the log we can see that it says `init set t3w3...ivjq t0100`, which is _not_ the `t01000` ID we might have expected. This is because a miner has two Actors representing it in the system: one with a `miner::State` and another with an `account::State`; the latter is identified by `t0100`, which is the first ID issued by the system.

For now we work within the common validation framework of Forest, which expects to see a miner ID in the header, not the account ID, but this works differently in Eudico, where they use the account ID directly. Both can be resolved to a public key, to check the block signature.

### Clean Up

The `genesis.car` file will need to be present on each node we add to the network. The other one we need to keep is `pre-seal-t01000.key` but _only_ for the process that will do the block production. It's probably a good idea to keep `genesis.json` as well in case we want to remind ourselves about the public key. We don't strictly need it because we will use the miner ID (`t01000`) as a configuration value to each participant to let them know about which miner to expect to sign the blocks - the corresponding key will be looked up in the ledger.

The rest of the files can be deleted.

```bash
$ cd genesis-files && ls | grep -v pre-seal | grep -v genesis | xargs rm -rf && cd -
$ ls genesis-files
genesis.car  genesis.json  pre-seal-t01000.json  pre-seal-t01000.key
```

### All together now

The [scripts/generate-genesis-files.sh](scripts/generate-genesis-files.sh) file is a convenience script that runs all the above commands assuming that `lotus-seed` is on the `PATH`, which may be the case if we already have Lotus or Eudico installed. If so, we can run the script directly, giving it the output directory (which must exist) to write the genesis files to.

And similarly we can use the docker image we prepared here to run all the above commands as a single step:

```bash
docker run -it --rm \
  -v $PWD/genesis-files:/out \
  -v $PWD/scripts:/scripts \
  --user $(id -u):$(id -g) \
  --entrypoint /scripts/generate-genesis-files.sh \
  filecoin/lotus-test /out
```
