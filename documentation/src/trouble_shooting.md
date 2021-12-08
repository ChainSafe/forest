# Trouble Shooting

## Common Issues

#### File Descriptor Limits

To properly manage the Filecoin state tree with RocksDB, Forest requires a custom allocation of file descriptors. Please set the process file descriptor limits to `1048576` to avoid stalling the node.

For MacOS and Linux distributions, this can be done by using the `usize` command. Using this command will set it for the current session. On MacOS, setting the limit too high will default it to a max of 
`12288`. There are many guides online to override this along with specific Linux guides to make the new max persistent.