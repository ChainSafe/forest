# Trouble Shooting

## Common Issues

#### Jemalloc issues on Apple Silicon macs

Forest is compiled with `jemalloc` as a default allocator. If you are having
problems running or compiling Forest, use this checklist:

1. Make sure you are using an arm64 version of homebrew; this could be a problem
   one inherits when migrating from an Intel Mac to Apple Silicon:
   [Stackoverflow example](https://stackoverflow.com/a/68443301).
2. Make sure your default host is set to `aarch64-apple-darwin` via
   `rustup set default-host aarch64-apple-darwin`.
3. This could result in various errors related to the fact that you still have
   some of the libraries symlinked to `/usr/local/lib` from an intel Homebrew
   installation. The easiest fix for this is:
   - Remove the libraries in question from `/usr/local/lib`.
   - Add `export LIBRARY_PATH=/opt/homebrew/lib` to your bash profile.
   - Source the new bash profile.
