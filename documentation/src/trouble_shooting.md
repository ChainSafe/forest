# Trouble Shooting

## Common Issues

#### File Descriptor Limits

By default, Forest will use large database files (roughly 1GiB each). Lowering
the size of these files lets RocksDB use less memory but runs the risk of
hitting the open-files limit. If you do hit this limit, either increase the file
size or use `ulimit` to increase the open-files limit.
