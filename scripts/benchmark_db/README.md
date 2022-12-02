# Forest benchmark db script

This script is here to help with testing of Forest db backends and their
configuration.

## Install dependencies

You will need to install Ruby first. Then go into `benchmark_db` and type:

```
$ bundle install
```

## Run benchmarks

You need to run the script at the root of the repository. Ie:

```
$ ./benchmark_db/bench.rb 2369040_2022_11_25t12_00_00z.car
```

You can create a selection of benchmarks using the `--pattern` flag. If used in conjunction with `--dry-run` you will see what commands will be run:

```
$ ./scripts/benchmark_db/bench.rb 2369040_2022_11_25t12_00_00z.car --pattern 'baseline*,paritydb' --dry-run
Running bench: baseline
$ cargo clean
$ cargo build --release
$ ./target/release/forest --config <tbd> --encrypt-keystore false --import-snapshot <tbd> --halt-after-import
$ ./target/release/forest --config <tbd> --encrypt-keystore false --import-snapshot <tbd> --halt-after-import --skip-load --height 2368640
Wiping db

Running bench: baseline-with-stats
$ cargo clean
$ cargo build --release
$ ./target/release/forest --config <tbd> --encrypt-keystore false --import-snapshot <tbd> --halt-after-import
$ ./target/release/forest --config <tbd> --encrypt-keystore false --import-snapshot <tbd> --halt-after-import --skip-load --height 2368640
Wiping db

Running bench: paritydb
$ cargo clean
$ cargo build --release --no-default-features --features forest_fil_cns,paritydb
$ ./target/release/forest --config <tbd> --encrypt-keystore false --import-snapshot <tbd> --halt-after-import
$ ./target/release/forest --config <tbd> --encrypt-keystore false --import-snapshot <tbd> --halt-after-import --skip-load --height 2368640
Wiping db
```

Benchmark results will be written to a markdown file at the end.
