# Trouble Shooting

## Common Issues

#### File Descriptor Limits

To properly manage the Filecoin state tree with RocksDB, Forest requires a custom allocation of file descriptors. Please set the process limits to `1048576`.