# Cleaning

    rm -rf ~/.genesis-sectors/ ~/.lotus-local-net/ ~/.lotus-miner-local-net/

# Running the node:

    export LOTUS_PATH=~/.lotus-local-net
    export LOTUS_MINER_PATH=~/.lotus-miner-local-net
    export LOTUS_SKIP_GENESIS_CHECK=_yes_
    export CGO_CFLAGS_ALLOW="-D__BLST_PORTABLE__"
    export CGO_CFLAGS="-D__BLST_PORTABLE__"
    make 2k
    ./lotus fetch-params 2048
    ./lotus-seed pre-seal --sector-size 2KiB --num-sectors 2
    ./lotus-seed genesis new localnet.json
    ./lotus-seed genesis add-miner localnet.json ~/.genesis-sectors/pre-seal-t01000.json
    ./lotus daemon --lotus-make-genesis=devgen.car --genesis-template=localnet.json --bootstrap=false
    # Keep this terminal open

# Running the miner:

    export LOTUS_PATH=~/.lotus-local-net
    export LOTUS_MINER_PATH=~/.lotus-miner-local-net
    export LOTUS_SKIP_GENESIS_CHECK=_yes_
    export CGO_CFLAGS_ALLOW="-D__BLST_PORTABLE__"
    export CGO_CFLAGS="-D__BLST_PORTABLE__"
    ./lotus wallet import --as-default ~/.genesis-sectors/pre-seal-t01000.key
    ./lotus-miner init --genesis-miner --actor=t01000 --sector-size=2KiB --pre-sealed-sectors=~/.genesis-sectors --pre-sealed-metadata=~/.genesis-sectors/pre-seal-t01000.json --nosync
    ./lotus-miner run --nosync
    # Keep this terminal open

# Helpers:

    ./lotus-miner info
    ./lotus-miner sectors list

# Send data to miner:

    ./lotus client query-ask t01000
    ./lotus client import LICENSE-APACHE
    ./lotus client deal
    ./lotus client retrieve [CID from import] test.txt # data has to be on chain first

# Get data on chain:

    ./lotus-miner storage-deals pending-publish --publish-now
    ./lotus-miner sectors seal 2
    ./lotus-miner sectors batching precommit --publish-now
    ./lotus-miner sectors batching commit --publish-now

# Retrieve:

    ./lotus client local
    ./lotus client retrieve --provider t01000 [CID from import] outputfile.txt
