# Trouble Shooting

## Common Issues

#### File Descriptor Limits

By default, Forest will use large database files (roughly 1GiB each). Lowering
the size of these files lets RocksDB use less memory but runs the risk of
hitting the open-files limit. If you do hit this limit, either increase the file
size or use `ulimit` to increase the open-files limit.

#### Jemalloc issues on Apple Silicon macs

Forest is compiled with `jemalloc` as a default allocator. If you are having
problems running it, perhaps this checklist would help:

1. Make sure you are using an arm64 version of homebrew, this could be a problem
   one inherits when migrating from an Intel Mac to Apple Silicon:
   [Stackoverflow example](https://stackoverflow.com/a/68443301).
2. Make sure your default host is set to `aarch64-apple-darwin` via
   `rustup set default-host aarch64-apple-darwin`.
3. This could result in various errors related to the fact that you still have
   some of the libraries symlinked to `/usr/local/lib` from an intel Homebrew
   version, this can be fixed by manually removing those and linking the correct
   libraries from `/opt/homebrew/Cellar` or referencing `/opt/homebrew/lib`.
   Note that you might need to install those libraries first using your arm64
   Homebrew.
