# Trouble Shooting

## Common Issues

#### File Descriptor Limits

To properly manage the Filecoin state tree with RocksDB, Forest requires a custom allocation of file descriptors. Please set the process file descriptor limits to `1048576` to avoid stalling the node.