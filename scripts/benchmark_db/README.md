# Forest benchmark db script

This script was developed to help with testing of Forest db backends and their
configuration; the script now also allows benchmarking ("daily" benchmarks) of
Forest and Lotus snapshot import times (in sec) and validation times (in
tipsets/sec).

## Install dependencies

[Install Ruby](https://www.ruby-lang.org/en/documentation/installation/) first.
Then go into `scripts/benchmark_db` and execute the following commands:

```
$ bundle config set --local path 'vendor/bundle'
$ bundle install
```

Note: depending upon your Ruby installation, it may be necessary to execute
`gem install bundler` first. In case of any issues with "native extensions"
during `bundle install` on a \*nix machine, it may also be necessary to execute
`apt-get update && apt-get install -y build-essential ruby-dev`.

The daily benchmarks also require the installation of
[aria2](https://github.com/aria2/aria2) and
[zstd](https://github.com/facebook/zstd), as well as dependencies required for
the installation of [Forest](https://github.com/ChainSafe/forest) and
[Lotus](https://github.com/filecoin-project/lotus) (note that the script handles
installation of the Forest and Lotus binaries).

## Run benchmarks

Run the script at the root of the repository. I.e.,:

```
$ ./scripts/benchmark_db/bench.rb <path to snapshot> <optional flags>
```

If the user does not specify a path to a snapshot, the script will automatically
download a fresh snapshot, then pause for 5 minutes to allow the network to
advance to ensure that enough time will be spent in the `message sync` stage for
proper calculation of the validation time metric. Also note that if `--chain` is
specified, the user must provide a script matching the specified `<chain>` (the
script defaults to `mainnet`, so if `--chain` is not specified, provide a
`mainnet` snapshot).

If the `--daily` flag is included in the command line arguments, the script will
run the daily benchmarks specified earlier; otherwise the script will run the
backend metrics.

On many machines, running the script with `--chain mainnet` may require more
space than allocated to the `tmp` partition. To address this, specify the
`--tempdir` flag with a user-defined directory (which will automatically be
created if it does not already exist).

To create a selection of benchmarks, use the `--pattern` flag (current defined
patterns are `'*'`, `'baseline'`, `'jemalloc'`, and `'mimalloc'`). Using
`--dry-run` outputs to the terminal the commands the script will run (without
actually running the commands):

```
$ ./scripts/benchmark_db/bench.rb <path to snapshot> --chain calibnet --pattern jemalloc --dry-run
(I) Using snapshot: <path to snapshot>
(I) WORKING_DIR: <generated directory>

(I) Running bench: jemalloc
(I) Building artefacts...
(I) Cloning repository
$ git clone https://github.com/ChainSafe/forest.git forest
(I) Clean and build client
$ cargo clean
$ cargo build --release --no-default-features --features forest_fil_cns,jemalloc
$ ./forest/target/release/forest --config <tbd> --encrypt-keystore false --import-snapshot <tbd> --halt-after-import
$ ./forest/target/release/forest --config <tbd> --encrypt-keystore false --import-snapshot <tbd> --halt-after-import --skip-load=true --height <height>
(I) Clean db
$ ./forest/target/release/forest-cli -c <path>
toml db clean --force

(I) Wrote result_<time>.md
```

```
$ ./scripts/benchmark_db/bench.rb <path to snapshot> --chain calibnet --dry-run --daily
(I) Using snapshot: <path to snapshot>
(I) WORKING_DIR: <generated directory>

(I) Running bench: forest
(I) Building artefacts...
(I) Cloning repository
$ git clone https://github.com/ChainSafe/forest.git forest
(I) Clean and build client
$ cargo clean
$ cargo build --release
$ ./forest/target/release/forest-cli fetch-params --keys
$ ./forest/target/release/forest --config <tbd> --encrypt-keystore false --import-snapshot <tbd> --halt-after-import
$ ./forest/target/release/forest --config <tbd> --encrypt-keystore false
(I) Clean db
$ ./forest/target/release/forest-cli -c <path> db clean --force

(I) Running bench: lotus
(I) Building artefacts...
(I) Cloning repository
$ git clone https://github.com/filecoin-project/lotus.git lotus
(I) Clean and build client
$ make clean
$ make calibnet
$ ./lotus/lotus daemon --import-snapshot <tbd> --halt-after-import
$ ./lotus/lotus daemon
(I) Clean db

Wrote result_<time>.csv
```

As seen in these examples, if `--daily` is passed in the command line, daily
benchmark results are written to a CSV in the current directory with naming
format `result_<time>.csv`. Otherwise, backend benchmark results will be written
to a markdown file with a similar naming convention.
