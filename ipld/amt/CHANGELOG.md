# 1.0.0 [UNRELEASED]

- Dynamic bit width functionality
    - Updates serialized format, this is a breaking change
- Removes inefficient caching, under the `go-interop` feature flag.
- Fixes expand and collapse bug written into actors v0 and v2
- v3 Actors compatible Amt
- Changed index to usize for more idiomatic usages
- Updated `MAX_INDEX` to take full advantage of the range of values.

# 0.2.0 [2021-02-02]

- Switched `new_from_slice` function to `new_from_iter` which takes a generic type which can be converted into an iterator of the Amt type, avoiding clones.